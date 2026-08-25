/**
 * 应用入口文件
 *
 * 作用：启动 Vue 应用，注册插件
 *
 * 这个文件做了什么？
 * 1. 创建 Vue 应用
 * 2. 注册 Vue Router（页面路由）
 * 3. 注册 Element Plus（UI 组件库）
 * 4. 注册 Pinia（状态管理）
 * 5. 挂载到 HTML 中的 #app 元素
 */

// ============================================
// 1. 导入需要的工具
// ============================================
import { createApp } from 'vue'        // 创建 Vue 应用的工具
import { createPinia } from 'pinia'    // Pinia 状态管理
import App from './App.vue'            // 根组件
import router from './router'          // 路由配置
import ElementPlus from 'element-plus' // Element Plus UI 库
import 'element-plus/dist/index.css'   // Element Plus 样式
import './style.css'                   // 全局样式

// ============================================
// 2. 创建 Vue 应用
// ============================================
const app = createApp(App)

// ============================================
// 3. 创建 Pinia 实例
// ============================================
const pinia = createPinia()

// ============================================
// 4. 注册插件
// ============================================
app.use(pinia)         // 使用 Pinia 状态管理
app.use(router)        // 使用路由
app.use(ElementPlus)   // 使用 Element Plus

// ============================================
// 5. 挂载应用
// ============================================
app.mount('#app')
