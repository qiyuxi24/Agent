# Votek 项目路径管理规范

> 最后更新：2026-07-15 · 基于目录重组（16→7条目）的经验教训

---

## 核心理念

本项目遵循以下四条来自业界的路径管理原则，确保文件组织**直观、可预测、低维护成本**。

---

## 一、四大核心原则

### 原则 1：最小惊讶原则（Principle of Least Astonishment, POLA）

> 文件应该放在人们**第一反应会去找的位置**，而不是"在某个角落里藏着"。

**来源**：UX 设计与软件工程的经典原则（1972 年，Martin/Clear/West 等提出）。
[参考：维基百科](https://en.wikipedia.org/wiki/Principle_of_least_astonishment)

**实践对比**：

| ✅ 正确做法 | ❌ 反模式 |
|-------------|----------|
| `build.bat` 在 `agent-desktop/` 下 — 它就是构建 desktop 的 | `build.bat` 散落在仓库根目录，让人困惑它是干什么的 |
| `branding.json` 在 `agent-desktop/` — 消费方全在 desktop 内部 | 放在根目录让人误以为是仓库级配置 |
| `votek-companion/` 作为 IDE 扩展，放在仓库根目录 | 藏在 `agent-desktop/` 里面，职责不清 |

**自查方法**：如果你是新人，打开某个目录，你最期望在那里看到什么？——那就把什么放在那里。

---

### 原则 2：自包含原则（Self-Contained Project）

> 一个项目的**构建、运行、配置、脚本**应该全部在自己的目录内，不依赖上级目录的特殊安排。

**来源**：Monorepo 最佳实践、[Tauri 官方项目结构](https://v2.tauri.app/start/project-structure/)

**本项目标准结构**：

```
agent-desktop/                  ← 自包含的 Tauri + React 项目
├── src-tauri/                  ← Rust 核心（Tauri 官方规范）
├── src/                        ← React 前端
├── scripts/                    ← 所有构建/工具脚本（不再散落根目录）
│   ├── build/
│   │   ├── index.mjs           ← 统一 CLI（check/prepare/dev/build）
│   │   └── check-env.mjs       ← 环境校验
│   ├── download-code-server.mjs
│   ├── sync-branding.mjs
│   ├── install-companion.mjs
│   ├── gen-bear-icon.mjs
│   ├── gen-icon.bat
│   └── release.bat
├── build.bat                   ← 入口
├── start-dev.bat               ← 开发入口
├── setup-code-server.bat       ← Code Server 准备
├── build-frontend.bat          ← 纯前端构建
├── build.config.json           ← 构建配置真相源
├── branding.json               ← 品牌名称真相源
├── package.json                ← npm
├── vite.config.ts              ← Vite
└── LICENSE
```

**关键推论**：
- 项目内的脚本用 `node scripts/xxx.mjs`，不要用 `node ../scripts/xxx.mjs`
- `.bat` 入口用 `cd /d "%~dp0"` 确保工作目录 = 项目根
- 可执行入口（`.bat`）放在项目根，被调用的脚本放 `scripts/`

---

### 原则 3：关注点分离（Separation of Concerns）

> 不同职责的项目/资源，放在不同的顶级目录，互不嵌套。

**来源**：Monorepo 架构（[Nx](https://nx.dev)、[Turborepo](https://turbo.build/repo) 的 `apps/` vs `packages/` 分离模式）、微服务设计

**本仓库结构**：

```
Agent/                          ← 仓库根 = 多项目容器（非任何单一项目的根！）
├── agent-desktop/              ← Votek 桌面端（Tauri + React）
├── votek-companion/            ← IDE 桥接扩展（VS Code Extension）
├── reference/                  ← 学习/参考资料（不参与构建，不提交业务代码）
│   ├── agent-loop/             ← Agent Loop 参考实现
│   ├── learning-roadmap.md     ← 学习路线图
│   └── prompt.md               ← 提示词参考
├── README.md                   ← 仓库总览
├── CHANGELOG.md
└── RELEASE.md
```

**黄金规则**：
- ❌ **不要**把一个独立项目塞到另一个项目内部
- ❌ **不要**把学习资料和业务代码混在一起
- ✅ 仓库根目录只放仓库级文档 + 子项目目录，**不超过 8 个条目**

---

### 原则 4：路径引用就近原则

> 路径引用应该从**消费端的视角**出发，用最短、最直接的相对路径。

**来源**：[Rust include_str! 文档](https://doc.rust-lang.org/std/macro.include_str.html)（路径相对于当前文件）、Node.js ESM 模块解析规则

#### 各语言/工具的路径基准

| 语言/工具 | 路径基准 | 示例 |
|-----------|----------|------|
| **Rust `include_str!`** | 当前 `.rs` 文件所在目录 | `build.rs` → `include_str!("../build.config.json")` |
| **Node.js `import`** | 当前 `.mjs` 文件所在目录 | `index.mjs` → `join(__dirname, '..', '..')` |
| **npm scripts** | `package.json` 所在目录 | `"download:code-server": "node scripts/download-code-server.mjs"` |
| **`.bat` 脚本** | `%~dp0` = 脚本自身目录 | `cd /d "%~dp0.."` → 向上一级 |
| **Tauri 配置** | `src-tauri/` 目录 | `icons/icon.ico`（相对于 `src-tauri/`） |
| **Vite 构建** | `vite.config.ts` 所在目录 | `src/xxx` 相对于项目根 |

---

## 二、各语言/平台的具体规则

### 2.1 Rust：`include_str!` 路径

Rust 编译期宏 `include_str!` / `include!` 的路径**相对于包含该宏的源文件**（类似模块查找），与运行时 `std::fs` 完全不同。

```rust
// 文件：agent-desktop/src-tauri/build.rs

// ✅ 正确：../ 从 src-tauri/ 向上到 agent-desktop/
include_str!("../build.config.json")

// ❌ 错误：../../ 回到了仓库根（Agent/），但文件已移到 agent-desktop/
include_str!("../../build.config.json")
```

**记忆法**：把 `include_str!` 路径想象成从当前 `.rs` 文件**爬目录树**。

**常见陷阱**：移动文件时容易忘记同步更新 `include_str!` 路径，导致**编译期报错**而非运行时。

### 2.2 Node.js：`__dirname` 与路径常量

ESM 模块中没有全局 `__dirname`，需要通过 `import.meta.url` 推导：

```js
// ✅ 标准写法（在本项目所有 .mjs 中统一使用）
import { dirname } from "path";
import { fileURLToPath } from "url";
const __dirname = dirname(fileURLToPath(import.meta.url));
```

**本项目约定**：脚本中统一用一个变量指向 `agent-desktop/`，避免到处写 `../../`：

```js
// scripts/build/index.mjs (__dirname = agent-desktop/scripts/build/)
const DESKTOP = join(__dirname, '..', '..');  // → agent-desktop/

// scripts/sync-branding.mjs (__dirname = agent-desktop/scripts/)
const ROOT = resolve(__dirname, "..");       // → agent-desktop/
const REPO = resolve(ROOT, "..");            // → 仓库根（仅用于 .github/ 等）
```

### 2.3 Windows `.bat`：三条铁律

| # | 规则 | 原因 |
|---|------|------|
| 1 | **CRLF 换行** | CMD 解析 LF-only 文件会逐字符错乱，报 `'ho' is not recognized`、`'gent-desktop'` 等 |
| 2 | **纯 ASCII 内容** | 中文全角括号 `（）` 会加剧 CMD 字节错位，注释用 `REM` + 英文 |
| 3 | **用 `%~dp0` 定位自身** | `%~dp0` = 当前 .bat 的驱动器+路径，末尾带 `\`；避免依赖调用者 cwd |

```batch
@echo off
cd /d "%~dp0"              REM 进入脚本自身所在目录
node scripts\build\index.mjs dev
```

**注意**：`write_to_file` 工具写出的是 LF 换行。写完 `.bat` 后必须转 CRLF：
```powershell
$f = "..."; $t = [IO.File]::ReadAllText($f);
$t = ($t -replace "`r`n","`n") -replace "`n","`r`n";
[IO.File]::WriteAllText($f, $t, (New-Object Text.UTF8Encoding($false)))
```

### 2.4 npm scripts：工作目录约定

npm 执行 scripts 时，**工作目录固定为 package.json 所在目录**，路径相对这个目录写即可：

```json
{
  "scripts": {
    // ✅ 正确：直接相对于 agent-desktop/
    "sync-branding": "node scripts/sync-branding.mjs",
    "download:code-server": "node scripts/download-code-server.mjs",

    // ❌ 错误：多了一层 ../（曾经 scripts/ 在根目录时的遗留）
    // "sync-branding": "node ../scripts/sync-branding.mjs"
  }
}
```

### 2.5 CI/CD（GitHub Actions）

CI 中 `working-directory` 和脚本路径需要一致：

```yaml
# ✅ 方案 A：指定 working-directory
- name: Download code-server
  working-directory: agent-desktop
  run: node scripts/download-code-server.mjs

# ✅ 方案 B：完整路径（不需要 working-directory）
- name: Download code-server
  run: node agent-desktop/scripts/download-code-server.mjs
```

---

## 三、本项目路径速查表

### 关键文件的绝对位置

| 文件 | 位置 | 被谁引用 |
|------|------|----------|
| `build.config.json` | `agent-desktop/build.config.json` | `build.rs` + 4 个 `.mjs` |
| `branding.json` | `agent-desktop/branding.json` | `sync-branding.mjs` |
| `package.json` | `agent-desktop/package.json` | npm / Vite / CI |
| `tauri.conf.json` | `agent-desktop/src-tauri/tauri.conf.json` | Tauri CLI |

### 关键目录的层级关系

```
项目内（agent-desktop/ 内部）
  src-tauri/build.rs  →  ../build.config.json          ← 向上 1 级
  scripts/build/       →  ../../                        ← 向上 2 级 = agent-desktop/
  build.bat            →  scripts/build/index.mjs       ← 同级的 scripts/

跨项目（从 agent-desktop/ 看仓库根）
  scripts/sync-branding.mjs → REPO = ../../           ← 仓库根
  install-companion.mjs     → ../../votek-companion/   ← 仓库根同级
```

---

## 四、移动文件的标准流程

当你需要移动文件/目录时，按以下步骤确保不漏改：

1. **搜索引用**：`git grep "旧路径"` 找出所有引用该路径的文件
2. **检查各语言**：
   - `.rs` → `include_str!` / `include!` / `mod` 声明
   - `.mjs` → `join(__dirname, ...)` / `resolve(ROOT, ...)`
   - `.json` → npm scripts、Tauri 配置中的路径
   - `.bat` → `%~dp0` 相对路径、`set "ROOT=%~dp0.."` 类变量
   - `.yml` → CI/CD 的 `working-directory` / `run` / `path`
3. **移动后验证**：
   - `node scripts/build/check-env.mjs`（环境检查）
   - `cargo check`（Rust 编译检查）
   - `npx tsc --noEmit`（TypeScript 类型检查）
4. **更新文档**：更新本文件 + `MEMORY.md` 中的路径引用

---

## 五、历史教训（避坑指南）

### 坑 1：`build.rs` 的 `include_str!` 向上爬太多

- **症状**：移动 `build.config.json` 后，`cargo check` 报 `couldn't read '../../build.config.json'`
- **原因**：`../../` 从 `src-tauri/` 到了仓库根，但文件已移到 `agent-desktop/`
- **教训**：`include_str!` 路径要手动数层级，不能凭感觉

### 坑 2：`.bat` 文件依赖调用者 cwd

- **症状**：在别处运行 `release.bat` 时找不到 `src-tauri\tauri.conf.json`
- **原因**：脚本依赖调用者的工作目录，而不是用 `%~dp0` 定位自身
- **教训**：每个 `.bat` 文件开头必须有 `cd /d "%~dp0.."` 或类似语句

### 坑 3：npm scripts 路径还带着 `../`

- **症状**：`scripts/` 移入项目后，`package.json` 里还写着 `node ../scripts/xxx.mjs`
- **原因**：移动目录时只移动了文件，忘了改引用
- **教训**：用上述"标准流程"第 1 步 `git grep` 检查

### 坑 4：同步忘改文档

- **症状**：README.md 还在描述旧的目录树，新同事被误导
- **原因**：改完代码就收工，忘了文档也是代码的一部分
- **教训**：README.md / MEMORY.md / 本文件 都要同步更新

---

## 参考资源

| 资源 | 链接 |
|------|------|
| Tauri 官方项目结构 | https://v2.tauri.app/start/project-structure/ |
| 最小惊讶原则 | https://en.wikipedia.org/wiki/Principle_of_least_astonishment |
| Rust `include_str!` 文档 | https://doc.rust-lang.org/std/macro.include_str.html |
| Go 项目布局标准 | https://github.com/golang-standards/project-layout |
| Nx Monorepo 指南 | https://nx.dev/concepts/decisions/project-location |
