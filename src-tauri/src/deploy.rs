use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use chrono::Local;
use futures_util::future::join_all;
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Command as TokioCommand;

use crate::engine::download;
use crate::engine::whisper_cpp;

/// 写入持久化调试日志（与 commands::debug_log 同一文件）
fn log_event(label: &str, msg: &str) {
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] [deploy] {} {}\n", ts, label, msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/voice2text-debug.log")
    {
        let _ = f.write_all(line.as_bytes());
    }
    eprintln!("{}", line.trim());
}

/// Repo 路径模板（{owner}/{repo}）
/// whisper.cpp 已从 ggerganov 迁移到 ggml-org 组织
const REPO_OWNER: &str = "ggml-org";
const REPO_NAME: &str = "whisper.cpp";

/// Clone 镜像条目：(host显示名, 类型标记)
/// 类型标记: "direct" | "proxy:"前缀host | "rewrite:"替换host | "cloud:"云厂商host | "edu:"高校host
/// 架构：直接 GitHub → Git Clone 代理加速 → 云厂商镜像 → 高校镜像 → GitCode
const REPO_URLS: &[(&str, &str)] = &[
    // ── Tier 0: 直接 GitHub ──
    ("github.com",                "direct:github.com"),
    // ── Tier 1: Git Clone 专用代理加速（命令行拉代码首选） ──
    ("GitCode",                   "rewrite:gitcode.com"),        // 国内实测最快
    ("gh-proxy.com",              "proxy:gh-proxy.com"),
    ("ghproxy.com",               "proxy:ghproxy.com"),
    ("github.akams.cn",           "rewrite:github.akams.cn"),   // 全资源加速
    ("kgithub.com",               "rewrite:kgithub.com"),       // 域名替换
    // ── Tier 2: 云厂商企业级镜像 ──
    ("阿里云镜像",                 "cloud:mirrors.aliyun.com"),
    ("华为云镜像",                 "cloud:mirrors.huaweicloud.com"),
    ("腾讯云镜像",                 "cloud:mirrors.cloud.tencent.com"),
    ("微软 Azure 中国",            "cloud:mirror.azure.cn"),
    // ── Tier 3: 高校官方开源镜像（教育网优先，长期稳定） ──
    ("中科大 USTC",               "edu:mirrors.ustc.edu.cn"),
    ("上海交大 SJTUG",             "edu:mirrors.sjtug.sjtu.edu.cn"),
    ("北京外国语 BFSU",            "edu:mirrors.bfsu.edu.cn"),
    ("浙江大学 ZJU",               "edu:mirrors.zju.edu.cn"),
    ("哈工大 HIT",                "edu:mirrors.hit.edu.cn"),
    ("南京大学 NJU",               "edu:mirrors.nju.edu.cn"),
    ("山东大学 SDU",               "edu:mirrors.sdu.edu.cn"),
    ("南方科大 SUSTech",           "edu:mirrors.sustech.edu.cn"),
    ("西北农林 NWAFU",             "edu:mirrors.nwafu.edu.cn"),
    ("兰州大学 LZU",               "edu:mirror.lzu.edu.cn"),
    ("北京交大 BJTU",              "edu:mirror.bjtu.edu.cn"),
    ("南阳理工 NYIST",             "edu:mirror.nyist.edu.cn"),
];

/// 根据镜像类型与 host 构造实际的 git clone URL
fn build_clone_url(mirror: &(&str, &str)) -> String {
    let (_, tag) = mirror;
    if let Some((kind, host)) = tag.split_once(':') {
        match kind {
            "direct"  => format!("https://github.com/{}/{}.git", REPO_OWNER, REPO_NAME),
            "proxy"   => format!("https://{}/https://github.com/{}/{}.git", host, REPO_OWNER, REPO_NAME),
            "rewrite" => format!("https://{}/{}/{}.git", host, REPO_OWNER, REPO_NAME),
            "cloud"   => format!("https://{}/github/{}/{}.git", host, REPO_OWNER, REPO_NAME),
            "edu"     => format!("https://{}/git/{}/{}.git", host, REPO_OWNER, REPO_NAME),
            _         => format!("https://github.com/{}/{}.git", REPO_OWNER, REPO_NAME),
        }
    } else {
        format!("https://github.com/{}/{}.git", REPO_OWNER, REPO_NAME)
    }
}

