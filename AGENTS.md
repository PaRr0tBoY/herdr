# Hive — Release & Deployment

## 仓库结构

| 仓库 | 用途 |
|---|---|
| `PaRr0tBoY/hive` (本仓库) | 源代码、CI、网站源码 |
| `PaRr0tBoY.github.io` (`~/Documents/repo/PaRr0tBoY.github.io`) | GitHub Pages 博客，托管安装脚本 |

## 发布流程

1. **准备**：`cargo fmt` → 提交所有改动到 feature 分支
2. **合并**：`git checkout master && git merge <branch>`
3. **版本号**：编辑 `Cargo.toml` 的 `version` 字段（CalVer：`YYYY.M.D`）
4. **推送**：`git push origin master`
5. **打标签**：`git tag -a v<version> -m "v<version>" && git push origin v<version>`
6. **CI**：tag 推送触发 `release-hive.yml`；5 个平台并行构建 → 创建 GitHub Release

## CI 流水线 (release-hive.yml)

```
preflight (fmt+clippy) → build (win/linux/mac × 5) → publish (GitHub Release)
                                                     → close-released-issues
                                                     → update-latest-json (需 RELEASE_DEPLOY_KEY)
```

## 安装脚本同步

安装脚本有**两份**，发布后需手动同步：

| 文件 | hive 仓库 | 博客仓库 |
|---|---|---|
| `install.sh` | `scripts/install.sh` | `product/Hive/install/install.sh` |
| `install.ps1` | `scripts/install.ps1` | `product/Hive/install/install.ps1` |
| `latest.json` | `website/latest.json` | `product/Hive/install/latest.json` |

发布新版本后：
1. 从 GitHub Release 获取 SHA256：`gh release view v<version> --json assets`
2. 更新博客仓库的 `latest.json`
3. 如果安装脚本有改动，同步到博客仓库

## 工具链

- Rust: `1.96.1-x86_64-pc-windows-msvc`（`rust-toolchain.toml` 指定 channel=`1.96.1`，默认选 MSVC）
- Zig: `0.15.2`（libghostty-vt 构建）
- `rustup override` 不会同步到 git worktree——每个工作树需单独设置

## 常见问题

- **子工作树编译失败**：`rust-toolchain.toml` 的 `channel` 不带目标三重奏时，rustup 可能选 GNU。确保 MSVC 工具链已安装，或给工作树设 `rustup override`
- **`update-latest-json` CI 失败**：需要 `RELEASE_DEPLOY_KEY` secret（SSH 私钥 + Deploy Key with write access）
- **安装脚本 URL**：README 指向 `PaRr0tBoY.github.io/product/Hive/install/`，不要用 `herdr.dev`
