# 小柳语音转写 (voice2text) — 项目全手册

> **项目定位**：macOS 本地离线语音转写桌面工具  
> **技术栈**：Tauri 2 + Rust + Svelte 5 + TypeScript  
> **核心理念**：零网络依赖、本地引擎、多格式输出  

---

## 一、项目来时路（开发历程日志）

### Day 1 — 骨架搭建
- `npm create tauri-app@latest` 脚手架初始化
- 技术选型：Tauri v2 + Svelte 5 + Vite 6 + TypeScript
- 窗口配置：980×740，最小 760×560
- 全局错误注入：`index.html` 顶部 `window.addEventListener("error", ...)` → `debug_log` Rust command → `/tmp/tauri-debug.log`

### Day 1 Trap（避坑记录）
| # | 陷阱 | 现象 | 修复 |
|---|------|------|------|
| Trap A | HTML5 drag-drop 被 Rust 层拦截 | 拖入文件无反应 | `tauri.conf.json` → `dragDropEnabled: false`，前端用 `onDragDropEvent()` |
| Trap B | WKWebView 不支持 blob URL download | 另存为按钮无效 | Rust command + `@tauri-apps/plugin-dialog` 原生 save() |
| Trap C | 函数名 `isTauri()` 与 Tauri 2 内置变量冲突 | 整段 JS 静默死亡 | 换名 |
| Trap D | pkill + open 竞态 | 新进程被刚 spawn 就杀掉 | `sleep 2` |

### Day 2 — 引擎系统
- **双引擎架构**：
  - 标准引擎：`whisper.cpp` CLI 二进制，CoreML 加速
  - 兼容引擎：Python `openai-whisper`，MPS GPU 回退
- 硬件检测：`platform.rs` → Apple Silicon / Intel / 内存 / 物理核心 / 加速后端
- 模型下载：`deploy.rs` — 流式下载 + 进度事件回传 + 自动解压安装

### Day 3 — 转写核心
- `transcribe.rs`：tokio `Semaphore` 控制并行数，事件逐条回传前端
- `engine/whisper_cpp.rs`：CLI 调用 `whisper-cli`，解析输出为 `Segment[]`
- `engine/python_whisper.rs`：Python 引擎检测 (`pip show`/`which`) + 调用

### Day 4 — 后处理管道
- **繁简转换**：`postprocess/t2s.rs` — 650 组繁简对照表，`OnceLock` 懒初始化
- **智能分段**：按句末标点 + 静音间隔自动合并为自然段落
- **多格式输出**：
  - MD 表格（全文 + 时间轴表格）
  - SRT / VTT 字幕
  - TXT 纯文本
  - HTML 提词稿（深色背景 + 大字居中 + 每行 ≤12 字 + 自动滚动）
  - MD 提词稿（纯文本 + 每行 ≤12 字）
  - RTF（跨平台默认中文字体，Word/WPS 可打开）

### Day 5 — 基线锁定体系
- 创建 `BASELINE.toml`：冻结区 17 文件 / 稳定区 6 文件 / 活跃区 3 文件
- 硬约束：lock 文件禁用 `write_to_file`，只能用 `replace_in_file` 最小增量
- 单次改动 ≤3 文件，跨区改动自动中止

### Day 6 — 提词稿迭代
- 三版迭代：纯文本 MD → 自包含 HTML 富文本 → 添加自动滚动
- Word 导出踩坑：`.doc` + RTF 在 macOS 上打不开 → 改用 `.rtf` 扩展名

### Day 7 — 交付闭环与经验沉淀
- **关键教训**：修引擎依赖时动了 lock 区文件（App.svelte），且 `get_engine_status()` 只检查"任意模型存在"而非"平台推荐模型" → 导致假就绪 → 用户转写失败
- **修复**：`commands.rs` 引擎状态检查改为匹配 `platform::recommended_model`，`whisper_cpp.rs` 去掉硬编码模型名
- **BASELINE.toml 升级**：新增 `[workflow.sop]` 四阶段闭环（Develop → Verify → Deliver → Close）+ `[workflow.fix_methodology]` 三层修复策略
- **记忆体系闭环**：5 条记忆全部更新，新增 SOP 总纲记忆，Trap 从 4 个扩展到 5 个（Trap E: 引擎状态假就绪）
- **交付确认**：用户 feedback "能正常使用了" — 3 个 m4a 文件并行转写成功，ggml-small.bin (465MB) 正确部署

