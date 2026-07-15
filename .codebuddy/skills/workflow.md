# 开发工作流 Skill

> 小柳语音转写 (voice2text) 项目的开发闭环流程。
> 此文件是 AI 辅助开发的"操作手册"，每次开工前必须先读。

---

## 一、五阶段闭环 SOP

```
Develop → Verify → Deliver → Release → Close
```

### 1.1 Develop（开发）

| 步骤 | 操作 | 工具 |
|------|------|------|
| 1 | 读 BASELINE.toml，确认目标文件区域 | read_file |
| 2 | lock 区：只用 replace_in_file 最小增量 | replace_in_file |
| 3 | stable 区：确认改动必要性（修 UI 不动 Cargo.toml） | replace_in_file |
| 4 | active 区：自由修改 | write_to_file / replace_in_file |
| 5 | 单次改动 ≤3 文件 | 硬约束，跨区自动检查 |

**红线**：
- lock 区文件禁用 write_to_file 全量覆盖
- 修 UI bug 不得动 Cargo.toml，修引擎 bug 不得动 UI 组件
- 对 lock 文件做大范围修改 → 先更新 BASELINE.toml 移至 active

### 1.2 Verify（验证）

```bash
# G1: 零 warning 编译
cargo build --manifest-path src-tauri/Cargo.toml

# diff review 确认无无关变更
git diff --stat

# DEV 模式验证正向用例
npm run tauri dev

# 验证引擎状态检测（平台推荐模型匹配）
# 不能只检查"任意模型存在"，必须验证 platform::recommended_model
```

### 1.3 Deliver（交付闭环）

1. 验证完整功能路径：拖入 → 引擎检测 → 模型存在 → 转写成功 → 结果预览 → 导出
2. 确认 8 项质量门禁 G1-G8 全部绿灯
3. 更新 BASELINE.toml（新文件入 lock，注明原因+时间）
4. 更新 PROJECT-HANDBOOK.md（日志/新Trap/架构变更）
5. 沉淀经验到记忆系统

### 1.4 Release（发布）

1. 确认质量门禁全部通过
2. 更新版本号：`tauri.conf.json` + `Cargo.toml` + `scripts/build-dmg.sh`
3. 构建 DMG：`zsh scripts/build-dmg.sh`
4. git tag + 双仓库推送
5. GitHub Releases 上传 DMG

### 1.5 Close（经验闭环）

1. 盘点新 Trap → 写入 PROJECT-HANDBOOK.md
2. 调整 BASELINE.toml 文件分类
3. 强化记忆规则
4. 输出闭环总结

---

## 二、BASELINE.toml 三级文件分类

| 区域 | 数量 | 规则 | 典型文件 |
|------|------|------|----------|
| **[lock]** | 19 | 禁止 write_to_file，只能 replace_in_file | platform.rs, engine/, UI 组件, skills |
| **[stable]** | 8 | 仅添加依赖/改配置时修改 | Cargo.toml, tauri.conf.json, auto-push |
| **[active]** | 3 | 自由修改 | commands.rs, transcribe.rs, deploy.rs |

**硬约束**：
- `max_files_per_change = 3`
- `require_baseline_check = true`
- `lock_violation = "abort"`

---

## 三、Debug 三层策略

| 层级 | 命名 | 适用场景 | 方式 |
|------|------|----------|------|
| L1 | 直接修复 | 语法/类型/路径错误 | 最小改动直接 fix |
| L2 | 引导诊断 | 逻辑错误/状态不一致 | 给 3-5 种可能原因+验证方法 |
| L3 | 手动深入 | 底层依赖/编译器/OS Bug | 传统工具排查，保留完整日志 |

每次修复后必须跑：正向用例 → 边界用例 → 异常用例

---

## 四、质量门禁 G1-G8

| # | 检查项 | 标准 |
|---|--------|------|
| G1 | 零 warning 编译 | 零 error、零 warning |
| G2 | bundle 文件完整 | ffmpeg ≥15, whisper-cli ≥5 |
| G3 | 自包含可运行 | bundle 内 ffmpeg -version 正常 |
| G4 | 零外部 dylib 引用 | otool -L 零 /opt/homebrew |
| G5 | DMG 存在 | 文件 >10MB |
| G6 | ad-hoc 签名 | codesign 通过 |
| G7 | BASELINE.toml 完整 | lock=19, stable=8, active=3 |
| G8 | 无死代码 | 零 unused import / dead_code warning |

---

## 五、关键红线（never break）

1. 引擎状态不能只看"任意模型存在"，必须检查**平台推荐模型**是否匹配
2. lock 文件每次改动后 diff review
3. 修 bug 同步更新 BASELINE.toml 和记忆
4. 手动下载 hack 不走已有部署系统 = 埋坑
5. 永远使用已有部署系统，不手动 curl/wget 下载模型
6. 平台推荐模型精确匹配：M2 Pro → small, M1 → base, Intel → tiny

---

## 六、Git 双仓库规范

```bash
# 每次推送必须双仓库
git push origin main
git push gitee main

# Tag 也要双推
git push origin --tags
git push gitee --tags
```

- `origin` → git@github.com:jaminliu89/voice2text.git (主)
- `gitee` → https://gitee.com/jaminkim/voice2text.git (镜像)

### 6.1 版本变更自动推送

```
git commit (含版本号变更)
    │
    └── .githooks/post-commit
         └── scripts/auto-push-on-version.sh
              ├── 检测 tauri.conf.json / Cargo.toml 是否变更
              ├── 版本未变 → "跳过自动推送" 静默退出
              └── 版本已变 → git push origin + gitee + 提示打 tag
```

安装 hook：`git config core.hooksPath .githooks`

每次 commit 后自动运行，非版本变更的普通 commit 不影响开发节奏。

---

## 七、Tauri v2 5 大陷阱

| Trap | 症状 | 修复 |
|------|------|------|
| A | 拖入文件无反应 | dragDropEnabled: false |
| B | 另存为按钮无效 | Rust command + 原生 save() |
| C | 整段 JS 静默死亡 | 函数名不能叫 isTauri() |
| D | 新进程被刚 spawn 就杀 | pkill 后 sleep 2 |
| E | 引擎假就绪 | 检查 recommended_model 匹配 |

---

## 八、DMG 构建 4 大陷阱

| Trap | 症状 | 修复 |
|------|------|------|
| B1 | libwhisper.1.dylib 找不到 | 解析 LC_RPATH + @rpath 预收集 |
| B2 | rpath 解析含 offset 后缀 | sed 's/ (offset.*)//' |
| B3 | Tauri DMG 构建失败 | targets: ["app"] + create-dmg CLI |
| B4 | build-dmg.sh 语法错误 | local 只能在函数体内用 |

---

## 九、新增外部依赖标准流程

```
1. 评估必要性 → 能用纯 Rust crate？
2. 递归收集依赖树（otool -L while 循环）
3. 加入 bundle-all.sh
4. 运行时三级回退（@loader_path → which → 硬编码）
5. 更新 tauri.conf.json resources
6. 通过全部 8 项质量门禁
7. 更新 PROJECT-HANDBOOK.md
```
