import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: { alias: { '@mak/rules': decodeURIComponent(new URL('../../packages/rules/src/index.ts', import.meta.url).pathname).replace(/^\/([A-Za-z]:)/, '$1') } },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
})