### Day 8 — 商业级 DMG 构建管道 + 双仓库发布
- **Git 双仓库配置**：
  - `origin` → `git@github.com:jaminliu89/voice2text.git` (主仓库)
  - `gitee` → `https://gitee.com/jaminkim/voice2text.git` (镜像仓库)
- **DMG 构建管道重写**：
  - `bundle-all.sh` 重写：动态发现 ffmpeg/whisper-cli（`which` + `brew --prefix` 回退），**@rpath 解析关键修复**（whisper-cli 的 `@rpath/libwhisper.1.dylib` 通过 `LC_RPATH` 预收集），递归收集直到零新增文件，所有引用改为 `@loader_path`，ad-hoc 签名
  - `build-dmg.sh` 新建：Step 0 杀旧进程 + 备份（`pkill -f` → `sleep 2` → `cp -R` 到 `/Applications/小柳语音转写-backups/`），Step 1 调用 bundle-all，Step 2 tauri build，Step 3 嵌入 README.txt，Step 4 create-dmg 打包，Step 5 质量审计
  - `tauri.conf.json` 修改：`dragDropEnabled: false` + `targets: ["app"]`（禁用 Tauri 自带 DMG 构建）
- **构建 Trap（4 个新增）**：
  | # | 陷阱 | 修复 |
  |---|------|------|
  | Build Trap 1 | `@rpath` 依赖未收集（whisper-cli 的 libwhisper.1.dylib） | 源二进制预收集 `LC_RPATH` + `@rpath` 解析 |
  | Build Trap 2 | otool rpath 输出含 `(offset 12)` 后缀 | `sed 's/ (offset.*)//'` |
  | Build Trap 3 | Tauri 自带 `bundle_dmg.sh` 失败 | 改用 `create-dmg` CLI 手动打包 |
  | Build Trap 4 | `local` 关键字在函数体外使用 | 改为普通变量 `_ts` / `_build_id` |
- **Scripts 目录职责分工**：8 个脚本，4 个废弃（`bundle-ffmpeg.sh` / `copy-dylibs.sh` / `copy-ffmpeg.sh` / `fix-dylibs.sh` 被 `bundle-all.sh` 替代），4 个活跃（`bundle-all.sh` / `build-dmg.sh` / `build.sh` / `dev-tail.sh`）
- **质量门禁通过**：G1✅(零warning) G2✅(19+5文件) G3✅(冒烟测试) G4✅(零外部引用) G5✅(21MB DMG) G6✅(ad-hoc签名)
- **发布产物**：`小柳语音转写_0.2.0_aarch64.dmg` (21MB)，内嵌 README.txt 使用说明
- **GitHub Releases**：tag `v0.1.0` 推送到双仓库，DMG 作为 Release 附件

### Day 9 — 技能沉淀 + 自动推送机制
- **工作流 Skill 文档化**：
  - `.codebuddy/skills/workflow.md` — 五阶段闭环 SOP / 三级文件分类 / Debug 策略 / 质量门禁 / 红线 / Tauri 陷阱
  - `.codebuddy/skills/release.md` — 发布流程 / 版本号管理 / 自动推送 / 回滚操作
- **兼容引擎检测增强**：
  - `python_whisper.rs`：`canonicalize()` 解析 python3 symlink 真实路径 → 修复 `/usr/local/bin/python3` → Framework 的 whisper 定位失败
  - 新增 `/Library/Frameworks/Python.framework/Versions` 扫描通道
  - 新增 5 个 `#[cfg(test)]` 单元测试覆盖所有检测路径
- **版本变更自动推送**：
  - `scripts/auto-push-on-version.sh`：检测 HEAD commit 是否含版本文件变更（tauri.conf.json / Cargo.toml），自动 push 双仓库
  - `.githooks/post-commit`：git post-commit hook 触发自动推送
  - 安装：`git config core.hooksPath .githooks`

---

## 二、技术架构

