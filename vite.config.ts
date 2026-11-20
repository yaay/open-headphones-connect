import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ command }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "chrome120", outDir: "dist", chunkSizeWarningLimit: 1200 },
  // `local/` holds a personal 3D model for the connect card. It is served
  // while developing but deliberately left out of builds: the built app reads
  // the model from the user's own settings folder instead.
  publicDir: command === "serve" ? "local" : false,
}));
