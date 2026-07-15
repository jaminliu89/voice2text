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
# Step 1: 确认质量门禁 G1-G6
cargo build --manifest-path src-tauri/Cargo.toml  # G1
# ... G2-G6 见 workflow.md

# Step 2: 更新版本号（三处）
# tauri.conf.json / Cargo.toml / build-dmg.sh

# Step 3: 构建 DMG
zsh scripts/build-dmg.sh

# Step 4: 提交版本变更
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml scripts/build-dmg.sh
git commit -m "release: v0.2.0 {描述}"

# Step 5: 创建 tag
git tag -a "v0.2.0" -m "release: v0.2.0 {描述}"

# Step 6: 双仓库推送
git push origin main && git push gitee main
git push origin --tags && git push gitee --tags

# Step 7: GitHub Releases
# 通过 Web UI 上传 DMG 附件
# 或: gh release create v0.2.0 --title "v0.2.0" --notes "..." DMG文件
```

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
- [ ] dragDropEnabled: false
- [ ] 版本号三处同步
- [ ] git push origin main && git push gitee main
- [ ] git tag + git push --tags
- [ ] GitHub Releases 上传 DMG
