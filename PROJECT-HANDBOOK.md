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
| **[lock] 冻结区** (17) | platform.rs, engine/, postprocess.rs, t2s.rs, lib.rs, main.rs, main.ts, App.svelte, DropZone.svelte, FileList.svelte, ResultView.svelte, SettingsPanel.svelte, api.js, app.css, vite-env.d.ts, BASELINE.toml | **禁止 `write_to_file`**；只能用 `replace_in_file` 做最小增量改动 |
| **[stable] 稳定区** (6) | Cargo.toml, tauri.conf.json, package.json, svelte.config.js, vite.config.js, tsconfig.json | 仅添加依赖/改配置时修改 |
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

### 修改 lock 区文件的标准流程

1. 只做 `replace_in_file`：find-and-replace 最小粒度
2. 单次改动 ≤3 个文件
3. 不改无关文件、不新增无关依赖
4. 改动后 `cargo build` 验证 → 人工审查 diff → 提交

### 大规模改动 lock 文件的流程

1. 先更新 `BASELINE.toml` 将目标文件移至 `active` 区
2. 自由修改
3. 验证通过后将文件移回 `lock` 区
4. 更新 `BASELINE.toml`

### 常规开发

```bash
npm run tauri dev    # 开发模式（热更新）
npm run build        # 仅构建前端
npm run tauri build  # 打包 macOS .dmg
```

### macOS 打包后去隔离

```bash
xattr -d com.apple.quarantine /path/to/voice2text.app
```

---

## 八、关键 Trap 速查

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
                         ├─ [1/3] bundle-all.sh
                         │      ├── 递归收集所有 brew dylib
                         │      ├── install_name_tool → @loader_path
                         │      ├── ad-hoc codesign 所有二进制
                         │      └── 冒烟测试: ffmpeg -version
                         │
                         ├─ [2/3] npx tauri build
                         │      ├── cargo build --release
                         │      ├── tauri.conf.json resources → bundle 内嵌
                         │      ├── 生成 .app bundle
                         │      └── 打包 .dmg
                         │
                         └─ [3/3] 签名验证
                                ├── codesign bundle 内所有 dylib + ffmpeg
                                ├── ffmpeg -version 确认可执行
                                └── 输出 DMG 路径
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
| G7 | BASELINE.toml 完整 | 检查 lock=17, stable=6, active=3 | 数量正确 |
| G8 | 无死代码/未使用导入 | `cargo build` 输出 | 零 `unused import` / `dead_code` warning |

### 9.5 交付产物清单

```
target/release/bundle/
├── dmg/
│   └── 小柳语音转写_0.1.0_aarch64.dmg    ← 交付物
└── macos/
    └── 小柳语音转写.app/
        └── Contents/
            ├── MacOS/
            │   └── voice2text              ← 主程序
            └── Resources/
                └── resources/
                    └── ffmpeg-bundle/       ← 自包含依赖 (19 files)
                        ├── ffmpeg           (431KB)
                        ├── libavcodec.62.dylib
                        ├── libavformat.62.dylib
                        ├── libavutil.60.dylib
                        ├── libswresample.6.dylib
                        ├── libswscale.9.dylib
                        ├── libx264.165.dylib
                        ├── libx265.215.dylib
                        ├── libmp3lame.0.dylib
                        ├── libmpg123.0.dylib         ← 间接依赖
                        ├── libssl.3.dylib
                        ├── libcrypto.3.dylib
                        ├── ... (共 18 个 dylib)
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

### 9.7 新增外部依赖的标准流程

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

### 9.8 交付检查清单 (Release Checklist)

发布新版本前逐项勾选：

- [ ] G1: `cargo build` 零 error 零 warning
- [ ] G2: bundle 文件 ≥18
- [ ] G3: bundle 内 `ffmpeg -version` 正常
- [ ] G4: `otool -L` 所有二进制零 `/opt/homebrew` 引用
- [ ] G5: DMG 文件存在且完整
- [ ] G6: bundle 内所有二进制已 ad-hoc 签名
- [ ] G7: `BASELINE.toml` 三级文件数量正确
- [ ] G8: 零 dead_code / unused_import
- [ ] 在干净环境（无 brew ffmpeg 的 Mac）验证 DMG 可运行
- [ ] `dragDropEnabled` 已设为 false（Trap A）
- [ ] 版本号已更新（`tauri.conf.json` + `Cargo.toml`）
- [ ] 更新日志已记录本次变更
