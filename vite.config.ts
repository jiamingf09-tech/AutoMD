import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      "h264-mp4-encoder": "h264-mp4-encoder/embuild/dist/h264-mp4-encoder.web.js"
    }
  },
  build: {
    chunkSizeWarningLimit: 5500,
    rollupOptions: {
      onwarn(warning, warn) {
        if (warning.code === "EVAL" && warning.id?.includes("h264-mp4-encoder")) return;
        warn(warning);
      },
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("/molstar/")) return "molstar";
          if (id.includes("/h264-mp4-encoder/")) return "media-encoder";
          return "vendor";
        }
      }
    }
  },
  server: {
    port: 5173,
    strictPort: true
  },
  envPrefix: ["VITE_", "TAURI_"]
});
