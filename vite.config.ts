import { fileURLToPath, URL } from "node:url";

import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // 配合 Tauri：关闭 vite 自己的清屏，避免盖住 tauri dev 的日志
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // Rust 源码变更由 cargo 负责，vite 不需要监听
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
