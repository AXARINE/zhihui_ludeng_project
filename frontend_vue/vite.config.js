import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      // 使用 import.meta.dirname（Node 20.11+ / Vite 8 native loader 要求），替代 __dirname
      '@': path.resolve(import.meta.dirname, 'src')
    }
  },
  // ============================================
  // 开发服务器代理（解决跨域 CORS 问题）
  // ============================================
  // 前端跑在 5173，后端跑在 8080，两个端口不同就算"跨域"。
  // 后端没加 CORS 响应头，浏览器会直接拦截跨域请求。
  //
  // 加了代理后：前端请求 /api/xxx（走 5173，同源），
  // Vite 在"服务器端"帮我们把 /api 开头的请求转发给 8080，
  // 浏览器以为一直是同源，就不会拦了。
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true
        // 注意：不写 rewrite，路径原样转发
        // 前端请求 /api/devices → 后端收到 http://localhost:8080/api/devices
      }
    }
  }
})
