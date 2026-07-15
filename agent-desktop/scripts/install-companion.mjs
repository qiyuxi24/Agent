/**
 * Votek Companion — 安装到 code-server
 *
 * 编译 TypeScript 扩展并复制到 code-server 的内置扩展目录，
 * 使其在 code-server 启动时自动激活。
 *
 * 用法：node scripts/install-companion.mjs
 */

import { execSync } from 'child_process';
import { existsSync, mkdirSync, readFileSync, cpSync, rmSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = join(__dirname, '..');

const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const RED = '\x1b[31m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';

function log(msg, color = '') { console.log(`${color}${msg}${RESET}`); }

// ── 路径 ──
const COMPANION_DIR = join(PROJECT_ROOT, 'votek-companion');
const COMPANION_OUT = join(COMPANION_DIR, 'out');
const COMPANION_PKG = join(COMPANION_DIR, 'package.json');

// code-server release 目录
const CS_RELEASE = join(PROJECT_ROOT, 'agent-desktop', 'src-tauri', 'binaries', 'code-server', 'release');
// code-server 内置扩展目录
const CS_EXTENSIONS = join(CS_RELEASE, 'lib', 'vscode', 'extensions');
const TARGET_DIR = join(CS_EXTENSIONS, 'votek-companion');

console.log('');
log('=== Votek Companion 扩展安装 ===', CYAN);

// ── 检查 code-server 是否已下载 ──
if (!existsSync(join(CS_RELEASE, 'out', 'node', 'entry.js'))) {
    log('code-server 未安装，请先运行 node scripts/download-code-server.mjs', RED);
    process.exit(1);
}

// ── 检查 companion 源码 ──
if (!existsSync(COMPANION_PKG)) {
    log('votek-companion 源码不存在, 跳过.', YELLOW);
    process.exit(0);
}

// ── 安装依赖 ──
log('安装 companion 依赖...', YELLOW);
try {
    execSync('npm install --no-audit --no-fund', {
        cwd: COMPANION_DIR,
        stdio: 'inherit',
        timeout: 120_000
    });
    log('  依赖安装完成', GREEN);
} catch (e) {
    log(`  依赖安装失败: ${e.message}`, RED);
    process.exit(1);
}

// ── 编译 TypeScript ──
log('编译 TypeScript...', YELLOW);
try {
    execSync('npx tsc -p tsconfig.json', {
        cwd: COMPANION_DIR,
        stdio: 'inherit',
        timeout: 60_000
    });
    log('  编译完成', GREEN);
} catch (e) {
    log(`  编译失败: ${e.message}`, RED);
    process.exit(1);
}

// ── 验证编译产物 ──
if (!existsSync(join(COMPANION_OUT, 'extension.js'))) {
    log('编译产物缺失: out/extension.js', RED);
    process.exit(1);
}

// ── 复制到 code-server 扩展目录 ──
log(`安装到: ${TARGET_DIR}`, CYAN);

// 清理旧安装
if (existsSync(TARGET_DIR)) {
    rmSync(TARGET_DIR, { recursive: true });
}
mkdirSync(TARGET_DIR, { recursive: true });

// 复制 package.json
cpSync(COMPANION_PKG, join(TARGET_DIR, 'package.json'));

// 复制编译产物
cpSync(COMPANION_OUT, join(TARGET_DIR, 'out'), { recursive: true });

// 复制 ws 依赖（WebSocket 库）
const WS_SRC = join(COMPANION_DIR, 'node_modules', 'ws');
if (existsSync(WS_SRC)) {
    mkdirSync(join(TARGET_DIR, 'node_modules', 'ws'), { recursive: true });
    cpSync(WS_SRC, join(TARGET_DIR, 'node_modules', 'ws'), { recursive: true });
    log('  ws 模块已复制', GREEN);
}

// 验证安装
if (!existsSync(join(TARGET_DIR, 'out', 'extension.js'))) {
    log('安装验证失败: extension.js 未找到', RED);
    process.exit(1);
}

// ── 清理编译产物中的 node_modules（不需要） ──
const bundledNodeModules = join(TARGET_DIR, 'out', 'node_modules');
if (existsSync(bundledNodeModules)) {
    rmSync(bundledNodeModules, { recursive: true });
}

console.log('');
log('=== Votek Companion 安装完成! ===', GREEN);
log(`安装位置: ${TARGET_DIR}`);
log('扩展将在 code-server 启动时自动激活，');
log(`WebSocket 服务监听 127.0.0.1:${process.env.VOTEK_BRIDGE_PORT || '19527'}`);
