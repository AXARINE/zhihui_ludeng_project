/**
 * Vue Router 配置文件
 */

import { createRouter, createWebHistory } from 'vue-router'
import { getMe } from '@/api/device'

const routes = [
  { path: '/', redirect: '/dashboard' },

  {
    path: '/login',
    name: 'Login',
    component: () => import('@/pages/Login.vue'),
    meta: { title: '登录', public: true }
  },

  {
    path: '/dashboard',
    name: 'Dashboard',
    component: () => import('@/pages/Dashboard.vue'),
    meta: { title: '首页大屏' }
  },

  {
    path: '/devices',
    name: 'DeviceList',
    component: () => import('@/pages/DeviceList.vue'),
    meta: { title: '设备列表' }
  },

  {
    path: '/map',
    name: 'MapPage',
    component: () => import('@/pages/MapPage.vue'),
    meta: { title: '设备地图' }
  },

  {
    path: '/device/:id',
    name: 'DeviceDetail',
    component: () => import('@/pages/DeviceDetail.vue'),
    meta: { title: '设备详情' }
  },

  {
    path: '/alarms',
    name: 'AlarmList',
    component: () => import('@/pages/AlarmList.vue'),
    meta: { title: '告警列表' }
  },

  {
    path: '/config',
    name: 'ThresholdConfig',
    component: () => import('@/pages/ThresholdConfig.vue'),
    meta: { title: '阈值配置' }
  },

  {
    path: '/commands',
    name: 'CommandLog',
    component: () => import('@/pages/CommandLog.vue'),
    meta: { title: '审计日志' }
  },

  {
    path: '/assistant',
    name: 'AssistantQA',
    component: () => import('@/pages/AssistantQA.vue'),
    meta: { title: '智能问答' }
  },

  {
    path: '/users',
    name: 'UserManage',
    component: () => import('@/pages/UserManage.vue'),
    meta: { title: '账号管理' }
  },

  {
    path: '/permissions',
    name: 'PermissionManage',
    component: () => import('@/pages/PermissionManage.vue'),
    meta: { title: '权限管理' }
  }
]

const router = createRouter({
  history: createWebHistory(),
  routes
})

// 路由守卫：未登录跳转登录页
// 每次路由切换时验证 token 有效性，防止重启前端后自动登录旧账号
router.beforeEach(async (to, from, next) => {
  document.title = to.meta.title ? `${to.meta.title} - 智慧路灯系统` : '智慧路灯系统'

  // 公开页面（如登录页）直接放行
  if (to.meta.public) {
    next()
    return
  }

  const token = localStorage.getItem('token')
  if (!token) {
    next('/login')
    return
  }

  // 验证 token 有效性：调用 /api/auth/me
  try {
    await getMe()
    next()
  } catch (e) {
    // token 无效或已过期，清除本地数据，跳转登录页
    localStorage.removeItem('token')
    localStorage.removeItem('user')
    localStorage.removeItem('role')
    localStorage.removeItem('permissions')
    next('/login')
  }
})

export default router