/// 并行探测所有镜像的 HTTP 连通延迟，返回按速度排序后的镜像列表。
/// 超时/不可达的排到末尾，避免拖慢整体。
async fn ranked_mirrors() -> Vec<(&'static str, &'static str)> {
    let tasks: Vec<_> = REPO_URLS
        .iter()
        .map(|(label, tag)| {
            let label = *label;
            let tag = *tag;
            let test_url = probe_url(tag);
            tokio::spawn(async move {
                let elapsed = probe_one(&test_url).await;
                (label, tag, elapsed)
            })
        })
        .collect();

    let mut results: Vec<_> = join_all(tasks)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    // 按延迟升序，快的排在前面
    results.sort_by(|a, b| {
        a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Greater)
    });

    results.into_iter().map(|(l, t, _)| (l, t)).collect()
}

/// 根据镜像 tag 构造测速用的探测 URL（只测基础域名，不做 git 操作）
fn probe_url(tag: &str) -> String {
    match tag.split_once(':') {
        Some(("direct", _))      => "https://github.com".to_string(),
        Some((_, host)) => format!("https://{}", host),
        None                     => "https://github.com".to_string(),
    }
}

/// 用 curl 探测单个 URL 的响应时间（秒），超时返回大值
async fn probe_one(url: &str) -> f64 {
    let Ok(curl) = which("curl") else { return 999.0 };
    let output = TokioCommand::new(curl)
        .args([
            "-o", "/dev/null",
            "-s",
            "-w", "%{time_total}",
            "--connect-timeout", "3",
            "--max-time", "5",
            url,
        ])
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<f64>()
                .unwrap_or(999.0)
        }
        _ => 999.0,
    }
}

/// 模型下载镜像（HuggingFace → hf-mirror → ModelScope → github.akams.cn RAW 加速 → 全量 ghproxy）
const MODEL_BASES: &[(&str, &str)] = &[
    ("huggingface.co",  "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/"),
    ("hf-mirror.com",   "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/"),
    ("modelscope.cn",   "https://www.modelscope.cn/models/BlueSeaAI/whisper.cpp/resolve/master/"),
    ("github.akams.cn", "https://github.akams.cn/https://raw.githubusercontent.com/ggerganov/whisper.cpp/master/"),
    ("ghproxy.com RAW", "https://ghproxy.com/https://raw.githubusercontent.com/ggerganov/whisper.cpp/master/"),
];

const PYPI_INDEX_TSUPER: &str = "https://pypi.tuna.tsinghua.edu.cn/simple";
const PYPI_INDEX_ALIYUN: &str = "https://mirrors.aliyun.com/pypi/simple";
const PYPI_INDEX_USTC: &str = "https://mirrors.ustc.edu.cn/pypi/simple";

/// 确保标准引擎（whisper.cpp）二进制与指定模型就绪。
/// 已存在则跳过对应步骤；否则优先使用 bundled 预编译二进制，最后才尝试网络下载/编译。
pub async fn ensure(app: &AppHandle, model: &str) -> Result<(), String> {
    log_event("ENSURE", &format!("开始部署标准引擎 model={}", model));
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let bin = whisper_cpp::binary_path(&data);
    let model_file = whisper_cpp::model_path(&data, model);

    if bin.exists() && model_file.exists() {
        log_event("OK", "引擎与模型已就绪，跳过部署");
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "完成", "percent": 100, "message": "引擎已就绪" }),
        );
        return Ok(());
    }

    std::fs::create_dir_all(data.join("bin")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(data.join("models")).map_err(|e| e.to_string())?;

    if !bin.exists() {
        log_event("STEP", &format!("需要安装二进制 -> {}", bin.display()));
        // 优先检查 bundled whisper-cli（已打包在 app 内）
        let bundled = crate::engine::resolve_tool_path("whisper-cli", &[]);
        if let Some(bundled_path) = bundled {
            log_event("BUNDLED", &format!("使用 bundled whisper-cli: {}", bundled_path.display()));
            // 把 bundled 二进制复制到 app data 目录（保持统一路径管理）
            if let Some(parent) = bin.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&bundled_path, &bin).map_err(|e| {
                format!("复制 bundled whisper-cli 失败: {}", e)
            })?;
            std::fs::set_permissions(
                &bin,
                std::os::unix::fs::PermissionsExt::from_mode(0o755),
            )
            .map_err(|e| e.to_string())?;
            // 同时复制 dylib 依赖到 bin/ 目录
            copy_whisper_dylibs(&bundled_path, &data)?;
            let _ = app.emit(
                "deploy-progress",
                serde_json::json!({
                    "stage": "安装",
                    "percent": 90,
                    "message": "使用内置引擎（无需网络）"
                }),
            );
        } else {
            log_event("STEP", "bundled 不可用，走网络部署流程");
            build_binary(app, &data).await.map_err(|e| {
                log_event("ERROR", &e);
                e
            })?;
        }
    }
    if !model_file.exists() {
        log_event("STEP", &format!("需要下载模型 model={}", model));
        download_model(app, &data, model).await.map_err(|e| {
            log_event("ERROR", &e);
            e
        })?;
    }
    log_event("OK", "部署完成");
    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "完成", "percent": 100, "message": "部署完成" }),
    );
    Ok(())
}

