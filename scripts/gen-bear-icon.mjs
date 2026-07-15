// 生成像素风熊图标：透明底 + 放大 + 方块像素感
// 用法：node scripts/gen-bear-icon.mjs  （在仓库根目录运行）
import { writeFileSync } from 'fs';
import { resolve } from 'path';

const OUT = resolve('agent-desktop/src-tauri/icons/icon.svg');

const N = 48; // 像素网格分辨率（越小越像素化，64 为"一点点"像素感）
const SIZE = 512;
const cell = SIZE / N;

const grid = Array.from({ length: N }, () => Array(N).fill(null));

function setPx(x, y, c) {
  if (x >= 0 && x < N && y >= 0 && y < N && c) grid[y][x] = c;
}

// 在归一化坐标(0..1)下画实心圆
function disc(cx, cy, r, color) {
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const dx = (x + 0.5) / N - cx;
      const dy = (y + 0.5) / N - cy;
      if (dx * dx + dy * dy <= r * r) setPx(x, y, color);
    }
  }
}

// 画实心椭圆
function ellipse(cx, cy, rx, ry, color) {
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const dx = (x + 0.5) / N - cx;
      const dy = (y + 0.5) / N - cy;
      if ((dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1) setPx(x, y, color);
    }
  }
}

const BROWN = '#A9744F';
const INNER = '#C68B59';
const MUZZ = '#EAD7BC';
const BLACK = '#241710';
const WHITE = '#FFFFFF';

// 耳朵（放大后占满画布上部）
disc(0.27, 0.22, 0.17, BROWN);
disc(0.73, 0.22, 0.17, BROWN);
// 头（放大：半径 0.42，几乎占满画布）
disc(0.5, 0.55, 0.42, BROWN);
// 耳内（覆盖）
disc(0.27, 0.22, 0.095, INNER);
disc(0.73, 0.22, 0.095, INNER);
// 口鼻区
ellipse(0.5, 0.66, 0.23, 0.19, MUZZ);
// 眼睛
disc(0.40, 0.50, 0.052, BLACK);
disc(0.60, 0.50, 0.052, BLACK);
// 眼睛高光
setPx(Math.round(0.375 * N), Math.round(0.485 * N), WHITE);
setPx(Math.round(0.575 * N), Math.round(0.485 * N), WHITE);
// 鼻子
ellipse(0.5, 0.60, 0.08, 0.058, BLACK);

// 组装 SVG（透明底，crispEdges 保证像素边缘锐利）
let rects = '';
for (let y = 0; y < N; y++) {
  for (let x = 0; x < N; x++) {
    const c = grid[y][x];
    if (!c) continue;
    rects += `<rect x="${x * cell}" y="${y * cell}" width="${cell}" height="${cell}" fill="${c}"/>`;
  }
}
const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${SIZE}" height="${SIZE}" viewBox="0 0 ${SIZE} ${SIZE}" shape-rendering="crispEdges">${rects}</svg>\n`;

writeFileSync(OUT, svg);
console.log('wrote', OUT, `(${N}x${N} grid)`);
