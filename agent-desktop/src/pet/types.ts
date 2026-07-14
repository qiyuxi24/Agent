/** 宠物视觉状态（对应 Codex 8x9 精灵图的 9 行 + 交互态） */
export type PetVisualState =
  | "idle"
  | "thinking"
  | "working"
  | "waiting"
  | "done"
  | "error"
  | "waving"
  | "jumping"
  | "failed"
  | "running-right"
  | "running-left";

/** 宠物数值（与 Rust PetStats 对应） */
export interface PetStats {
  mood: number;
  friendship: number;
  energy: number;
  daysTogether: number;
}

/** 真实 Codex 宠物配置（放入 public/pets/<name>/pet.json 后自动启用） */
export interface PetConfig {
  name: string;
  /** 精灵图 URL（相对 pet.html，如 ./pets/default/spritesheet.webp） */
  spritesheet: string;
  /** 网格列数，默认 8 */
  frameCols?: number;
  /** 网格行数，默认 9 */
  frameRows?: number;
  /** 单帧宽，默认 192 */
  frameWidth?: number;
  /** 单帧高，默认 208 */
  frameHeight?: number;
  /** 状态 -> 行号映射（覆盖默认） */
  rowMap?: Partial<Record<PetVisualState, number>>;
}
