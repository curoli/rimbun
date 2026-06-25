import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

const backendPort = process.env.RIMBUN_PORT ?? "3000";
const backendTarget = `http://127.0.0.1:${backendPort}`;

export default defineConfig({
  plugins: [vue()],
  server: {
    proxy: {
      "/api": {
        target: backendTarget,
        changeOrigin: true,
      },
      "/health": {
        target: backendTarget,
        changeOrigin: true,
      },
    },
  },
});
