import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// https://v2.tauri.app/start/frontend/vite/
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,

  // 路径别名 — 用 @/ 替代多层 ../../
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },

  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  // 生产构建优化
  build: {
    target: "es2021",
    minify: "esbuild",
    // 代码分割 — 按依赖类型拆包，减少首屏加载体积
    rollupOptions: {
      output: {
        manualChunks: {
          // UI 框架（稳定，更新频率低）
          vendor: ["react", "react-dom", "zustand"],
          // Monaco 编辑器（~5MB 独立 chunk，按需加载）
          monaco: ["@monaco-editor/react"],
          // 终端模拟器
          xterm: ["@xterm/xterm", "@xterm/addon-fit", "@xterm/addon-web-links"],
          // 国际化
          i18n: ["i18next", "react-i18next"],
          // Markdown 渲染
          markdown: ["react-markdown", "react-syntax-highlighter", "remark-gfm"],
        },
      },
    },
    // 超过 500KB 才报警（拆包后单个 chunk 通常 < 200KB）
    chunkSizeWarningLimit: 500,
  },
});
