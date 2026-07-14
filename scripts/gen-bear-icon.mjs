/**
 * Votek 像素风熊图标生成器
 * ====================================================================
 * 作用：用脚本画出一只「透明底 + 放大 + 方块像素感」的棕熊，
 *       输出为 agent-desktop/src-tauri/icons/icon.svg，
 *       再交给 `npx tauri icon` 生成全套平台图标（ico/icns/png/iOS/Android…）。
 *
 * 为什么用脚本而不是手画 SVG：
 *   - 像素风 = 用很多小方块拼出来，手画几百个 <rect> 不现实。
 *   - 改大小/像素粗细只需改一个常量，重跑即可，不用重新手绘。
 *
 * 怎么用（两步，或用 scripts/gen-icon.bat 一步完成）：
 *   1) 在【仓库根目录】生成 icon.svg：
 *        node scripts/gen-bear-icon.mjs            # 默认颗粒度 64
 *        node scripts/gen-bear-icon.mjs 32         # 颗粒度 32（更像素化）
 *        node scripts/gen-bear-icon.mjs 128        # 颗粒度 128（更平滑）
 *   2) 进 agent-desktop 生成全套图标：
 *        cd agent-desktop
 *        npx tauri icon src-tauri/icons/icon.svg
 *
 * 想微调？改下面这几个常量就行：
 *   - N            ：像素网格分辨率。越小越像素化（粗块），越大越平滑。
 *                    例如 N=32 很复古，N=64 是「一点点」像素感，N=128 接近矢量。
 *   - SIZE         ：输出 SVG 边长（像素），保持 512 即可。
 *   - disc/ellipse 调用里的坐标（0..1 归一化）和半径：调熊的脸型/五官位置/大小。
 *   - BROWN/INNER/MUZZ/BLACK/WHITE：改配色。
 * 改完重跑上面两条命令即可生效。
 * ====================================================================
 */

import { writeFileSync } from 'fs';
import { resolve } from 'path';

// 输出路径：仓库根/agent-desktop/src-tauri/icons/icon.svg
const OUT = resolve('agent-desktop/src-tauri/icons/icon.svg');

// ---- 可调参数 ----
// 颗粒度（像素网格分辨率）：可从命令行传入，例如 `node gen-bear-icon.mjs 32`。
// 越小越像素化（粗块），越大越平滑。64 = "一点点"像素感。
const N = parseInt(process.argv[2], 10) || 64;
const SIZE = 512; // 输出 SVG 边长（像素）
const cell = SIZE / N; // 每个像素方块在画布上的边长（512/64 = 8px）

// 画布网格：grid[y][x] 存颜色字符串，null 表示透明（不画）
const grid = Array.from({ length: N }, () => Array(N).fill(null));

// 把某个网格点涂成指定颜色（越界或空颜色则忽略）
function setPx(x, y, c) {
  if (x >= 0 && x < N && y >= 0 && y < N && c) grid[y][x] = c;
}

// 在归一化坐标(0..1)下画实心圆：cx/cy 圆心，r 半径，color 填充色
function disc(cx, cy, r, color) {
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const dx = (x + 0.5) / N - cx;
      const dy = (y + 0.5) / N - cy;
      if (dx * dx + dy * dy <= r * r) setPx(x, y, color);
    }
  }
}

// 画实心椭圆：cx/cy 圆心，rx/ry 横/纵半轴，color 填充色
function ellipse(cx, cy, rx, ry, color) {
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const dx = (x + 0.5) / N - cx;
      const dy = (y + 0.5) / N - cy;
      if ((dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1) setPx(x, y, color);
    }
  }
}

// ---- 配色 ----
const BROWN = '#A9744F'; // 熊的毛色
const INNER = '#C68B59'; // 耳朵内侧
const MUZZ = '#EAD7BC'; // 口鼻浅色区
const BLACK = '#241710'; // 眼睛/鼻子
const WHITE = '#FFFFFF'; // 眼睛高光

// ---- 画熊（后画的覆盖先画的）----
// 耳朵（放大后占满画布上部）
disc(0.27, 0.22, 0.17, BROWN);
disc(0.73, 0.22, 0.17, BROWN);
// 头（放大：半径 0.42，几乎占满画布）
disc(0.5, 0.55, 0.42, BROWN);
// 耳内（覆盖在耳朵之上，形成深浅两层）
disc(0.27, 0.22, 0.095, INNER);
disc(0.73, 0.22, 0.095, INNER);
// 口鼻区（浅色椭圆）
ellipse(0.5, 0.66, 0.23, 0.19, MUZZ);
// 眼睛（两个黑圆）
disc(0.4, 0.5, 0.052, BLACK);
disc(0.6, 0.5, 0.052, BLACK);
// 眼睛高光（各一个白点，位置换算成网格坐标）
setPx(Math.round(0.375 * N), Math.round(0.485 * N), WHITE);
setPx(Math.round(0.575 * N), Math.round(0.485 * N), WHITE);
// 鼻子（黑色小椭圆）
ellipse(0.5, 0.6, 0.08, 0.058, BLACK);

// ---- 组装 SVG ----
// 透明底（SVG 默认透明，不画 rect 的地方就是透明）；
// shape-rendering="crispEdges" 保证方块边缘锐利、不被抗锯齿糊掉。
let rects = '';
for (let y = 0; y < N; y++) {
  for (let x = 0; x < N; x++) {
    const c = grid[y][x];
    if (!c) continue; // null = 透明，跳过
    rects += `<rect x="${x * cell}" y="${y * cell}" width="${cell}" height="${cell}" fill="${c}"/>`;
  }
}
const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${SIZE}" height="${SIZE}" viewBox="0 0 ${SIZE} ${SIZE}" shape-rendering="crispEdges">${rects}</svg>\n`;

writeFileSync(OUT, svg);
console.log('wrote', OUT, `(${N}x${N} grid)`);