```
┌─────────────────────────────────────────────────────┐
│                    前端 (Svelte 5)                     │
│  index.html → main.ts → App.svelte                    │
│    ├── DropZone.svelte      (拖拽/选择文件)            │
│    ├── SettingsPanel.svelte  (引擎/模型/语言/格式配置)   │
│    ├── FileList.svelte       (文件列表+进度)           │
│    └── ResultView.svelte     (结果预览/另存为)         │
│         │                                              │
│    ┌────┴──── Tauri invoke() / listen() ────┐         │
│    │    api.js 封装层                        │         │
└────┼─────────────────────────────────────────┼────────┘
     │                                         │
┌────┴─────────────────────────────────────────┴────────┐
│               后端 (Rust / Tauri 2)                     │
│                                                       │
│  main.rs → lib.rs (13 commands)                       │
│    ├── commands.rs      (Tauri 命令转发)               │
│    ├── transcribe.rs    (转写任务编排/并行调度)         │
│    ├── deploy.rs        (引擎部署/下载/安装)            │
│    ├── platform.rs      (硬件加速检测)                  │
│    ├── postprocess.rs   (输出管道 + 提词稿 + RTF)       │
│    │     └── t2s.rs     (繁→简转换 ~650组)             │
│    └── engine/                                        │
│          ├── mod.rs          (Segment, TranscribeOptions)│
│          ├── whisper_cpp.rs  (标准引擎)                 │
│          ├── python_whisper.rs(兼容引擎)                │
│          └── download.rs     (模型下载)                 │
└───────────────────────────────────────────────────────┘
```

### 双引擎设计

| 引擎 | 底层 | 加速 | 模型 |
|------|------|------|------|
| **标准引擎** | whisper.cpp CLI 二进制 | CoreML (Apple Silicon) / Metal GPU | tiny/base/small/medium |
| **兼容引擎** | Python openai-whisper | MPS GPU → CPU 回退 | tiny/base/small/medium |

### 数据流

```
音频文件 → collect_audio (递归收集)
         → transcribe_batch (Semaphore 并行调度)
         → 引擎执行 (whisper.cpp / Python)
         → 进度事件回传前端 (事件流)
         → 转写完成
         → postprocess 管道:
            ├── MD 表格
            ├── SRT 字幕
            ├── VTT 字幕
            ├── TXT 纯文本
            ├── HTML 提词稿 (大字深色 + 自动滚动)
            ├── MD 提词稿 (纯文本)
            └── RTF (Word 兼容)
         → 写入 voice2text/ 目录
         → 前端显示结果 + 预览 + 另存为
```

---

## 三、快速部署指南

### 前置条件

| 依赖 | 版本 | 安装方式 |
|------|------|----------|
| Rust | 1.70+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 18+ | `brew install node` 或 nvm |
| Xcode Command Line Tools | - | `xcode-select --install` |
| Python 3 (兼容引擎) | 3.9+ | `brew install python@3` |

### 中国网络配置（关键）

```bash
# ~/.cargo/config.toml
[source.crates-io]
replace-with = 'rsproxy'

[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"

[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"
```

### 一键部署

```bash
git clone <your-repo-url> voice2text
cd voice2text

# 安装前端依赖
npm install

# 开发模式运行
npm run tauri dev

# 生产构建
npm run tauri build
```

### 首次运行流程

1. 启动 → 自动检测硬件（Apple Silicon / Intel / 加速后端）
2. 选择引擎：**标准引擎**（推荐）或 兼容引擎
3. 选择模型：tiny（最快）/ base（均衡）/ small（更准）/ medium（最准）
4. 点击「部署引擎」→ 自动下载模型（~75MB-1.5GB）
5. 拖入音频文件（支持 mp3/wav/m4a/flac/ogg）
6. 选择导出格式（MD/SRT/VTT/TXT/提词稿/RTF）
7. 点击「开始转写」
8. 查看结果 → 预览/另存为

---

## 四、导出格式说明

