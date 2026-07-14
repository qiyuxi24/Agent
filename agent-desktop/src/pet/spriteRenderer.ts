import type { PetVisualState } from "./types";

/** 真实 Codex 精灵图（8列×9行），加载后由 PetApp 传入 */
export interface SpriteSheet {
  img: HTMLImageElement;
  cols: number;
  rows: number;
  frameW: number;
  frameH: number;
  rowMap: Record<string, number>;
}

/** 默认状态 -> 行号映射（Codex hatch-pet 官方契约） */
export const DEFAULT_ROW_MAP: Record<string, number> = {
  idle: 0,
  "running-right": 1,
  "running-left": 2,
  waving: 3,
  jumping: 4,
  failed: 5,
  waiting: 6,
  running: 7, // 工作中/处理中
  review: 8, // 完成/审查
};

/** 主绘制入口：有精灵图用精灵图，否则用程序化绘制 */
export function drawPet(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  state: PetVisualState,
  timeMs: number,
  sprite: SpriteSheet | null,
) {
  ctx.clearRect(0, 0, w, h);
  if (sprite) {
    drawSprite(ctx, w, h, state, timeMs, sprite);
  } else {
    drawProcedural(ctx, w, h, state, timeMs);
  }
}

/** 绘制 Codex 8x9 精灵图的一帧 */
function drawSprite(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  state: PetVisualState,
  timeMs: number,
  sprite: SpriteSheet,
) {
  // 把状态映射到行；同时兼容前端派生状态
  const rowKey =
    state in sprite.rowMap
      ? state
      : state === "thinking" || state === "working"
        ? "running"
        : state === "done"
          ? "review"
          : state === "error"
            ? "failed"
            : "idle";
  const row = sprite.rowMap[rowKey] ?? 0;

  // 每行动画帧数（idle 6 / 其余 8，取保守值：用列数）
  const frames = sprite.cols;
  const fps = 8;
  const col = Math.floor(timeMs / (1000 / fps)) % frames;

  const sx = col * sprite.frameW;
  const sy = row * sprite.frameH;

  // 缩放铺满画布（保持比例，居中）
  const scale = Math.min(w / sprite.frameW, h / sprite.frameH) * 0.95;
  const dw = sprite.frameW * scale;
  const dh = sprite.frameH * scale;
  const dx = (w - dw) / 2;
  const dy = (h - dh) / 2;

  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(sprite.img, sx, sy, sprite.frameW, sprite.frameH, dx, dy, dw, dh);
}

// ───────────────────────── 程序化宠物（默认，开箱即用） ─────────────────────────

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

const PALETTE: Record<string, { body: string; body2: string; cheek: string }> = {
  idle: { body: "#7ec8e3", body2: "#4aa3c7", cheek: "#ffb3c6" },
  thinking: { body: "#b39ddb", body2: "#8e6fc7", cheek: "#e6b3ff" },
  working: { body: "#80deea", body2: "#4db6c4", cheek: "#ffd180" },
  waiting: { body: "#a5d6a7", body2: "#6fbf73", cheek: "#ffccbc" },
  done: { body: "#fff176", body2: "#ffd54f", cheek: "#ff8a80" },
  error: { body: "#ef9a9a", body2: "#e57373", cheek: "#ff5252" },
  waving: { body: "#f48fb1", body2: "#ec6a9c", cheek: "#ff80ab" },
  jumping: { body: "#ce93d8", body2: "#ba68c8", cheek: "#f8bbd0" },
  failed: { body: "#ef9a9a", body2: "#e57373", cheek: "#ff5252" },
  "running-right": { body: "#80deea", body2: "#4db6c4", cheek: "#ffd180" },
  "running-left": { body: "#80deea", body2: "#4db6c4", cheek: "#ffd180" },
};