/// 从 bundled whisper-cli 的目录复制 dylib 依赖到 app data bin/ 目录
fn copy_whisper_dylibs(bundled_bin: &Path, data: &Path) -> Result<(), String> {
    let bundle_dir = bundled_bin.parent().ok_or("无法获取 bundle 目录")?;
    let dest_dir = data.join("bin");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(bundle_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".dylib") {
            let dest = dest_dir.join(&*name_str);
            if !dest.exists() {
                std::fs::copy(entry.path(), &dest).map_err(|e| format!("复制 {} 失败: {}", name_str, e))?;
            }
        }
    }
    Ok(())
}

/// 一键部署兼容引擎（本机 Python openai-whisper）。
/// 智能策略：已安装 whisper + ffmpeg 则跳过；已装 whisper 缺 ffmpeg 则只装 ffmpeg；
/// 都没有则全量安装。
pub async fn ensure_compat(app: &AppHandle) -> Result<(), String> {
    log_event("COMPAT", "开始部署兼容引擎");
    let full = crate::engine::python_whisper::detect_full();
    let has_whisper = full.cli_path.is_some() || full.python_path.is_some();

    // Whisper + ffmpeg 都就绪 → 直接跳过
    if has_whisper && full.ffmpeg_available {
        log_event("COMPAT", "兼容引擎已就绪，跳过");
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "完成", "percent": 100, "message": "兼容引擎已就绪" }),
        );
        return Ok(());
    }

    // 检查 bundled ffmpeg 是否可用（已打包在 app 内，零外部依赖）
    let bundled_ffmpeg = crate::engine::resolve_tool_path("ffmpeg", &[
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
    ]);

    // 有 whisper 但缺 ffmpeg → 优先用 bundled，没有才尝试 brew
    if has_whisper && !full.ffmpeg_available {
        if bundled_ffmpeg.is_some() {
            log_event("COMPAT", "whisper已安装，bundled ffmpeg可用，跳过安装");
            let _ = app.emit(
                "deploy-progress",
                serde_json::json!({
                    "stage": "完成", "percent": 100,
                    "message": "ffmpeg 已内置，兼容引擎就绪"
                }),
            );
            return Ok(());
        }
        // Bundled 不存在，最后一招才用 brew
        log_event("COMPAT", "whisper已安装，ffmpeg不可用，尝试brew安装");
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "音频组件", "percent": 10, "message": "bundled ffmpeg 未找到，尝试通过 brew 安装..." }),
        );
        let brew = which("brew").map_err(|_| {
            "ffmpeg 不可用，且未检测到 Homebrew。\n请确保已安装 ffmpeg 或 Homebrew。".to_string()
        })?;
        run_step(
            brew.to_str().unwrap(),
            &["install", "ffmpeg"],
            None,
            Some(&homebrew_mirror_envs()),
        )
        .await
        .map_err(|e| format!("安装 ffmpeg 失败：{}", e))?;
        log_event("COMPAT", "brew ffmpeg 安装完成");
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "完成", "percent": 100, "message": "ffmpeg 安装完成，兼容引擎已就绪" }),
        );
        return Ok(());
    }

    // 完全没有 whisper → 需要全量安装
    // 1) ffmpeg：优先 bundled，没有才 brew
    if bundled_ffmpeg.is_none() {
        log_event("COMPAT", "bundled ffmpeg 不可用，尝试 brew 安装");
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "音频组件", "percent": 10, "message": "正在安装 ffmpeg（brew）" }),
        );
        let brew = which("brew").map_err(|_| {
            "ffmpeg 不可用，且未检测到 Homebrew。\n请确保已安装 ffmpeg 或 Homebrew。".to_string()
        })?;
        run_step(
            brew.to_str().unwrap(),
            &["install", "ffmpeg"],
            None,
            Some(&homebrew_mirror_envs()),
        )
        .await
        .map_err(|e| format!("安装 ffmpeg 失败：{}", e))?;
    } else {
        log_event("COMPAT", "使用 bundled ffmpeg，跳过 brew");
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "音频组件", "percent": 10, "message": "ffmpeg 已内置，跳过安装" }),
        );
    }

    // 2) pip install openai-whisper
    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "转写内核", "percent": 55, "message": "正在安装转写内核（pip）" }),
    );
    install_python_whisper().await?;

    // 3) whisper --help 校验
    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "校验", "percent": 90, "message": "正在校验兼容引擎" }),
    );
    let whisper = crate::engine::python_whisper::detect()
        .ok_or("安装完成但未检测到 whisper，请检查 PATH。".to_string())?;
    run_step(whisper.to_str().unwrap(), &["--help"], None, None)
        .await
        .map_err(|e| format!("兼容引擎校验失败：{}", e))?;

    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "完成", "percent": 100, "message": "兼容引擎部署完成" }),
    );
    Ok(())
}

