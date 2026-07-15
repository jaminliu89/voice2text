# 发布流程 Skill

> voice2text 项目的版本发布与自动推送机制。

---

## 一、版本号管理

**三处同步**（每次发布前必须全部更新）：

| 文件 | 字段 | 示例 |
|------|------|------|
| `src-tauri/tauri.conf.json` | `"version"` | `"0.2.0"` |
| `src-tauri/Cargo.toml` | `version` | `"0.2.0"` |
| `scripts/build-dmg.sh` | `VERSION` 变量 | `VERSION="0.2.0"` |

---

## 二、完整发布流程

```bash
# Step 1: 确认质量门禁 G1-G8
cargo build --manifest-path src-tauri/Cargo.toml  # G1 零 warning
# ... G2-G8 见 workflow.md

# Step 2: 更新版本号（三处同步）
# src-tauri/tauri.conf.json → "version": "0.2.1"
# src-tauri/Cargo.toml      → version = "0.2.1"
# scripts/build-dmg.sh      → VERSION="0.2.1"

# Step 3: 构建 DMG
zsh scripts/build-dmg.sh

# Step 4: 提交版本变更 + 自动推送
# commit 后 post-commit hook 自动检测版本变更 → push 双仓库
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml scripts/build-dmg.sh
git commit -m "release: v0.2.1 {描述}"

# Step 5: 创建 tag（手动，hook 不自动打 tag）
git tag -a "v0.2.1" -m "release: v0.2.1 {描述}"

# Step 6: 推送 tag 到双仓库
git push origin --tags && git push gitee --tags

# Step 7: GitHub Releases 上传 DMG
gh release create v0.2.1 --title "v0.2.1" --notes "..." \

  小柳语音转写_0.2.1_aarch64.dmg
```

> **关键**：Step 4 commit 后 hook 自动触发 `auto-push-on-version.sh`，检测到版本文件变更后自动 push origin+gitee main 分支。tag 仍需手动打并推送。

---

## 三、自动推送机制

### 3.1 版本检测脚本

`scripts/auto-push-on-version.sh`：

- 检测 HEAD commit 是否包含版本号变更（tauri.conf.json / Cargo.toml）
- 如果版本已变更且尚未推送 → 自动推送到双仓库
- 如果版本已变更且尚未打 tag → 提示用户打 tag

### 3.2 Git Hook 触发

`.githooks/post-commit`：

```bash
#!/bin/bash
# 每次 commit 后自动检测版本变更
scripts/auto-push-on-version.sh
```

安装 hook：
```bash
git config core.hooksPath .githooks
```

### 3.3 工作流

```
git commit → post-commit hook
              │
              ├── 检测 tauri.conf.json / Cargo.toml 是否变更
              │
              ├── 版本未变 → 静默退出
              │
              └── 版本已变 → git push origin main
                           → git push gitee main
                           → 提示: 是否打 tag？
```

---

## 四、回滚操作

```bash
# 回到任意稳定版本
git checkout stable-v{N}

# 恢复单个文件
git checkout stable-v{N} -- <file>

# 查看所有稳定版本
git tag -l "stable-*"
```

---

## 五、交付后去隔离

```bash
xattr -d com.apple.quarantine /Applications/小柳语音转写.app
```

---

## 六、发布检查清单

- [ ] G1: `cargo build` 零 error 零 warning
- [ ] G2: bundle 文件 ≥15 (ffmpeg) + ≥5 (whisper-cli)
- [ ] G3: bundle 内 ffmpeg -version 正常
- [ ] G4: otool -L 零 /opt/homebrew 引用
- [ ] G5: DMG >10MB
- [ ] G6: ad-hoc 签名
- [ ] G7: BASELINE.toml lock=19, stable=8, active=3
- [ ] G8: 零 unused import / dead_code
- [ ] dragDropEnabled: false (Tauri trap A)
- [ ] 版本号三处同步（tauri.conf.json + Cargo.toml + build-dmg.sh）
- [ ] git commit 后 hook 自动推送到 origin + gitee main
- [ ] git tag + git push --tags 双仓库
- [ ] GitHub Releases 上传 DMG
- [ ] 干净环境验证 DMG 可运行
- [ ] 更新 PROJECT-HANDBOOK.md 开发日志
- [ ] 更新记忆系统沉淀新 Trap/经验