function drawProcedural(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  state: PetVisualState,
  t: number,
) {
  const sec = t / 1000;
  const cx = w / 2;
  const baseY = h - 28;

  // 状态驱动的运动参数
  let bob = 0;
  let tilt = 0;
  let shakeX = 0;
  let squash = 1;
  let armWave = 0;

  switch (state) {
    case "idle":
      bob = Math.sin(sec * 2) * 4;
      break;
    case "thinking":
      tilt = Math.sin(sec * 1.5) * 0.08;
      bob = Math.sin(sec * 1.5) * 2;
      break;
    case "working":
      shakeX = Math.sin(sec * 16) * 2;
      bob = Math.sin(sec * 11) * 1.5;
      break;
    case "waiting":
      tilt = Math.sin(sec * 0.8) * 0.12;
      break;
    case "done":
      bob = Math.abs(Math.sin(sec * 6)) * -12;
      squash = 1 + Math.sin(sec * 6) * 0.06;
      break;
    case "error":
    case "failed":
      shakeX = Math.sin(sec * 28) * 4;
      break;
    case "waving":
      armWave = Math.sin(sec * 10) * 0.9;
      tilt = Math.sin(sec * 8) * 0.06;
      break;
    case "jumping":
      bob = Math.abs(Math.sin(sec * 5)) * -16;
      squash = 1 + Math.sin(sec * 5) * 0.08;
      break;
    case "running-right":
      shakeX = Math.sin(sec * 20) * 3;
      bob = Math.abs(Math.sin(sec * 14)) * -3;
      break;
    case "running-left":
      shakeX = -Math.sin(sec * 20) * 3;
      bob = Math.abs(Math.sin(sec * 14)) * -3;
      break;
  }

  const pal = PALETTE[state] ?? PALETTE.idle;
  const bodyW = 96;
  const bodyH = 84 * squash;

  ctx.save();
  ctx.translate(cx + shakeX, baseY + bob);
  ctx.rotate(tilt);

  // 影子
  ctx.save();
  ctx.translate(0, -bob * 0.0 + 6);
  ctx.scale(1, 0.35);
  ctx.fillStyle = "rgba(0,0,0,0.12)";
  ctx.beginPath();
  ctx.arc(0, 0, bodyW * 0.42, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();

  // 身体
  const grad = ctx.createLinearGradient(0, -bodyH, 0, 6);
  grad.addColorStop(0, pal.body);
  grad.addColorStop(1, pal.body2);
  roundRect(ctx, -bodyW / 2, -bodyH, bodyW, bodyH, 30);
  ctx.fillStyle = grad;
  ctx.fill();

  // 高光
  ctx.fillStyle = "rgba(255,255,255,0.25)";
  roundRect(ctx, -bodyW / 2 + 12, -bodyH + 12, 26, 18, 9);
  ctx.fill();

  // 腮红
  ctx.fillStyle = pal.cheek;
  ctx.globalAlpha = 0.7;
  ctx.beginPath();
  ctx.arc(-26, -bodyH * 0.42, 8, 0, Math.PI * 2);
  ctx.arc(26, -bodyH * 0.42, 8, 0, Math.PI * 2);
  ctx.fill();
  ctx.globalAlpha = 1;

  // 眨眼
  const blink = (Math.sin(sec * 1.3) > 0.96) ? 0.15 : 1;
  drawEyes(ctx, -bodyH * 0.6, blink, state);

  // 嘴
  drawMouth(ctx, -bodyH * 0.32, state, sec);

  // 小手（waving 时挥动）
  if (armWave !== 0 || state === "waving") {
    ctx.save();
    ctx.translate(bodyW / 2 - 6, -bodyH * 0.5);
    ctx.rotate(-0.6 + armWave);
    ctx.fillStyle = pal.body2;
    roundRect(ctx, 0, -6, 22, 12, 6);
    ctx.fill();
    ctx.restore();
  }

  // 思考气泡
  if (state === "thinking") {
    ctx.fillStyle = "rgba(255,255,255,0.9)";
    ctx.font = "bold 16px sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("?", 22, -bodyH - 6);
  }
  // 错误叹号
  if (state === "error" || state === "failed") {
    ctx.fillStyle = "#fff";
    ctx.font = "bold 18px sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("!", 0, -bodyH - 8);
  }
  // 完成星星
  if (state === "done") {
    ctx.fillStyle = "#ffd54f";
    ctx.font = "16px sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("✦", -bodyW / 2 - 6, -bodyH * 0.6);
    ctx.fillText("✦", bodyW / 2 + 6, -bodyH * 0.5);
  }

  ctx.restore();
}

function drawEyes(
  ctx: CanvasRenderingContext2D,
  y: number,
  blink: number,
  state: PetVisualState,
) {
  const ex = 18;
  const eyeR = 7;
  ctx.fillStyle = "#2b2b3a";
  for (const sx of [-ex, ex]) {
    ctx.save();
    ctx.translate(sx, y);
    ctx.scale(1, blink);
    ctx.beginPath();
    ctx.arc(0, 0, eyeR, 0, Math.PI * 2);
    ctx.fill();
    // 高光
    ctx.fillStyle = "rgba(255,255,255,0.9)";
    ctx.beginPath();
    ctx.arc(-2, -2, 2.2, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#2b2b3a";
    ctx.restore();
  }
  // 思考时眼睛稍微上挑（用眉毛点表示）
  if (state === "thinking") {
    ctx.strokeStyle = "#2b2b3a";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(-ex - 4, y - eyeR - 4);
    ctx.lineTo(-ex + 4, y - eyeR - 2);
    ctx.moveTo(ex - 4, y - eyeR - 2);
    ctx.lineTo(ex + 4, y - eyeR - 4);
    ctx.stroke();
  }
}

function drawMouth(
  ctx: CanvasRenderingContext2D,
  y: number,
  state: PetVisualState,
  sec: number,
) {
  ctx.strokeStyle = "#2b2b3a";
  ctx.fillStyle = "#2b2b3a";
  ctx.lineWidth = 2.5;
  ctx.beginPath();
  switch (state) {
    case "done":
    case "waving":
    case "jumping":
      // 大笑：弧线 + 舌头
      ctx.arc(0, y - 4, 10, 0.15 * Math.PI, 0.85 * Math.PI);
      ctx.stroke();
      ctx.fillStyle = "#ff8a80";
      ctx.beginPath();
      ctx.arc(0, y + 2, 5, 0, Math.PI);
      ctx.fill();
      break;
    case "error":
    case "failed":
      // 波浪嘴（沮丧）
      ctx.moveTo(-10, y);
      for (let i = -10; i <= 10; i += 4) {
        ctx.lineTo(i, y + Math.sin((i + sec * 10) * 0.6) * 2);
      }
      ctx.stroke();
      break;
    case "thinking":
    case "working":
      // 小圆嘴
      ctx.arc(0, y - 2, 4, 0, Math.PI * 2);
      ctx.stroke();
      break;
    case "waiting":
      // 一字嘴
      ctx.moveTo(-6, y);
      ctx.lineTo(6, y);
      ctx.stroke();
      break;
    default:
      // 微笑
      ctx.arc(0, y - 4, 9, 0.1 * Math.PI, 0.9 * Math.PI);
      ctx.stroke();
  }
}