/// 使用清华 PyPI 镜像安装 openai-whisper，失败则回退阿里云镜像，最后走官方源
async fn install_python_whisper() -> Result<(), String> {
    for (pip, idx_args) in [
        ("pip3", &[][..]),
        ("pip", &[][..]),
        ("python3", &["-m", "pip"][..]),
    ] {
        if let Ok(p) = which(pip) {
            let program = p.to_str().unwrap();
            // 先尝试清华源
            let mut args: Vec<&str> = idx_args.to_vec();
            args.extend(["install", "-U", "openai-whisper", "-i", PYPI_INDEX_TSUPER]);
            if run_step(program, &args, None, None).await.is_ok() {
                return Ok(());
            }
            // 再尝试阿里云源
            let mut args2: Vec<&str> = idx_args.to_vec();
            args2.extend(["install", "-U", "openai-whisper", "-i", PYPI_INDEX_ALIYUN]);
            if run_step(program, &args2, None, None).await.is_ok() {
                return Ok(());
            }
            // 再尝试中科大源
            let mut args3: Vec<&str> = idx_args.to_vec();
            args3.extend(["install", "-U", "openai-whisper", "-i", PYPI_INDEX_USTC]);
            if run_step(program, &args3, None, None).await.is_ok() {
                return Ok(());
            }
            // 最后走官方源
            let mut args4: Vec<&str> = idx_args.to_vec();
            args4.extend(["install", "-U", "openai-whisper"]);
            if run_step(program, &args4, None, None).await.is_ok() {
                return Ok(());
            }
        }
    }
    Err("未检测到 pip / python3，或所有镜像均安装失败。".to_string())
}

async fn run_step(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    envs: Option<&[(&str, &str)]>,
) -> Result<(), String> {
    let mut cmd = TokioCommand::new(program);
    cmd.args(args)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    if let Some(e) = envs {
        for (k, v) in e {
            cmd.env(k, v);
        }
    }
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        Err(if tail.is_empty() { format!("退出码 {:?}", out.status.code()) } else { tail })
    }
}

/// 带超时的 run_step：超时后自动 kill 子进程，防止网络卡死
async fn run_step_timeout(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    envs: Option<&[(&str, &str)]>,
    timeout_secs: u64,
) -> Result<(), String> {
    let program_owned = program.to_string();
    let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        async {
            run_step(&program_owned, &args_owned.iter().map(|s| s.as_str()).collect::<Vec<_>>(), cwd, envs).await
        },
    )
    .await
    .map_err(|_| format!("{} {} 执行超时（{}秒）", program, args.join(" "), timeout_secs))?
}

/// 国内 Homebrew 镜像环境变量（USTC）
fn homebrew_mirror_envs() -> [(&'static str, &'static str); 5] {
    [
        ("HOMEBREW_BREW_GIT_REMOTE", "https://mirrors.ustc.edu.cn/brew.git"),
        ("HOMEBREW_CORE_GIT_REMOTE", "https://mirrors.ustc.edu.cn/homebrew-core.git"),
        ("HOMEBREW_API_DOMAIN", "https://mirrors.ustc.edu.cn/homebrew-bottles/api"),
        ("HOMEBREW_BOTTLE_DOMAIN", "https://mirrors.ustc.edu.cn/homebrew-bottles"),
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
    ]
}