| 格式 | 文件 | 说明 |
|------|------|------|
| **MD 表格** | `{文件名}.md` | 全文 + 时间轴表格，含繁→简 |
| **SRT 字幕** | `{文件名}.srt` | 标准字幕，含时间戳 |
| **VTT 字幕** | `{文件名}.vtt` | WebVTT 字幕 |
| **纯文本** | `{文件名}.txt` | 纯文本，段落分段 |
| **HTML 提词稿** | `{文件名}-提词稿.html` | 深色背景，大字居中，每行 ≤12 字，**自动慢速滚动**（1px/30ms ≈ 自然语速） |
| **MD 提词稿** | `{文件名}-提词稿.md` | 纯文本提词稿，每行 ≤12 字 |
| **RTF** | `{文件名}.rtf` | 跨平台 RTF 格式，Word/WPS/文本编辑 可打开 |

---

## 五、文件结构总览

```
voice2text/
├── index.html              # HTML 入口 + 全局错误捕获
├── package.json             # 前端依赖
├── package-lock.json
├── svelte.config.js         # Svelte 预处理
├── vite.config.js           # Vite 配置 (端口 1420)
├── tsconfig.json            # TypeScript 严格模式
├── BASELINE.toml            # 基线锁定文件 (17 lock + 6 stable + 3 active)
├── README.md
├── PROJECT-HANDBOOK.md      # 本文档
├── static/                  # 静态资源 (favicon/logo)
│   ├── favicon.png
│   ├── svelte.svg
│   ├── tauri.svg
│   └── vite.svg
├── src/                     # 前端源码
│   ├── main.ts              # Svelte 入口
│   ├── App.svelte           # 根组件 (395行)
│   ├── app.css              # 全局样式
│   ├── vite-env.d.ts        # 类型声明
│   └── lib/
│       ├── api.js           # Tauri invoke 封装
│       └── components/
│           ├── DropZone.svelte      # 拖拽上传
│           ├── SettingsPanel.svelte # 设置面板
│           ├── FileList.svelte      # 文件列表
│           └── ResultView.svelte    # 结果预览
├── src-tauri/               # Rust 后端
│   ├── Cargo.toml           # Rust 依赖
│   ├── tauri.conf.json      # Tauri 窗口/构建/安全配置
│   ├── icons/               # 应用图标
│   └── src/
│       ├── main.rs          # 入口 (6行)
│       ├── lib.rs           # 命令注册 (27行)
│       ├── commands.rs      # Tauri 命令转发 (190行)
│       ├── transcribe.rs    # 转写任务编排
│       ├── deploy.rs        # 引擎部署/下载
│       ├── platform.rs      # 硬件加速检测
│       ├── postprocess.rs   # 输出管道 + 提词稿 + RTF
│       │   └── t2s.rs       # 繁→简转换表
│       └── engine/
│           ├── mod.rs       # Segment/TranscribeOptions
│           ├── whisper_cpp.rs     # 标准引擎
│           ├── python_whisper.rs  # 兼容引擎
│           └── download.rs        # 模型下载
└── build/                   # 前端构建产物
    ├── index.html
    └── assets/
```

---

## 六、BASELINE.toml 基线锁定体系

### 三级文件分类

| 区域 | 文件 | 规则 |
|------|------|------|
| **[lock] 冻结区** (19) | platform.rs, engine/, postprocess.rs, t2s.rs, lib.rs, main.rs, main.ts, App.svelte, DropZone.svelte, FileList.svelte, ResultView.svelte, SettingsPanel.svelte, api.js, app.css, vite-env.d.ts, BASELINE.toml, workflow.md, release.md | **禁止 `write_to_file`**；只能用 `replace_in_file` 做最小增量改动 |
| **[stable] 稳定区** (8) | Cargo.toml, tauri.conf.json, package.json, svelte.config.js, vite.config.js, tsconfig.json, auto-push-on-version.sh, post-commit | 仅添加依赖/改配置时修改 |
| **[active] 活跃区** (3) | commands.rs, transcribe.rs, deploy.rs | 允许自由修改 |

### 硬性约束

```
max_files_per_change = 3        # 单次操作 ≤3 个文件
require_baseline_check = true   # 每次写操作前检查 BASELINE.toml
lock_violation = "abort"        # 违规 → 中止
ui_bug_no_config_change         # 修 UI bug 不动 Cargo.toml
engine_bug_no_ui_change         # 修引擎 bug 不动 UI 组件
```

