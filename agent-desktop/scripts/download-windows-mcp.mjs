// Windows MCP Server 下载脚本 (sbroenne/mcp-windows)
// 用法：
//   node scripts/download-windows-mcp.mjs              # 自动下载配置的版本
//   node scripts/download-windows-mcp.mjs --force       # 强制重新下载

import { createWriteStream, existsSync, mkdirSync, rmSync, readFileSync, readdirSync, renameSync, statSync } from 'fs';
import { join, dirname, basename } from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';
import https from 'https';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = join(__dirname, '..');

// --- 从 build.config.json 读取配置 ---
const CONFIG = JSON.parse(readFileSync(join(PROJECT_ROOT, 'build.config.json'), 'utf-8'));

const VERSION = CONFIG.windowsMcp.version;
const IS_FORCE = process.argv.includes('--force');
const BINARIES_DIR = join(PROJECT_ROOT, 'src-tauri', 'binaries');
const TARGET_DIR = join(BINARIES_DIR, 'windows-mcp');
const EXE_NAME = CONFIG.windowsMcp.exeName;
const EXE_PATH = join(TARGET_DIR, EXE_NAME);
const ZIP_FILE = join(BINARIES_DIR, 'windows-mcp-server.zip');
const URL = CONFIG.windowsMcp.downloadUrlTemplate.replace('{version}', VERSION);

const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const RED = '\x1b[31m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';

function log(msg, color = '') { console.log(`${color}${msg}${RESET}`); }

console.log('');
log('=== Windows MCP Server 下载脚本 ===', CYAN);
log(`配置版本: v${VERSION}`);
log(`目标路径: ${EXE_PATH}`);

// 检查是否已存在
if (existsSync(EXE_PATH) && !IS_FORCE) {
  log(`Windows MCP Server v${VERSION} 已存在，跳过下载。`, GREEN);
  log('如需重新下载: node scripts/download-windows-mcp.mjs --force');
  process.exit(0);
}

if (IS_FORCE && existsSync(EXE_PATH)) {
  log('--force 模式：删除旧版本重新下载...', YELLOW);
  rmSync(TARGET_DIR, { recursive: true });
}

// 创建目录
mkdirSync(TARGET_DIR, { recursive: true });

// === 下载 ===
log(`下载 windows-mcp-server-v${VERSION}-win-x64.zip... 约 51MB`, YELLOW);

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);
    const request = https.get(url, { timeout: 30000 }, (response) => {
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
        const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
        const size = (downloaded / 1024 / 1024).toFixed(1);
        process.stdout.write('\r\x1b[K');
        log(`  完成: ${size}MB in ${elapsed}s`, GREEN);
        resolve();
      });
    });

    request.on('error', (err) => {
      file.close();
      reject(err);
    });
  });
}

try {
  await downloadFile(URL, ZIP_FILE);
} catch (err) {
  log(`下载失败: ${err.message}`, RED);
  try { rmSync(ZIP_FILE); } catch (_) {}
  process.exit(1);
}

// === 解压 ===
log('解压中...', YELLOW);

try {
  if (process.platform === 'win32') {
    execSync(
      `powershell -Command "Expand-Archive -Path '${ZIP_FILE}' -DestinationPath '${TARGET_DIR}' -Force"`,
      { stdio: 'pipe' }
    );
  } else {
    execSync(`unzip -o "${ZIP_FILE}" -d "${TARGET_DIR}"`, { stdio: 'pipe' });
  }

  // 递归查找 exe 文件（可能在子目录中）
  function findExe(dir, depth = 2) {
    if (depth <= 0) return null;
    try {
      const entries = readdirSync(dir);
      for (const entry of entries) {
        const full = join(dir, entry);
        try {
          const s = statSync(full);
          if (s.isFile() && entry.endsWith('.exe')) {
            return full;
          }
          if (s.isDirectory()) {
            const found = findExe(full, depth - 1);
            if (found) return found;
          }
        } catch (_) {}
      }
    } catch (_) {}
    return null;
  }

  const foundExe = findExe(TARGET_DIR);
  if (!foundExe) {
    log('错误: 解压后未找到 .exe 文件', RED);
    log(`请检查 ${TARGET_DIR} 目录内容`, RED);
    process.exit(1);
  }

  // 如果 exe 不在目标根目录，移到根目录
  if (basename(foundExe) !== EXE_NAME) {
    const destExe = join(TARGET_DIR, EXE_NAME);
    renameSync(foundExe, destExe);
    log(`重命名: ${basename(foundExe)} → ${EXE_NAME}`, GREEN);
  }

  // 清理 zip 文件
  rmSync(ZIP_FILE);
  
  log(`解压完成: ${EXE_NAME}`, GREEN);
} catch (err) {
  log(`解压失败: ${err.message}`, RED);
  try { rmSync(ZIP_FILE); } catch (_) {}
  process.exit(1);
}

// === 验证 ===
if (existsSync(EXE_PATH)) {
  log('');
  log('✅ Windows MCP Server 下载完成！', GREEN);
  log(`   路径: ${EXE_PATH}`);
  log(`   版本: v${VERSION}`);
  log('');
  log('提示: 启动 Votek 后，在 Agent 模式下即可使用 Windows 自动化工具。', CYAN);
  log('工具包括: ui_find, ui_click, ui_type, ui_read, screenshot, mouse, keyboard, window_management 等');
  log('');
} else {
  log('');
  log('❌ 安装验证失败：EXE 文件未找到', RED);
  log(`   期望路径: ${EXE_PATH}`);
  log(`   解压目录内容:`);
  try {
    const files = readdirSync(TARGET_DIR);
    files.forEach(f => log(`     - ${f}`, RED));
  } catch {}
  process.exit(1);
}