/// 预编译二进制下载通道 —— 不依赖编译工具链，直接从 GitHub Releases 拉取现成的 whisper-cli。
/// 尝试顺序：直接 GitHub → ghproxy → gh-proxy → github.akams.cn
async fn try_prebuilt_binary(app: &AppHandle, data: &Path) -> Result<(), String> {
    let dest = whisper_cpp::binary_path(data);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // 构造多条下载 URL，涵盖多个镜像
    let urls: &[(&str, &str)] = &[
        ("GitHub 直连", &format!(
            "https://github.com/{0}/{1}/releases/latest/download/whisper-cli",
            REPO_OWNER, REPO_NAME
        )),
        ("ghproxy.com", &format!(
            "https://ghproxy.com/https://github.com/{0}/{1}/releases/latest/download/whisper-cli",
            REPO_OWNER, REPO_NAME
        )),
        ("gh-proxy.com", &format!(
            "https://gh-proxy.com/https://github.com/{0}/{1}/releases/latest/download/whisper-cli",
            REPO_OWNER, REPO_NAME
        )),
        ("github.akams.cn", &format!(
            "https://github.akams.cn/https://github.com/{0}/{1}/releases/latest/download/whisper-cli",
            REPO_OWNER, REPO_NAME
        )),
    ];

    let mut last_err = String::new();
    for (label, url) in urls {
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({
                "stage": "下载预编译",
                "percent": 42,
                "message": format!("尝试下载预编译二进制（{}）", label)
            }),
        );
        match download::download_with_progress(app, url, &dest, "deploy-progress-dl").await {
            Ok(()) => {
                // 校验：下载的文件确实是可执行格式
                if dest.is_file() && dest.metadata().map(|m| m.len() > 4096).unwrap_or(false) {
                    let _ = std::fs::set_permissions(
                        &dest,
                        std::os::unix::fs::PermissionsExt::from_mode(0o755),
                    );
                    let _ = app.emit(
                        "deploy-progress",
                        serde_json::json!({
                            "stage": "安装",
                            "percent": 90,
                            "message": format!("预编译二进制就绪（{}）", label)
                        }),
                    );
                    return Ok(());
                }
                last_err = format!("[{}] 文件校验失败（过小或不存在）", label);
                let _ = std::fs::remove_file(&dest);
            }
            Err(e) => {
                last_err = format!("[{}] {}", label, e);
                let _ = std::fs::remove_file(&dest);
            }
        }
    }
    Err(format!("预编译二进制下载失败: {}", last_err))
}

/// 检查 Xcode Command Line Tools，缺少时自动引导安装
/// whisper.cpp 编译需要 cc / c++，这些来自 Xcode CLT
async fn ensure_xcode_clt(app: &AppHandle) -> Result<(), String> {
    // 用 xcode-select -p 检查是否已安装
    let check = {
        let child = TokioCommand::new("xcode-select")
            .args(["-p"])
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(c) => match tokio::time::timeout(std::time::Duration::from_secs(10), c.wait_with_output()).await {
                Ok(Ok(out)) => Some(out),
                _ => None,
            },
            Err(_) => None,
        }
    };

    if check.map(|o| o.status.success()).unwrap_or(false) {
        return Ok(());
    }

    // Xcode CLT 未安装，尝试触发安装
    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({
            "stage": "编译环境",
            "percent": 2,
            "message": "检测到缺少 Xcode Command Line Tools，正在触发安装..."
        }),
    );

    // xcode-select --install 会弹出系统对话框，用户需要手动确认
    let install = {
        let child = TokioCommand::new("xcode-select")
            .args(["--install"])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match child {
            Ok(c) => match tokio::time::timeout(std::time::Duration::from_secs(30), c.wait_with_output()).await {
                Ok(Ok(out)) => Some(out),
                _ => None,
            },
            Err(_) => None,
        }
    };

    match install {
        Some(out) if out.status.success() => {
            // 安装请求已发出，但安装器是 GUI 的，给一点时间
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _ = app.emit(
                "deploy-progress",
                serde_json::json!({
                    "stage": "编译环境",
                    "percent": 3,
                    "message": "请在弹窗中确认安装 Xcode Command Line Tools，完成后将自动继续..."
                }),
            );
            Ok(())
        }
        _ => {
            // 有些系统上 xcode-select --install 可能报错（已安装但路径未配置好）
            // 检查 cc 是否存在作为兜底
            if which("cc").is_ok() {
                return Ok(());
            }
            Err("未检测到 Xcode Command Line Tools 且自动安装失败。\
                 请手动运行: xcode-select --install 后重试。"
                .to_string())
        }
    }
}

