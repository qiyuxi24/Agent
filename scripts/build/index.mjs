#!/usr/bin/env node
// build/index.mjs — Votek 构建统一 CLI
//
// 用法：
//   node scripts/build/index.mjs dev               开发模式（check + tauri dev）
//   node scripts/build/index.mjs build             生产构建（check + prepare + tauri build）
//   node scripts/build/index.mjs check             仅环境校验
//   node scripts/build/index.mjs prepare           仅下载/准备 code-server
//   node scripts/build/index.mjs --help            帮助

import { existsSync, readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { execSync, spawn } from 'child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..', '..');
const DESKTOP = join(ROOT, 'agent-desktop');
const CONFIG = JSON.parse(readFileSync(join(ROOT, 'build.config.json'), 'utf-8'));

const CYAN = '\x1b[36m'; const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m'; const RED = '\x1b[31m'; const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';

// ============================================================
// CLI 入口
// ============================================================
const cmd = process.argv[2] || '--help';

switch (cmd) {
  case 'check':   await check();   break;
  case 'prepare': await prepare(); break;
  case 'dev':     await dev();     break;
  case 'build':   await build();   break;
  default:        help();          break;
}

// ============================================================
// 子命令
// ============================================================

function help() {
  console.log(`\n${BOLD}Votek Build CLI${RESET}\n`);
  console.log(`  ${CYAN}node scripts/build/index.mjs check${RESET}     Environment check only`);
  console.log(`  ${CYAN}node scripts/build/index.mjs prepare${RESET}   Download code-server (~${CONFIG.codeServer.compressedSize})`);
  console.log(`  ${CYAN}node scripts/build/index.mjs dev${RESET}       Dev mode (check + tauri dev)`);
  console.log(`  ${CYAN}node scripts/build/index.mjs build${RESET}     Production build (check + prepare + tauri build)`);
  console.log(`\nOr use the .bat shortcuts: ${CYAN}build.bat${RESET} / ${CYAN}start-dev.bat${RESET}\n`);
}

async function check() {
  const ok = runNode(join(__dirname, 'check-env.mjs'));
  if (!ok) {
    console.log(`${RED}Environment check failed.${RESET}\n`);
    process.exit(1);
  }
}

async function prepare() {
  console.log(`\n${CYAN}--- Preparing code-server v${CONFIG.codeServer.version} ---${RESET}\n`);
  if (!existsSync(join(DESKTOP, 'src-tauri', 'binaries', 'code-server', 'release', 'out', 'node', 'entry.js'))) {
    runNode(join(ROOT, 'scripts', 'download-code-server.mjs'));
  } else {
    console.log(`${GREEN}code-server already installed.${RESET}\n`);
  }
}

async function dev() {
  // 1. 环境检查
  await check();

  // 2. code-server 检查（dev 模式仅提示，不强制）
  const entry = join(DESKTOP, 'src-tauri', 'binaries', 'code-server', 'release', 'out', 'node', 'entry.js');
  if (!existsSync(entry)) {
    console.log(`${YELLOW}code-server not found — IDE feature will be unavailable.${RESET}`);
    console.log(`${YELLOW}Run "npm run download:code-server" later to add it.${RESET}\n`);
  }

  // 3. npm install（如缺失）
  ensureNpm(/*auto*/true);

  // 4. 清理残留端口（上次异常退出可能未释放）
  killPort(1420); // Vite dev server

  // 5. 启动 tauri dev
  console.log(`${CYAN}--- Starting tauri dev ---${RESET}\n`);
  await runTauri('dev');
}

async function build() {
  // 1. 环境检查
  await check();

  // 2. code-server 准备（生产构建强制要求）
  await prepare();

  // 3. npm install（如缺失）
  ensureNpm(/*auto*/true);

  // 4. tauri build
  console.log(`${CYAN}--- Running tauri build ---${RESET}\n`);
  console.log(`${GREEN}Output: src-tauri/target/release/bundle/${RESET}\n`);

  const code = await runTauri('build');
  if (code === 0) {
    console.log(`\n${GREEN}${BOLD}Build complete!${RESET}\n`);
  } else {
    console.log(`\n${RED}Build failed (exit ${code}).${RESET}\n`);
    process.exit(code);
  }
}

// ============================================================
// 工具函数
// ============================================================

function runNode(script, args = []) {
  try {
    execSync(`node "${script}" ${args.join(' ')}`, {
      cwd: ROOT,
      stdio: 'inherit',
      timeout: 120_000
    });
    return true;
  } catch {
    return false;
  }
}

async function runTauri(action) {
  return new Promise((resolve) => {
    const child = spawn('npx', ['tauri', action], {
      cwd: DESKTOP,
      stdio: 'inherit',
      shell: true,
    });
    child.on('close', resolve);
  });
}

function ensureNpm(auto) {
  if (!existsSync(join(DESKTOP, 'node_modules'))) {
    if (auto) {
      console.log(`${YELLOW}Installing npm dependencies...${RESET}`);
      execSync('npm install', { cwd: DESKTOP, stdio: 'inherit', timeout: 300_000 });
    } else {
      console.log(`${YELLOW}node_modules not found. Run: npm install${RESET}`);
    }
  }
}

function killPort(port) {
  try {
    // netstat + taskkill 不依赖额外 npm 包，比 npx kill-port 更快
    const out = execSync(`netstat -ano | findstr ":${port}"`, {
      encoding: 'utf-8', timeout: 5000, windowsHide: true
    });
    const lines = out.trim().split('\n').filter(l => l.includes('LISTENING'));
    for (const line of lines) {
      const pid = line.trim().split(/\s+/).pop();
      if (pid && pid !== '0') {
        console.log(`${YELLOW}Port ${port} occupied by PID ${pid}, killing...${RESET}`);
        execSync(`taskkill /PID ${pid} /F`, { timeout: 5000, windowsHide: true });
      }
    }
  } catch {
    // 端口空闲或清理失败，静默继续
  }
}
