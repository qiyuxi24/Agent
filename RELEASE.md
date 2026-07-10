# 发布操作指南

本文档描述 Agent Desktop 的完整发布流程，包括版本号管理、构建、签名和分发。

---

## 目录

- [前提条件](#前提条件)
- [版本号规范](#版本号规范)
- [快速发布（推荐）](#快速发布推荐)
- [手动发布](#手动发布)
- [CI 自动发布](#ci-自动发布)
- [发布检查清单](#发布检查清单)
- [安装包分发](#安装包分发)
- [故障排查](#故障排查)

---

## 前提条件

### 编译环境

| 工具 | 最低版本 | 用途 |
|------|---------|------|
| Node.js | 18+ | 前端编译（Vite + TypeScript + React） |
| Rust | stable (MSVC) | 编译后端 → .exe |
| Visual Studio Build Tools | 2022 | C++ 链接器（Rust 编译需要） |
| Git | 2.30+ | 版本管理和标签 |

> **Rust 必须选 MSVC 工具链**，不是 MinGW。验证：`rustup show | findstr msvc`

### 快速验证

```powershell
node --version     # 应 >= 18
rustc --version    # 应显示 stable-x86_64-pc-windows-msvc
cargo --version
git --version
```

---

## 版本号规范

遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)：

```
主版本.次版本.修订号
  0  .  2  .  0
```

| 改动类型 | 示例 | 升级规则 |
|----------|------|---------|
| Bug 修复 | 修复崩溃、逻辑错误 | 修订号 +1 → 0.2.0 → 0.2.1 |
| 新功能（兼容） | 新增面板、新增命令 | 次版本 +1 → 0.2.0 → 0.3.0 |
| 破坏性变更 | 删除 API、格式不兼容 | 主版本 +1 → 0.x → 1.0.0 |

**当前阶段（0.x）**：次版本号兼做功能版本，修订号用于修复。1.0 之前允许小范围不兼容变更。

> 版本号唯一出现在 `agent-desktop/src-tauri/tauri.conf.json` 的 `version` 字段。

---

## 快速发布（推荐）

使用 `scripts/release.bat` 一键发布：

```batch
# 本地构建 + 自动推送（最常用）
scripts\release.bat 0.2.0

# 预览模式（不实际操作，查看会执行什么）
scripts\release.bat 0.2.0 --dry

# 只打 tag 推送，由 CI 构建
scripts\release.bat 0.2.0 --skip
```

脚本会自动完成：
1. 更新 `tauri.conf.json` 中的版本号
2. `git commit` + `git tag v0.2.0`
3. 本地编译打包
4. 复制安装包到 `releases/` 目录
5. `git push` + 推送标签

---

## 手动发布

如果不想用脚本，按以下步骤手动操作：

### 第一步：更新版本号

编辑 `agent-desktop/src-tauri/tauri.conf.json`，修改 `version` 字段：

```json
{
  "version": "0.2.0",
  ...
}
```

### 第二步：更新 CHANGELOG

在 `CHANGELOG.md` 中将 `[Unreleased]` 改为新版本号：

```markdown
## [0.2.0] - 2026-07-10
```

### 第三步：提交和打标签

```bash
git add CHANGELOG.md agent-desktop/src-tauri/tauri.conf.json
git commit -m "chore: bump version to v0.2.0"
git tag -a v0.2.0 -m "Release v0.2.0"
```

### 第四步：构建安装包

```bash
cd agent-desktop
npm run tauri build
```

产物路径：`agent-desktop/src-tauri/target/release/bundle/nsis/Agent Desktop_0.2.0_x64-setup.exe`

### 第五步：推送到 GitHub

```bash
git push origin main
git push origin v0.2.0
```

### 第六步：创建 GitHub Release

1. 打开 [GitHub Releases](https://github.com/你的仓库/Agent/releases)
2. 点击 "Draft a new release"
3. Tag 选择 `v0.2.0`
4. Release title: `Agent Desktop v0.2.0`
5. 描述中引用 CHANGELOG 内容
6. 上传安装包 `.exe`
7. 点击 "Publish release"

---

## CI 自动发布

项目配置了 GitHub Actions 自动构建（`.github/workflows/release.yml`）。

### 触发方式

| 方式 | 操作 |
|------|------|
| **推送标签** | `git tag v0.2.0 && git push origin v0.2.0` |
| **手动触发** | GitHub Actions → Release → Run workflow |

### CI 做了什么

```
git tag v0.2.0
    ↓
GitHub Actions 检测到新标签
    ↓
Setup Node.js + Rust
    ↓
npm ci → npm run tauri build（前端 + Rust + NSIS 打包）
    ↓
创建 Draft Release（带安装包附件）
    ↓
你去 Release 页面审核 → 点 "Publish"
```

> CI 创建的 Release 是 **Draft** 状态，需要手动审核后才公开可见。

### 构建时间

- 约 8-12 分钟（首次缓存前会更慢）
- Rust 编译有缓存后约 4-6 分钟

---

## 发布检查清单

每次发布前确认：

- [ ] 版本号已更新（`tauri.conf.json`）
- [ ] CHANGELOG.md 已更新
- [ ] 所有目标功能已在当前分支合入
- [ ] 本地测试已通过（`npm run dev`）
- [ ] 安装包已在干净环境测试（虚拟机或另一台电脑）
- [ ] git tag 已创建并推送
- [ ] GitHub Release 已创建（CI 自动或手动）
- [ ] 通知相关用户

---

## 安装包分发

### 文件命名规范

```
Agent Desktop_{版本号}_x64-setup.exe
```

例：`Agent Desktop_0.2.0_x64-setup.exe`

### 分发渠道

| 渠道 | 方式 | 适合场景 |
|------|------|---------|
| **GitHub Releases** | 直接上传 | 公开下载、自动更新 |
| **直接发送** | 微信/网盘/邮件 | 内测、快速分发 |
| **下载页** | 自建页面 | 正式官网 |

### 用户安装要求

| 要求 | 说明 |
|------|------|
| Windows 10 1803+ 或 Windows 11 | 不支持 Windows 7/8 |
| WebView2 运行时 | Win11 自带；Win10 安装器会自动安装 |
| 磁盘空间 | ~100 MB |
| 管理员权限 | 不需要（安装为当前用户） |

---

## 故障排查

### 构建失败：link.exe not found

原因：未安装 Visual Studio Build Tools 或未选 C++ 工作负载。

解决：
1. 下载 [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
2. 安装时勾选 **"使用 C++ 的桌面开发"**
3. 重启终端后重试

### npm install 太慢

```bash
npm config set registry https://registry.npmmirror.com
```

### cargo 下载依赖慢

设置 Rust 镜像（清华源）：

```powershell
$env:RUSTUP_DIST_SERVER = "https://mirrors.tuna.tsinghua.edu.cn/rustup"
$env:RUSTUP_UPDATE_ROOT = "https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup"
```

### NSIS 打包卡住

确认 `tauri.conf.json` 中 `bundle.targets` 是 `["nsis"]`（不是 `"all"`）。`"all"` 会尝试打 MSI 包，需要安装 WiX Toolset。

### GitHub Actions 构建失败

常见原因：
- 忘记推送 `Cargo.lock`（确保它在版本控制中）
- `package-lock.json` 未提交
- Actions 权限未开启：Settings → Actions → General → Workflow permissions → Read and write

### 安装后启动显示白屏/错误

确认 `tauri.conf.json` 中：
- `build.frontendDist` 正确指向 `../dist`
- `build.beforeBuildCommand` 是 `npm run build`
- 没有残留的自定义协议手动导航代码