/// 检测编译工具链完整性：git, make, cmake, cc
async fn ensure_build_tools(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let git = which("git")?;
    let make = which("make")?;

    // 检查并安装 cmake
    ensure_cmake(app).await?;

    // 检查 Xcode CLT（cc 编译器）
    ensure_xcode_clt(app).await?;

    // 最终校验 cc 是否可用
    which("cc").map_err(|_| {
        "编译 whisper.cpp 需要 C 编译器（cc）。请确保 Xcode Command Line Tools 已安装。\
         终端运行: xcode-select --install"
            .to_string()
    })?;

    Ok((git, make))
}

/// 源码编译（make），返回 build stderr 用于错误诊断
struct BuildResult {
    success: bool,
    stderr: String,
}

async fn build_from_source(
    make: &PathBuf,
    src: &Path,
    cores: usize,
) -> BuildResult {
    // 策略 1: CoreML + Metal（Apple Silicon 最优）
    let build = make_build(
        make,
        src,
        &[
            "WHISPER_COREML=1",
            "WHISPER_METAL=1",
            "-j",
            &cores.to_string(),
        ],
    )
    .await;

    match build {
        Ok(out) if out.status.success() => {
            return BuildResult {
                success: true,
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            };
        }
        Ok(out) => {
            let err1 = String::from_utf8_lossy(&out.stderr).to_string();
            // 策略 2: Metal only
            let build2 = make_build(
                make,
                src,
                &["WHISPER_METAL=1", "-j", &cores.to_string()],
            )
            .await;
            match build2 {
                Ok(out2) if out2.status.success() => BuildResult {
                    success: true,
                    stderr: String::from_utf8_lossy(&out2.stderr).to_string(),
                },
                Ok(out2) => BuildResult {
                    success: false,
                    stderr: format!(
                        "CoreML+Metal stderr:\n{}\n---\nMetal-only stderr:\n{}",
                        err1,
                        String::from_utf8_lossy(&out2.stderr)
                    ),
                },
                Err(e) => BuildResult {
                    success: false,
                    stderr: format!("CoreML+Metal stderr:\n{}\n---\nMetal-only 启动失败: {}", err1, e),
                },
            }
        }
        Err(e) => BuildResult {
            success: false,
            stderr: format!("编译进程启动失败: {}", e),
        },
    }
}

