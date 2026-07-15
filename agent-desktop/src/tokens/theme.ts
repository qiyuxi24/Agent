/**
 * Votek Design Tokens — TypeScript 侧颜色常量
 *
 * 与 variables.css 保持同步，供需要 JS 直接操作颜色的场景使用
 * （canvas 渲染、图表库、动态 style 等）。
 *
 * 命名约定：
 *   camelCase，与 CSS --palette-* / --color-* / --bg-* 对应。
 *
 * 同步方式：手动。修改变量时两侧一起改。
 */

export const palette = {
  white:    "#ffffff",
  black:    "#000000",

  gray: {
    50:  "#f7f7f8",
    100: "#f0f0f0",
    200: "#e5e7eb",
    400: "#9ca3af",
    500: "#6b7280",
    600: "#4b5563",
    700: "#374151",
  },

  brown: {
    400: "#daa06d",
    500: "#c08040",
    600: "#9e6530",
  },

  green: {
    400: "#4ade80",
    500: "#22c55e",
    600: "#16a34a",
  },

  red: {
    400: "#f87171",
    500: "#ef4444",
    600: "#dc2626",
  },

  amber: {
    500: "#f59e0b",
    600: "#d97706",
  },

  purple: {
    600: "#7c3aed",
  },

  sky: {
    500: "#0ea5e9",
  },

  warm: {
    50: "#faf3ea",
  },

  blue: {
    300: "#28b4ff",
    400: "#1a8fff",
    500: "#007acc",
  },

  dark: {
    400: "#3f3f5c",
    500: "#2d2d5e",
    600: "#2a2a3e",
    700: "#1e1e2e",
    800: "#181825",
    900: "#11111b",
  },

  code: {
    bg:     "#282c34",
    text:   "#abb2bf",
    border: "#4b5263",
    hover:  "#3e4452",
  },

  misc: {
    stderr:   "#a0c0a0",
    envChip:  "#a0a0ff",
    mcpOff:   "#999999",
  },
} as const;

/** Light-theme semantic colors */
export const semanticLight = {
  accent:       palette.brown[500],
  accentHover:  palette.brown[600],
  success:      palette.green[500],
  successDark:  palette.green[600],
  danger:       palette.red[500],
  dangerHover:  palette.red[600],
  error:        palette.red[400],
  warning:      palette.amber[600],
  info:         palette.sky[500],
  brand:        palette.purple[600],
  ide:          palette.blue[500],
  ideLight:     palette.blue[400],
  ideProgress:  palette.blue[300],
  stderr:       palette.misc.stderr,
  envChip:      palette.misc.envChip,
  mcpOff:       palette.misc.mcpOff,
} as const;

/** Light-theme component tokens */
export const tokensLight = {
  ...semanticLight,

  bgPrimary:          palette.white,
  bgSecondary:        palette.gray[50],
  bgSidebar:          palette.gray[100],
  bgBubbleUser:       palette.warm[50],
  bgBubbleAssistant:  palette.white,
  bgHover:            "rgba(0, 0, 0, 0.04)",
  bgElevated:         palette.white,

  textPrimary:    "#1a1a2e",
  textSecondary:  palette.gray[500],
  textMuted:      palette.gray[400],
  textOnAccent:   palette.white,

  borderColor: palette.gray[200],

  successBg:     "rgba(34, 197, 94, 0.12)",
  successBorder: "rgba(34, 197, 94, 0.30)",
  dangerBg:      "rgba(239, 68, 68, 0.10)",
  dangerBorder:  "rgba(239, 68, 68, 0.20)",
  warningBg:     "rgba(217, 119, 6, 0.10)",
  brandBg:       "rgba(124, 58, 237, 0.10)",

  accentLight:      "rgba(192, 128, 64, 0.12)",
  accentRing:       "rgba(192, 128, 64, 0.08)",
  accentRingStrong: "rgba(192, 128, 64, 0.25)",

  radius:    "12px",
  radiusSm:  "8px",
} as const;

/** Dark-theme component tokens */
export const tokensDark = {
  ...semanticLight,
  accent:       palette.brown[400],
  accentHover:  palette.brown[500],
  danger:       palette.red[400],
  dangerHover:  palette.red[500],
  warning:      palette.amber[500],

  bgPrimary:          palette.dark[700],
  bgSecondary:        palette.dark[800],
  bgSidebar:          palette.dark[900],
  bgBubbleUser:       palette.dark[500],
  bgBubbleAssistant:  palette.dark[600],
  bgHover:            "rgba(255, 255, 255, 0.04)",
  bgElevated:         palette.dark[700],

  textPrimary:    "#e4e4e7",
  textSecondary:  "#a1a1aa",
  textMuted:      "#71717a",
  textOnAccent:   palette.white,

  borderColor: palette.dark[400],

  dangerBg: "rgba(239, 68, 68, 0.08)",

  accentLight:      "rgba(218, 160, 109, 0.15)",
  accentRing:       "rgba(218, 160, 109, 0.08)",
  accentRingStrong: "rgba(218, 160, 109, 0.25)",
} as const;