---

## 七、开发工作流

### 7.1 Scripts 脚本目录职责

| 脚本 | 角色 | 触发条件 | 状态 |
|------|------|----------|------|
| `bundle-all.sh` | **主依赖打包** — 递归收集 ffmpeg + whisper-cli 及所有 brew dylib → `@loader_path` + ad-hoc 签名 | 构建 DMG 前 / 新增外部依赖后 | ✅ 活跃 |
| `build-dmg.sh` | **主构建入口** — 杀旧进程 → 备份 → bundle-all → tauri build → 嵌入 README → create-dmg → 质量审计 | 发布新版本时 | ✅ 活跃 |
| `build.sh` | 构建入口转发 → 调用 `build-dmg.sh` | `npm run dist` | ✅ 活跃 |
| `auto-push-on-version.sh` | **版本检测自动推送** — 检测 HEAD commit 版本文件变更，自动 push 双仓库 | git post-commit hook | ✅ 活跃 |
| `dev-tail.sh` | DEV 日志监控 — `tail -f /tmp/voice2text-debug.log` | DEV 排查问题时 | ✅ 活跃 |
| `bundle-ffmpeg.sh` | 旧版 ffmpeg 打包（仅处理 ffmpeg，不处理 whisper-cli） | — | ❌ 已废弃 |
| `copy-dylibs.sh` | 旧版 dylib 复制（硬编码路径，不做 install_name_tool） | — | ❌ 已废弃 |
| `copy-ffmpeg.sh` | 旧版 ffmpeg 复制（硬编码路径，不做 install_name_tool） | — | ❌ 已废弃 |
| `fix-dylibs.sh` | 旧版 dylib 引用修复（已被 bundle-all.sh 内置步骤替代） | — | ❌ 已废弃 |

### 7.2 修改 lock 区文件的标准流程

1. 只做 `replace_in_file`：find-and-replace 最小粒度
2. 单次改动 ≤3 个文件
3. 不改无关文件、不新增无关依赖
4. 改动后 `cargo build` 验证 → 人工审查 diff → 提交

### 7.3 大规模改动 lock 文件的流程

1. 先更新 `BASELINE.toml` 将目标文件移至 `active` 区
2. 自由修改
3. 验证通过后将文件移回 `lock` 区
4. 更新 `BASELINE.toml`

### 7.4 常规开发

```bash
npm run tauri dev    # 开发模式（热更新）
npm run build        # 仅构建前端
npm run tauri build  # 打包 macOS .dmg
```

### 7.5 macOS 打包后去隔离

```bash
xattr -d com.apple.quarantine /path/to/voice2text.app
```

---

## 八、关键 Trap 速查

### 8.1 运行时 Trap

| # | 症状 | 原因 | 修复 |
|---|------|------|------|
| 1 | 拖入文件无反应 | `dragDropEnabled` 默认 true | `tauri.conf.json` → `dragDropEnabled: false` |
| 2 | 另存为按钮无效 | WKWebView 不支持 blob URL | Rust command + 原生 save() |
| 3 | 某段 JS 静默不执行 | 函数名 `isTauri()` 冲突 | 换名字 |
| 4 | cargo build 卡死 0% CPU | 中国网络无镜像 | `~/.cargo/config.toml` 配 rsproxy.cn |
| 5 | RTF/Word 文件空白 | 字体 `Microsoft YaHei` 在 macOS 不存在 | `\fnil\fcharset134` 系统默认中文字体 |
| 6 | pkill 后新进程起不来 | pkill + open 竞态 | `sleep 2` |
| 7 | 修一个 bug 引入三个 | AI 重写整个文件 | 基线锁定：只用 `replace_in_file` |
| 8 | 打包 app 找不到 ffmpeg | GUI app 没有终端 PATH | bundle 内自包含 + @loader_path |
| 9 | bundle 内 ffmpeg 被 SIGKILL | Apple Silicon 无签名 Gatekeeper 拦截 | ad-hoc codesign 所有二进制 |
| 10 | 引擎显示"就绪"但转写失败 | 只检查任意模型存在，未匹配平台推荐模型 | `get_engine_status()` 检查 `recommended_model` |