async fn build_binary(app: &AppHandle, data: &PathBuf) -> Result<(), String> {
    let tmp = std::env::temp_dir().join("voice2text_build");
    let src = tmp.join("whisper.cpp");

    // 如果上次编译的产物在 data 目录下已存在，直接复用
    let dest = whisper_cpp::binary_path(data);
    if dest.exists() {
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({ "stage": "完成", "percent": 100, "message": "复用已编译引擎" }),
        );
        return Ok(());
    }

    // ━━━ 通道 B (优先): 预编译二进制下载 ━━━
    // 零用户配置，不需要 cmake / git / make / cc 等任何编译工具链
    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({
            "stage": "下载",
            "percent": 5,
            "message": "正在下载预编译引擎（无需编译工具链）..."
        }),
    );
    match try_prebuilt_binary(app, data).await {
        Ok(()) => return Ok(()),
        Err(prebuilt_err) => {
            let _ = app.emit(
                "deploy-progress",
                serde_json::json!({
                    "stage": "下载",
                    "percent": 10,
                    "message": format!("预编译下载失败，回退源码编译... ({})", prebuilt_err)
                }),
            );
        }
    }

    // ━━━ 通道 A: 源码克隆 + 编译 ━━━
    // 仅此通道需要 cmake / git / make / cc 等编译工具链
    let (git, make) = ensure_build_tools(app).await?;

    // 清理旧目录
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "测速", "percent": 12, "message": "正在探测各镜像延迟，自动选最快..." }),
    );
    let mirrors = ranked_mirrors().await;
    let total = mirrors.len();

    let mut _last_clone_err = String::new();
    let mut cloned = false;
    for (idx, (label, tag)) in mirrors.iter().enumerate() {
        let url = build_clone_url(&(label, tag));
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({
                "stage": "拉取源码",
                "percent": 15 + ((idx * 8) / total) as u8,
                "message": format!("正在克隆引擎源码（{}）", label)
            }),
        );
        let clone_child = TokioCommand::new(&git)
            .args([
                "clone",
                "--depth",
                "1",
                &url,
                src.to_str().unwrap(),
            ])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let clone_output = match clone_child {
            Ok(child) => {
                let pid = child.id();
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    child.wait_with_output(),
                )
                .await;
                match result {
                    Ok(out) => out,
                    Err(_elapsed) => {
                        if let Some(id) = pid {
                            let _ = TokioCommand::new("kill")
                                .args(["-9", &format!("-{}", id)])
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn();
                        }
                        Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "clone timeout"))
                    }
                }
            }
            Err(e) => {
                _last_clone_err = format!("[{}] 启动 git clone 失败: {}", label, e);
                let _ = std::fs::remove_dir_all(&src);
                continue;
            }
        };
        match clone_output {
            Ok(out) if out.status.success() => {
                let _ = app.emit(
                    "deploy-progress",
                    serde_json::json!({
                        "stage": "拉取源码",
                        "percent": 30,
                        "message": format!("源码克隆成功（{}）", label)
                    }),
                );
                cloned = true;
                break;
            }
            Ok(out) => {
                _last_clone_err = format!(
                    "[{}] 克隆失败: {}",
                    label,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            Err(e) => {
                _last_clone_err = format!("[{}] 克隆超时: {}", label, e);
            }
        }
        let _ = std::fs::remove_dir_all(&src);
    }

    if !cloned {
        return Err(format!(
            "标准引擎获取失败。预编译下载不可用，源码克隆也全部失败。\n\
             请检查网络连接，或手动安装 git + cmake + Xcode CLT 后重试。\n\
             最后克隆错误: {}",
            _last_clone_err
        ));
    }

    // 源码就绪，开始编译
    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "拉取源码", "percent": 32, "message": "同步子模块（ggml）" }),
    );
    update_submodules_fallible(&git, &src).await;

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({ "stage": "编译", "percent": 50, "message": "正在编译（CoreML / Metal）" }),
    );
    let build_result = build_from_source(&make, &src, cores).await;

    if build_result.success {
        if let Some(found) = find_binary(&src) {
            let _ = app.emit(
                "deploy-progress",
                serde_json::json!({ "stage": "安装", "percent": 95, "message": "复制可执行文件" }),
            );
            std::fs::copy(&found, &dest).map_err(|e| e.to_string())?;
            std::fs::set_permissions(
                &dest,
                std::os::unix::fs::PermissionsExt::from_mode(0o755),
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }
        log_build_dir(&src);
        return Err("编译通过但未找到可执行文件产物，请稍后重试。".to_string());
    }

    Err(format!(
        "标准引擎编译失败。\n\
         预编译下载不可用，源码编译也失败了。\n\
         编译错误: {}",
        &build_result.stderr[..build_result.stderr.len().min(300)]
    ))
}

/// 调试用：列出 build 目录内容，帮助诊断"编译通过但找不到产物" 
fn log_build_dir(src: &Path) {
    if let Ok(entries) = std::fs::read_dir(src) {
        let names: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let ft = e.file_type().ok()?;
                let name = e.file_name().to_string_lossy().to_string();
                let marker = if ft.is_dir() { "/" } else if ft.is_symlink() { "@" } else { "" };
                Some(format!("{}{}", name, marker))
            })
            .collect();
        log_event(
            "BUILD_DIR",
            &format!("{}: {} files [{}]", src.display(), names.len(), names.join(", ")),
        );
    }
}

async fn make_build(
    make: &PathBuf,
    src: &Path,
    args: &[&str],
) -> Result<std::process::Output, String> {
    let make_path = make.clone();
    let src_path = src.to_path_buf();
    let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::time::timeout(
        std::time::Duration::from_secs(600),
        async move {
            let child = TokioCommand::new(&make_path)
                .current_dir(&src_path)
                .args(args_owned.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .kill_on_drop(true)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            child.wait_with_output().await.map_err(|e| e.to_string())
        },
    )
    .await
    .map_err(|_| "编译超时（10分钟），请检查网络或重试".to_string())?
}

async fn download_model(app: &AppHandle, data: &PathBuf, model: &str) -> Result<(), String> {
    let filename = crate::engine::whisper_cpp_model_file(model);
    let dest = whisper_cpp::model_path(data, model);

    let mut last_err = String::new();
    for (idx, (label, base_url)) in MODEL_BASES.iter().enumerate() {
        let url = format!("{}{}", base_url, filename);
        let _ = app.emit(
            "deploy-progress",
            serde_json::json!({
                "stage": "下载模型",
                "percent": 70 + (idx * 2) as u8,
                "message": format!("正在下载模型 {}（{}）", model, label)
            }),
        );
        match download::download_with_progress(app, &url, &dest, "deploy-progress-dl").await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = format!("[{}] {}", label, e);
                // 删除不完整文件
                let _ = std::fs::remove_file(&dest);
            }
        }
    }
    Err(format!("所有 {} 个模型镜像均失败: {}", MODEL_BASES.len(), last_err))
}

