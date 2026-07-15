// 品牌同步脚本：读取 agent-desktop/ 下的 branding.json，把品牌名/标识符写入所有消费方文件。
// 用法（在 agent-desktop 目录下）：npm run sync-branding
// 注意：只替换「显示名 Agent Desktop」与「标识符 com.agent.desktop」，不会动目录名 agent-desktop / Rust crate 名。
import { readFileSync, writeFileSync, existsSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, ".."); // 脚本在 agent-desktop/scripts/，ROOT = agent-desktop/
const REPO = resolve(ROOT, "..");      // 仓库根目录（.github/ 等）

const b = JSON.parse(readFileSync(resolve(ROOT, "branding.json"), "utf8"));
const { productName, identifier, packageName, userAgent } = b;

// ── 1) tauri.conf.json：productName / identifier / 窗口标题 / 开始菜单文件夹 ──
const confPath = resolve(ROOT, "src-tauri/tauri.conf.json");
const conf = JSON.parse(readFileSync(confPath, "utf8"));
conf.productName = productName;
conf.identifier = identifier;
if (conf.app?.windows) {
  for (const w of conf.app.windows) {
    if (w.title) w.title = productName;
    if (w.startMenuFolder) w.startMenuFolder = productName;
  }
}
writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n", "utf8");

// ── 2) package.json：name ──
const pkgPath = resolve(ROOT, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.name = packageName;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n", "utf8");

// ── 3) 文本文件：替换显示名 / 标识符（固定清单，避免误伤 .codebuddy / node_modules / target）──
const textFiles = [
  // agent-desktop/ 根目录
  "README.md",
  "RELEASE.md",
  "TODO.md",
  "TECH_DEBT.md",
  "index.html",
  "build.bat",
  "start-dev.bat",
  "build-frontend.bat",
  "setup-code-server.bat",
  "scripts/release.bat",
  // agent-desktop/ 子目录
  "src-tauri/src/lib.rs",
  "src/styles/global.css",
  "src/i18n/locales/zh-CN.json",
  "src/i18n/locales/en.json",
];
// 仓库根目录的文件（使用 REPO 路径）
const repoFiles = [
  ".github/workflows/release.yml",
  "README.md",
  "CHANGELOG.md",
  "RELEASE.md",
];
let changed = 0;

// 处理 agent-desktop/ 内文件
for (const rel of textFiles) {
  const p = resolve(ROOT, rel);
  if (!existsSync(p)) {
    console.log("  skip (missing):", rel);
    continue;
  }
  let s = readFileSync(p, "utf8");
  const before = s;
  s = s.split("Agent Desktop").join(productName);
  s = s.split("com.agent.desktop").join(identifier);
  if (rel.endsWith("lib.rs")) {
    s = s.split("agent-desktop/").join(`${userAgent}/`);
  }
  if (s !== before) {
    writeFileSync(p, s, "utf8");
    changed++;
    console.log("  updated:", rel);
  }
}

// 处理仓库根目录文件（使用 REPO 路径）
for (const rel of repoFiles) {
  const p = resolve(REPO, rel);
  if (!existsSync(p)) {
    console.log("  skip (missing):", rel);
    continue;
  }
  let s = readFileSync(p, "utf8");
  const before = s;
  s = s.split("Agent Desktop").join(productName);
  s = s.split("com.agent.desktop").join(identifier);
  if (s !== before) {
    writeFileSync(p, s, "utf8");
    changed++;
    console.log("  updated:", rel);
  }
}

console.log(`\nbranding synced → productName="${productName}" identifier="${identifier}" (${changed} text files changed)`);
