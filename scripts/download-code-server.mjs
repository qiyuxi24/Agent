// code-server 下载/设置脚本 (Node.js 版本，不依赖 PowerShell)
// 用法：node scripts/download-code-server.mjs [版本号]
import { createWriteStream, existsSync, mkdirSync, rmSync, statSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';
import https from 'https';

const VERSION = process.argv[2] || '4.127.0';
const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = join(__dirname, '..');
const TARGET_DIR = join(PROJECT_ROOT, 'agent-desktop', 'src-tauri', 'binaries');
const RELEASE_DIR = join(TARGET_DIR, 'code-server', 'release');
const ENTRY_JS = join(RELEASE_DIR, 'out', 'node', 'entry.js');
const TARBALL = join(TARGET_DIR, 'package.tar.gz');
const URL = `https://github.com/coder/code-server/releases/download/v${VERSION}/package.tar.gz`;

const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const RED = '\x1b[31m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';
const GRAY = '\x1b[90m';

function log(msg, color = '') { console.log(`${color}${msg}${RESET}`); }

console.log('');
log('=== code-server 下载/设置脚本 (Node.js) ===', CYAN);
log(`版本: v${VERSION}`);
log(`目标: ${RELEASE_DIR}`);

// 检查是否已存在
if (existsSync(ENTRY_JS)) {
    log(`code-server v${VERSION} 已存在，跳过下载。`, GREEN);
    log('如需重新下载，请先删除目标目录后重试。');
    log(`  路径: ${RELEASE_DIR}`);
    process.exit(0);
}

// 创建目录
mkdirSync(RELEASE_DIR, { recursive: true });

// === 下载 ===
log(`下载 package.tar.gz (v${VERSION})... 约 54MB`, YELLOW);

function downloadFile(url, dest) {
    return new Promise((resolve, reject) => {
        const file = createWriteStream(dest);
        const request = https.get(url, { timeout: 30000 }, (response) => {
            // 处理重定向
            if (response.statusCode >= 301 && response.statusCode <= 308) {
                file.close();
                downloadFile(response.headers.location, dest).then(resolve).catch(reject);
                return;
            }
            if (response.statusCode !== 200) {
                file.close();
                reject(new Error(`HTTP ${response.statusCode}`));
                return;
            }

            const total = parseInt(response.headers['content-length'] || '0', 10);
            let downloaded = 0;
            let lastPct = -1;
            const startTime = Date.now();

            response.on('data', (chunk) => {
                downloaded += chunk.length;
                if (total > 0) {
                    const pct = Math.round((downloaded / total) * 100);
                    if (pct !== lastPct) {
                        lastPct = pct;
                        const elapsed = (Date.now() - startTime) / 1000;
                        const speed = downloaded / elapsed / 1024 / 1024;
                        process.stdout.write(`\r  下载中... ${pct}% (${(downloaded / 1024 / 1024).toFixed(1)}MB / ${speed.toFixed(1)}MB/s)    `);
                    }
                }
            });

            response.pipe(file);
            file.on('finish', () => {
                file.close();
                process.stdout.write('\r  下载完成!                              \n');
                resolve();
            });
            file.on('error', (err) => {
                file.close();
                reject(err);
            });
        });

        request.on('error', reject);
        request.on('timeout', () => {
            request.destroy();
            reject(new Error('连接超时'));
        });
    });
}

let downloaded = false;

// 方式1: Node.js 原生 https 下载
try {
    log('  使用 Node.js 原生下载...', GRAY);
    await downloadFile(URL, TARBALL);
    if (existsSync(TARBALL) && statSync(TARBALL).size > 1000000) {
        downloaded = true;
    }
} catch (err) {
    log(`  Node.js 下载失败: ${err.message}`, GRAY);
}

// 方式2: curl 备选
if (!downloaded) {
    try {
        log('  尝试 curl...', GRAY);
        execSync(`curl.exe -k -L --connect-timeout 30 --max-time 600 --retry 2 -o "${TARBALL}" "${URL}"`, {
            stdio: 'inherit',
            timeout: 600_000
        });
        if (existsSync(TARBALL) && statSync(TARBALL).size > 1000000) {
            downloaded = true;
        }
    } catch {
        log('  curl 失败', GRAY);
    }
}

if (!downloaded) {
    log('所有下载方式均失败！请手动下载：', RED);
    log(`  ${URL}`, YELLOW);
    log(`  保存为: ${TARBALL}`, YELLOW);
    log('  下载后重新运行本脚本即可继续。');
    process.exit(1);
}

const sizeMB = (statSync(TARBALL).size / 1024 / 1024).toFixed(1);
log(`  下载完成: ${sizeMB}MB`, GREEN);

// === 解压 ===
log('解压 package.tar.gz...', YELLOW);
let extracted = false;

try {
    execSync(`tar -xzf "${TARBALL}" --strip-components=1`, {
        cwd: RELEASE_DIR,
        stdio: 'inherit',
        timeout: 300_000
    });
    extracted = true;
    log('  解压完成', GREEN);
} catch {
    log('  tar 解压失败，尝试用 7z...', YELLOW);
    try {
        execSync(`7z x "${TARBALL}" -y`, {
            cwd: RELEASE_DIR,
            stdio: 'inherit',
            timeout: 300_000
        });
        extracted = true;
        log('  7z 解压完成', GREEN);
    } catch {
        log('解压失败', RED);
        log(`请手动解压 ${TARBALL} 到 ${RELEASE_DIR}`, YELLOW);
        log('然后重新运行本脚本以完成 npm install。');
        process.exit(1);
    }
}

// 清理 tarball
try { rmSync(TARBALL); } catch {}

// 如果解压失败，退出
if (!extracted) {
    log('无法继续，解压步骤未完成。', RED);
    process.exit(1);
}

// === 安装依赖 ===
log('安装 npm 依赖 (--production --ignore-scripts)...', YELLOW);
try {
    execSync('npm install --production --ignore-scripts', {
        cwd: RELEASE_DIR,
        stdio: 'inherit',
        timeout: 600_000
    });
    log('  npm install 完成', GREEN);
} catch {
    log('  npm install 有警告（部分依赖可能未安装），可手动处理', YELLOW);
}

// === 检查和编译原生模块 ===
// code-server 依赖 7 个 @vscode/* 原生 .node 模块。
// --ignore-scripts 跳过了编译，使用 code-server 预编译的二进制。
// 但如果预编译模块与当前系统不兼容（Node.js 版本、VS 组件等），
// 需要重新编译。此步骤自动检测并修复。
const NATIVE_MODULES = [
    { pkg: 'windows-registry', file: 'winregistry.node' },
    { pkg: 'windows-process-tree', file: 'windows_process_tree.node' },
    { pkg: 'deviceid', file: 'windows.node' },
    { pkg: 'native-watchdog', file: 'watchdog.node' },
    { pkg: 'spdlog', file: 'spdlog.node' },
    { pkg: 'sqlite3', file: 'vscode-sqlite3.node' },
    { pkg: 'windows-ca-certs', file: 'crypt32.node' },
];

const VSCODE_DIR = join(RELEASE_DIR, 'lib', 'vscode', 'node_modules', '@vscode');
let missingModules = [];

for (const m of NATIVE_MODULES) {
    const nodePath = join(VSCODE_DIR, m.pkg, 'build', 'Release', m.file);
    if (!existsSync(nodePath)) {
        missingModules.push(`${m.pkg}/${m.file}`);
    }
}

if (missingModules.length > 0) {
    log(`检测到 ${missingModules.length} 个原生模块缺失:`, YELLOW);
    missingModules.forEach(m => log(`  - ${m}`, GRAY));
    log('正在重新安装 code-server 依赖（含原生编译，约 1-3 分钟）...', YELLOW);
    log('如失败，请确保已安装 Visual Studio Build Tools（C++ 工作负载）', GRAY);
    try {
        // 对 vscode 子目录重新执行 npm install（这次不跳过 scripts）
        const VSCODE_NODE_DIR = join(RELEASE_DIR, 'lib', 'vscode');
        execSync('npm install --production', {
            cwd: VSCODE_NODE_DIR,
            stdio: 'inherit',
            timeout: 600_000
        });
        log('  原生模块编译完成', GREEN);
    } catch (e) {
        log('  原生模块编译失败！', RED);
        log(`  错误: ${e.message}`, RED);
        log('  请手动运行（可能需要管理员权限）：', YELLOW);
        log(`    cd "${VSCODE_NODE_DIR}"`, YELLOW);
        log('    npm install --production', YELLOW);
        log('', GRAY);
        log('  常见原因及修复：', CYAN);
        log('  1. 未安装 VS BuildTools → 安装时勾选"使用 C++ 的桌面开发"', GRAY);
        log('  2. 缺少 Spectre 缓解库 → VS Installer → 修改 → 单个组件 → 搜索 Spectre', GRAY);
        log('  3. Node.js 版本不匹配 → code-server 要求 Node.js 22，当前: ' + process.version, GRAY);
    }
} else {
    log('  所有 7 个原生模块验证通过', GREEN);
}

// === 验证 ===
if (existsSync(ENTRY_JS)) {
    console.log('');
    log('=== 设置完成! ===', GREEN);
    log(`Code Server v${VERSION} 已就绪！`);
    log(`入口文件: ${ENTRY_JS}`);

    // 版本验证
    try {
        const version = execSync(`node "${ENTRY_JS}" --version`, {
            encoding: 'utf-8',
            timeout: 30_000
        }).trim();
        log(`版本验证: ${version}`, CYAN);
    } catch {
        log('版本验证跳过（可能存在兼容性问题）', YELLOW);
    }
} else {
    console.log('');
    log('设置失败：入口文件未找到！', RED);
    log(`预期路径: ${ENTRY_JS}`, RED);
    process.exit(1);
}
