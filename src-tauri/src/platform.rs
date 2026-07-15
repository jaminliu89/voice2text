use serde::{Deserialize, Serialize};

/// 统一加速后端：双引擎共享的硬件加速能力检测结果
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AccelerationBackend {
    /// Apple CoreML + ANE（whisper.cpp 专用，Apple Silicon 原生）
    CoreML,
    /// Metal Performance Shaders（Python torch 专用，Apple Silicon）
    Mps,
    /// Metal GPU 通用计算（whisper.cpp 非 Apple Silicon 回退）
    Metal,
    /// 纯 CPU 回退
    Cpu,
}

/// 平台能力信息：用于自动选择加速方式与推荐模型大小
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlatformInfo {
    pub apple_silicon: bool,
    pub memory_gb: f64,
    pub physical_cores: usize,
    /// 当前平台最优加速后端
    pub acceleration: AccelerationBackend,
    /// 推荐的模型键（base / small / medium）
    pub recommended_model: String,
    /// PyTorch MPS 运行时是否可用（独立于芯片检测，需要 torch 安装）
    pub mps_available: bool,
}

/// 检测当前 macOS 平台的芯片、内存、核心数、以及双引擎共用加速能力
pub fn detect() -> PlatformInfo {
    let brand = sysctl("machdep.cpu.brand_string");
    let apple_silicon = brand.to_lowercase().contains("apple")
        || std::env::consts::ARCH == "aarch64";

    let mem_bytes = sysctl("hw.memsize").trim().parse::<u64>().unwrap_or(0);
    let memory_gb = mem_bytes as f64 / 1_073_741_824.0;

    let physical_cores = sysctl("hw.physicalcpu")
        .trim()
        .parse::<usize>()
        .unwrap_or(4)
        .max(1);

    // 统一加速后端检测（双引擎共享）
    let acceleration = if apple_silicon {
        AccelerationBackend::CoreML
    } else {
        AccelerationBackend::Metal
    };

    // Python 引擎 MPS 运行时可用性检测
    let mps_available = check_mps_runtime();

    // 内存越大可选越大模型；16G 以上推荐 small，否则 base
    let recommended_model = if memory_gb >= 16.0 {
        "small".to_string()
    } else {
        "base".to_string()
    };

    PlatformInfo {
        apple_silicon,
        memory_gb,
        physical_cores,
        acceleration,
        recommended_model,
        mps_available,
    }
}

/// 共享加速检测：运行时探 PyTorch MPS 是否可用。
/// 打包后的 macOS .app 没有终端 PATH，需要硬编码 brew 路径回退。
fn check_mps_runtime() -> bool {
    for py in resolve_python_candidates() {
        if let Ok(o) = std::process::Command::new(&py)
            .args(["-c", "import torch; print(torch.backends.mps.is_available())"])
            .output()
        {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_lowercase();
                return s == "true";
            }
        }
    }
    false
}

/// 返回 Python 解释器的候选路径列表
fn resolve_python_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    // which 优先（dev 模式 PATH 完整）
    for name in &["python3", "python"] {
        if let Some(p) = which_cmd(name) {
            candidates.push(p);
        }
    }
    // 硬编码回退（打包 app PATH 不含 brew）
    for p in &[
        "/opt/homebrew/bin/python3",
        "/opt/homebrew/bin/python",
        "/usr/local/bin/python3",
        "/usr/local/bin/python",
        "/usr/bin/python3",
    ] {
        let s = p.to_string();
        if std::path::Path::new(&s).exists() && !candidates.contains(&s) {
            candidates.push(s);
        }
    }
    candidates
}

fn which_cmd(name: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| std::path::Path::new(s).exists())
}

fn sysctl(key: &str) -> String {
    std::process::Command::new("sysctl")
        .arg("-n")
        .arg(key)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}
