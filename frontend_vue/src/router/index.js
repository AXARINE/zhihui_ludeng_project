/**
 * Vue Router 配置文件
 */

import { createRouter, createWebHistory } from 'vue-router'

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
  }
]

const router = createRouter({
  history: createWebHistory(),
  routes
})

// 路由守卫：未登录跳转登录页
router.beforeEach((to, from, next) => {
  document.title = to.meta.title ? `${to.meta.title} - 智慧路灯系统` : '智慧路灯系统'

  const token = localStorage.getItem('token')
  if (!to.meta.public && !token) {
    next('/login')
  } else {
    next()
  }
})

export default router