fn which(name: &str) -> Result<PathBuf, String> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim().to_string()))
        .filter(|p| p.exists())
        .ok_or_else(|| format!("未找到 {}，请先安装", name))
}

/// 确保 cmake 可用；没有则自动通过 brew 安装
async fn ensure_cmake(app: &AppHandle) -> Result<(), String> {
    if which("cmake").is_ok() {
        return Ok(()); // 已有，直接过
    }

    let brew = which("brew").map_err(|_| {
        "编译 whisper.cpp 需要 cmake，但您的 Mac 上未安装 cmake 且没有 Homebrew。\
         请手动安装 Homebrew（https://brew.sh）后重试。"
            .to_string()
    })?;

    let _ = app.emit(
        "deploy-progress",
        serde_json::json!({
            "stage": "编译环境",
            "percent": 2,
            "message": "检测到缺少 cmake，正在通过 brew 安装..."
        }),
    );

    run_step(
        brew.to_str().unwrap(),
        &["install", "cmake"],
        None,
        Some(&homebrew_mirror_envs()),
    )
    .await
    .map_err(|e| format!("安装 cmake 失败：{}。请手动运行 brew install cmake 后重试", e))?;

    // 安装后再校验一次
    which("cmake").map_err(|_| "cmake 安装完成但未找到，请检查 PATH".to_string())?;
    Ok(())
}

fn find_binary(dir: &Path) -> Option<PathBuf> {
    let search_dirs = [
        dir.to_path_buf(),
        dir.join("build").join("bin"),  // CMake build 产物路径
    ];
    for d in &search_dirs {
        for name in ["whisper-cli", "main", "whisper"] {
            let p = d.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 非致命地尝试更新子模块；失败时依次尝试 ghproxy / gh-proxy / github.akams.cn 代理加速
async fn update_submodules_fallible(git: &PathBuf, src: &Path) {
    let submodule_proxies: &[&str] = &[
        "ghproxy.com",
        "gh-proxy.com",
        "github.akams.cn",
    ];

    // 先尝试直接 submodule update
    if run_step_timeout(
        git.to_str().unwrap(),
        &["submodule", "update", "--init", "--recursive"],
        Some(src),
        None,
        60,
    )
    .await
    .is_ok()
    {
        return;
    }

    // 依次尝试不同代理前缀
    for proxy_host in submodule_proxies {
        let instead_url = format!("https://{}/https://github.com/", proxy_host);
        // 直接在源码目录里设 git 全局替代
        let _ = run_step(
            git.to_str().unwrap(),
            &[
                "config",
                "--local",
                "url.https://ghproxy.com/https://github.com/.insteadOf",
                "https://github.com/",
            ],
            Some(src),
            None,
        )
        .await;
        // 为每个已存在的子模块 config 也加代理
        if let Ok(entries) = std::fs::read_dir(src.join(".git").join("modules")) {
            for e in entries.flatten() {
                let config = e.path().join("config");
                if config.is_file() {
                    let _ = run_step(
                        git.to_str().unwrap(),
                        &[
                            "config",
                            "--file",
                            config.to_str().unwrap_or(""),
                            &format!("url.{}.insteadOf", instead_url),
                            "https://github.com/",
                        ],
                        None,
                        None,
                    )
                    .await;
                }
            }
        }
        // 重试 submodule update
        if run_step_timeout(
            git.to_str().unwrap(),
            &["submodule", "update", "--init", "--recursive"],
            Some(src),
            None,
            60,
        )
        .await
        .is_ok()
        {
            return;
        }
    }

    // 最后兜底：直接设 insteadOf 再试一次
    let _ = run_step(
        git.to_str().unwrap(),
        &[
            "config",
            "--local",
            "url.https://ghproxy.com/https://github.com/.insteadOf",
            "https://github.com/",
        ],
        Some(src),
        None,
    )
    .await;
    let _ = run_step_timeout(
        git.to_str().unwrap(),
        &["submodule", "update", "--init", "--recursive"],
        Some(src),
        None,
        60,
    )
    .await;
}