### 8.2 构建管道 Trap

| # | 症状 | 原因 | 修复 |
|---|------|------|------|
| B1 | whisper-cli 冒烟测试失败，libwhisper.1.dylib 找不到 | `@rpath` 依赖未收集（rpath = `@loader_path/../lib`） | bundle-all.sh 预收集 `LC_RPATH` + `@rpath` 解析 |
| B2 | rpath 解析失败，路径含垃圾字符 | `otool -l` 输出 `path /xxx (offset 12)` | `sed 's/ (offset.*)//'` |
| B3 | npx tauri build 的 DMG 阶段报错 | Tauri 2 内置 DMG 打包不稳定 | `targets: ["app"]` + 改用 `create-dmg` CLI |
| B4 | build-dmg.sh 语法错误 | zsh 不允许 `local` 在函数体外 | 改为普通变量名 |

---

## 九、商业级交付标准与闭环

### 9.1 核心理念：零用户配置交付

> **交付 != 代码能跑**。一个合格的商业级交付要求：用户拿到 DMG 后，双击、拖入 /Applications、运行，三个动作即可使用。任何"去终端敲 brew install xxx"、"手动下载 xxx 放到某个目录"、"配置环境变量 PATH"都是交付缺陷。

| 级别 | 描述 | 用户操作 |
|------|------|----------|
| ❌ 不可交付 | 需要用户手动安装 3+ 个外部依赖，需要配置 PATH/环境变量 | 10+ 步 |
| ⚠️ 勉强可交付 | 需要用户 brew install 1-2 个常见工具 | 3-5 步 |
| ✅ **商业级** | 所有依赖内嵌 bundle，零手动安装 | 3 步（双击→拖入→运行） |

### 9.2 依赖自包含体系

#### 问题根源

macOS GUI app 启动时不继承终端 PATH。即使用户用 `brew install ffmpeg` 安装了工具，打包后的 `.app` 内 `which ffmpeg` 返回 `None`，因为 `/opt/homebrew/bin` 不在 GUI 进程的 PATH 中。

#### 解决方案：三级回退路径解析

每个外部依赖的查找函数必须遵循**三级回退**：

```
1. @loader_path 自包含  →  app bundle 内嵌版本（打包后唯一可靠来源）
2. which 探查          →  dev 模式下 PATH 完整时可用
3. 硬编码 brew 路径    →  最后兜底（/opt/homebrew/bin, /usr/local/bin）
```

已实施的文件：

| 文件 | 函数 | 覆盖依赖 |
|------|------|----------|
| `engine/whisper_cpp.rs` | `which_ffmpeg()` | ffmpeg |
| `engine/python_whisper.rs` | `resolve_python_paths()` / `resolve_pip_candidates()` | python3, pip3 |
| `platform.rs` | `resolve_python_candidates()` | python3 (MPS 检测) |

#### ffmpeg 自包含打包原理

```
原始 ffmpeg (brew)
  ├── ffmpeg binary          → @rpath 指向 /opt/homebrew/opt/...
  ├── libavcodec.62.dylib    → 又依赖 libx264, libx265...
  ├── libavformat.62.dylib   → 又依赖 libssl, libcrypto...
  └── ... (递归依赖树，共 18 个 brew dylib)

↓ bundle-all.sh 处理 ↓

bundle 内 ffmpeg (自包含)
  ffmpeg-bundle/
    ├── ffmpeg               → @loader_path/libavcodec.62.dylib
    ├── libavcodec.62.dylib  → @loader_path/libx264.165.dylib
    ├── libmp3lame.0.dylib   → @loader_path/libmpg123.0.dylib  (间接依赖！)
    └── ... 19 个文件，全部 @loader_path 相对路径，零外部引用
```

关键陷阱：**必须递归收集**。`otool -L ffmpeg` 只显示第一层依赖。需要用 while 循环对 BUNDLED_DIR 内所有文件反复执行 `otool -L` 直到不再发现新的 brew dylib。

### 9.3 构建管道 (Delivery Pipeline)

