// check-env.mjs — 统一环境校验模块
// 用法：
//   node scripts/build/check-env.mjs          基础校验（Node + Cargo）
//   node scripts/build/check-env.mjs --release 完整校验（+ Git + 分支）
//   node scripts/build/check-env.mjs --help
// 退出码：0=通过，1=不满足

import { existsSync, readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..', '..');
const CONFIG = JSON.parse(readFileSync(join(ROOT, 'build.config.json'), 'utf-8'));

const CYAN = '\x1b[36m'; const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m'; const RED = '\x1b[31m'; const RESET = '\x1b[0m';

let errors = 0;
function ok(msg)   { console.log(`  ${GREEN}ok${RESET}  ${msg}`); }
function warn(msg) { console.log(`  ${YELLOW}warn${RESET} ${msg}`); }
function fail(msg) { console.log(`  ${RED}ERR${RESET}  ${msg}`); errors++; }

// ============================================================
// ① Node.js
// ============================================================
console.log(`\n${CYAN}[1/4] Node.js${RESET}`);
try {
  const nodeV = execSync('node --version', { encoding: 'utf-8', timeout: 5000 }).trim();
  ok(`node ${nodeV}`);

  const major = parseInt(nodeV.replace(/^v/, '').split('.')[0], 10);
  const required = CONFIG.node.requiredVersion;
  if (major !== required) {
    warn(`Node ${required}.x required for code-server (current: ${major}.x).`);
    warn(`Some features may fail. Use nvm/nvm-windows to switch.`);
  }
} catch {
  fail('Node.js not found. Install: https://nodejs.org/');
}

// ============================================================
// ② Cargo / Rust
// ============================================================
console.log(`\n${CYAN}[2/4] Rust / Cargo${RESET}`);
try {
  const rustcV = execSync('rustc --version', { encoding: 'utf-8', timeout: 5000 }).trim();
  ok(rustcV);
  try {
    const cargoV = execSync('cargo --version', { encoding: 'utf-8', timeout: 5000 }).trim();
    ok(cargoV);
  } catch {
    warn('cargo not found but rustc present — path may be incomplete.');
  }
} catch {
  fail('Rust not found. Install: https://rustup.rs/');
}

// ============================================================
// ③ npm dependencies
// ============================================================
console.log(`\n${CYAN}[3/4] npm dependencies${RESET}`);
const desktopDir = join(ROOT, 'agent-desktop');
if (!existsSync(join(desktopDir, 'node_modules'))) {
  warn('node_modules/ not found. Will auto-install during build.');
} else {
  ok('node_modules/ found');
}

// ============================================================
// ④ Git（仅 --release 模式）
// ============================================================
const isRelease = process.argv.includes('--release');
console.log(`\n${CYAN}[4/4] Git${RESET}`);
if (isRelease) {
  try {
    const gitV = execSync('git --version', { encoding: 'utf-8', timeout: 5000 }).trim();
    ok(gitV);

    // 检查是否在 main 分支
    try {
      const branch = execSync('git branch --show-current',
        { encoding: 'utf-8', timeout: 5000, cwd: ROOT }).trim();
      if (branch !== 'main') warn(`Not on main branch (current: ${branch})`);
      else ok(`branch: ${branch}`);
    } catch {
      warn('Could not determine current branch');
    }

    // 检查工作区是否干净
    try {
      const dirty = execSync('git status --porcelain',
        { encoding: 'utf-8', timeout: 5000, cwd: ROOT }).trim();
      if (dirty) warn('Working directory has uncommitted changes');
      else ok('working directory clean');
    } catch {
      warn('Could not check git status');
    }
  } catch {
    fail('Git not found. Required for release. Install: https://git-scm.com/');
  }
} else {
  // 非 release 模式只检查 git 存在
  try {
    execSync('git --version', { encoding: 'utf-8', timeout: 5000 });
    ok('Git available');
  } catch {
    warn('Git not found (only needed for release)');
  }
}

// ============================================================
// 总结
// ============================================================
console.log('');
if (errors === 0) {
  console.log(`${GREEN}All checks passed.${RESET}\n`);
  process.exit(0);
} else {
  console.log(`${RED}${errors} check(s) failed.${RESET}\n`);
  process.exit(1);
}
