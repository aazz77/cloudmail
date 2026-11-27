import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

// 构建到后端的 frontend/dist/ 目录（rust-embed 内嵌该目录）。
// emptyOutDir=true 会先清空 dist/ 再生成，确保产物不残留。
export default defineConfig({
  base: '/',
  plugins: [vue()],
  build: {
    outDir: '../frontend/dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 2000,
    rollupOptions: {
      output: {
        manualChunks: {
          vue: ['vue', 'vue-router'],
          element: ['element-plus', '@element-plus/icons-vue'],
        },
      },
    },
  },
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/proxy': 'http://127.0.0.1:8080',
      '/oss': 'http://127.0.0.1:8080',
      '/static': 'http://127.0.0.1:8080',
      '/attachments': 'http://127.0.0.1:8080',
    },
  },
});