```
npm run dist  ────► scripts/build.sh
                         │
                         └─► scripts/build-dmg.sh
                              │
                              ├─ [0/5] 杀旧进程 + 备份旧版本
                              │      pkill -f 小柳语音转写.app
                              │      sleep 2
                              │      cp -R → /Applications/小柳语音转写-backups/
                              │
                              ├─ [1/5] bundle-all.sh
                              │      ├── 动态发现 ffmpeg/whisper-cli (which + brew --prefix)
                              │      ├── 解析源二进制 @rpath 预收集依赖 (LC_RPATH)
                              │      ├── 递归收集所有 brew dylib (while 直到零新增)
                              │      ├── install_name_tool → @loader_path
                              │      ├── ad-hoc codesign 所有二进制
                              │      └── 冒烟测试: ffmpeg -version + whisper-cli --version
                              │
                              ├─ [2/5] npx tauri build (targets: ["app"])
                              │      ├── cargo build --release
                              │      ├── tauri.conf.json resources → bundle 内嵌
                              │      └── 生成 .app bundle
                              │
                              ├─ [3/5] 嵌入 README.txt
                              │      └── 写入 DMG staging 目录
                              │
                              ├─ [4/5] create-dmg 打包
                              │      └── 生成 DMG + 时间戳副本
                              │
                              └─ [5/5] 质量门禁审计
                                     ├── G2: bundle 文件完整性 (ffmpeg ≥15, whisper-cli ≥5)
                                     ├── G3: 自包含可运行
                                     ├── G4: 零外部 dylib 引用
                                     ├── G5: DMG 存在且 >10MB
                                     └── G6: ad-hoc 签名
```

#### tauri.conf.json 资源配置

```json
"bundle": {
  "resources": ["resources/ffmpeg-bundle/"]
}
```

Tauri v2 会将 `src-tauri/resources/ffmpeg-bundle/` 目录完整复制到 `.app/Contents/Resources/resources/ffmpeg-bundle/`。

#### Rust 端运行时路径解析

```rust
// 打包后路径：.app/Contents/MacOS/voice2text 同级 ../Resources/resources/ffmpeg-bundle/ffmpeg
let exe_dir = std::env::current_exe()?.parent()?;  // → Contents/MacOS/
let bundled = exe_dir.join("../Resources/resources/ffmpeg-bundle/ffmpeg");
```

### 9.4 质量门禁 (Quality Gates)

每次交付前必须通过以下检查点，**全部绿灯才可发布**：

| # | 检查项 | 验证命令/方式 | 通过标准 |
|---|--------|--------------|----------|
| G1 | 零 warning 编译 | `cargo build --manifest-path src-tauri/Cargo.toml` | 零 error、零 warning |
| G2 | bundle 文件完整性 | `ls src-tauri/resources/ffmpeg-bundle/ \| wc -l` | ≥18 个文件 |
| G3 | ffmpeg 自包含可运行 | `src-tauri/resources/ffmpeg-bundle/ffmpeg -version` | 正常输出版本号 |
| G4 | 无外部 dylib 引用 | `otool -L bundle内ffmpeg \| grep /opt/homebrew` | 零输出（全部 @loader_path） |
| G5 | DMG 生成成功 | `ls -lh target/release/bundle/dmg/*.dmg` | 文件存在且 >50MB |
| G6 | bundle 内 ffmpeg 签名 | `codesign -dv app内ffmpeg` 或直接运行 | 不被 Gatekeeper 拦截 |
| G7 | BASELINE.toml 完整 | 检查 lock=19, stable=8, active=3 | 数量正确 |
| G8 | 无死代码/未使用导入 | `cargo build` 输出 | 零 `unused import` / `dead_code` warning |

### 9.5 交付产物清单

```
target/release/bundle/
├── dmg/
│   ├── 小柳语音转写_0.2.0_aarch64.dmg           ← 交付物
│   └── 小柳语音转写_0.2.0_aarch64_20260716-xxxx.dmg  ← 时间戳副本
└── macos/
    └── 小柳语音转写.app/
        └── Contents/
            ├── MacOS/
            │   └── voice2text              ← 主程序
            └── Resources/
                └── resources/
                    ├── ffmpeg-bundle/       ← 自包含依赖 (19 files)
                    │   ├── ffmpeg
                    │   ├── libavcodec.62.dylib
                    │   ├── libavformat.62.dylib
                    │   ├── libavutil.60.dylib
                    │   ├── libswresample.6.dylib
                    │   ├── libswscale.9.dylib
                    │   ├── libx264.165.dylib
                    │   ├── libx265.215.dylib
                    │   ├── libmp3lame.0.dylib
                    │   ├── libmpg123.0.dylib         ← 间接依赖
                    │   ├── libssl.3.dylib
                    │   ├── libcrypto.3.dylib
                    │   └── ...
                    └── whisper-cli-bundle/  ← 自包含依赖 (5 files)
                        ├── whisper-cli
                        ├── libwhisper.1.dylib
                        ├── libggml.0.dylib
                        ├── libggml-base.0.dylib
                        └── libomp.dylib
```

### 9.6 用户安装指南（可写入 README）

```bash
# 1. 双击 DMG，拖入 /Applications
# 2. 去隔离（首次运行前执行一次）
xattr -d com.apple.quarantine /Applications/小柳语音转写.app
# 3. 双击运行
```

用户**不需要**：
- `brew install ffmpeg` — 已内嵌
- `brew install python` — 引擎自带路径回退
- 配置任何 PATH 或环境变量
- 下载模型文件（应用内一键部署）

### 9.7 Git 双仓库发布流程

项目托管在 GitHub (主) + Gitee (镜像)，每次发布需同步：

```bash
# 1. 确认所有变更已提交
git status

# 2. 双仓库推送主分支
git push origin main
git push gitee main

# 3. 创建版本 tag 并双仓库推送
git tag -a "v0.2.0" -m "release: v0.2.0 商业级DMG交付"
git push origin --tags
git push gitee --tags

# 4. 上传 DMG 到 GitHub Releases
#    通过 Web UI 或 gh release create v0.2.0 --title "v0.2.0" --notes "..." 小柳语音转写_0.2.0_aarch64.dmg
```

### 9.9 新增外部依赖的标准流程

当需要引入新的外部可执行文件（如 `sox`、`yt-dlp`）时，必须按以下流程纳入自包含体系：

```
1. 评估必要性
   └── 能否用纯 Rust crate 替代？（如 symphonia 替代 ffmpeg 音频解码）
        ├── 能 → 加 Cargo.toml 依赖，不进 bundle
        └── 不能 → 继续

2. 收集依赖树
   └── 递归 otool -L 直到没有新的 brew dylib

3. 加入 bundle-all.sh
   └── 更新脚本，将新二进制及其 dylib 一起处理

4. 运行时三级回退
   └── 在对应 engine/*.rs 中实现 @loader_path / which / 硬编码 三级查找

5. 更新 tauri.conf.json resources
   └── 确认 bundle.resources 覆盖新目录

6. 通过全部 8 项质量门禁
   └── 尤其是 G3 (自包含可运行) 和 G4 (零外部引用)

7. 更新本手册
   └── 更新 9.5 产物清单、9.2 依赖表
```

### 9.10 交付检查清单 (Release Checklist)

发布新版本前逐项勾选：

- [ ] G1: `cargo build` 零 error 零 warning
- [ ] G2: bundle 文件 ≥15 (ffmpeg) + ≥5 (whisper-cli)
- [ ] G3: bundle 内 `ffmpeg -version` + `whisper-cli --version` 正常
- [ ] G4: `otool -L` 所有二进制零 `/opt/homebrew` 引用
- [ ] G5: DMG 文件存在且 >10MB
- [ ] G6: bundle 内所有二进制已 ad-hoc 签名
- [ ] `dragDropEnabled` 已设为 false（Trap A）
- [ ] 版本号已更新（`tauri.conf.json` + `Cargo.toml` + `build-dmg.sh` 中的 VERSION）
- [ ] `git push origin main && git push gitee main` 双仓库推送
- [ ] `git tag -a "v{版本号}" && git push origin --tags && git push gitee --tags`
- [ ] GitHub Releases 上传 DMG 附件
- [ ] 在干净环境（无 brew ffmpeg 的 Mac）验证 DMG 可运行
