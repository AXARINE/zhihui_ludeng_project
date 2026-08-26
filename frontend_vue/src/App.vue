<script setup>
/**
 * 根组件 — 侧边栏 + 顶栏 + 路由出口
 *
 * 角色与权限控制逻辑：
 * - 登录时后端返回 role（含 role_code）和 permissions（权限码数组），存入 localStorage
 * - 侧边栏菜单项根据用户权限动态显示/隐藏
 * - super_admin（系统管理员）拥有全部权限，不受权限数组限制
 *
 * 响应式说明：
 * - localStorage 变更不会触发 Vue 响应式更新
 * - 因此用 ref 存储用户信息，通过 watch(route) 在路由变化时重新读取
 * - 登录后 router.push 会触发路由变化，从而刷新侧边栏
 */
import { useRouter, useRoute } from 'vue-router'
import { ref, computed, watch } from 'vue'
import {
  Monitor, List, Bell, Setting, Document,
  User, ChatDotRound, Lock, UserFilled
} from '@element-plus/icons-vue'

const router = useRouter()
const route = useRoute()
const activeMenu = ref('/dashboard')

// ---- 从 localStorage 读取用户信息（响应式） ----
const currentUser = ref({})
const currentRole = ref({})
const userPermissions = ref([])

/**
 * 从 localStorage 重新读取用户数据
 * 登录后、路由切换时都会调用
 */
function refreshUserFromStorage() {
  try { currentUser.value = JSON.parse(localStorage.getItem('user') || '{}') } catch { currentUser.value = {} }
  try { currentRole.value = JSON.parse(localStorage.getItem('role') || '{}') } catch { currentRole.value = {} }
  try { userPermissions.value = JSON.parse(localStorage.getItem('permissions') || '[]') } catch { userPermissions.value = [] }
}

// 初始化读取
refreshUserFromStorage()

// 路由变化时重新读取（解决登录后侧边栏不刷新的问题）
watch(() => route.path, () => {
  refreshUserFromStorage()
  activeMenu.value = route.path
})

// ---- 登录页面不显示侧边栏 ----
const showSidebar = computed(() => route.meta.public !== true)

// ---- 权限判断 ----
const isSuperAdmin = computed(() => currentRole.value.role_code === 'super_admin')

/**
 * 检查当前用户是否拥有某个权限码
 * super_admin 直接返回 true（拥有全部权限）
 */
function hasPerm(code) {
  if (isSuperAdmin.value) return true
  return userPermissions.value.includes(code)
}

// ---- 菜单路由跳转 ----
const handleMenuSelect = (index) => {
  activeMenu.value = index
  router.push(index)
}

// ---- 退出登录：清理所有 localStorage 数据 ----
function handleLogout() {
  localStorage.removeItem('token')
  localStorage.removeItem('user')
  localStorage.removeItem('role')
  localStorage.removeItem('permissions')
  refreshUserFromStorage()
  router.push('/login')
}
</script>

<template>
  <!-- 登录页面：全屏无侧边栏 -->
  <div v-if="!showSidebar">
    <router-view />
  </div>

  <!-- 主界面：侧边栏 + 内容 -->
  <div v-else class="app-container">
    <aside class="sidebar">
      <div class="logo">
        <h2>🏮 智慧路灯</h2>
        <p>IoT 管理系统</p>
      </div>

      <el-menu
        :default-active="activeMenu"
        @select="handleMenuSelect"
        class="sidebar-menu"
      >
        <!-- 仪表盘 — 所有已登录用户可见 -->
        <el-menu-item index="/dashboard">
          <el-icon><Monitor /></el-icon>
          <span>首页大屏</span>
        </el-menu-item>

        <!-- 设备列表 — 需要 device:status 权限 -->
        <el-menu-item v-if="hasPerm('device:status')" index="/devices">
          <el-icon><List /></el-icon>
          <span>设备列表</span>
        </el-menu-item>

        <!-- 告警列表 — 需要 alarm:log 权限 -->
        <el-menu-item v-if="hasPerm('alarm:log')" index="/alarms">
          <el-icon><Bell /></el-icon>
          <span>告警列表</span>
        </el-menu-item>

        <!-- 审计日志 — 需要 command:log 权限 -->
        <el-menu-item v-if="hasPerm('command:log')" index="/commands">
          <el-icon><Document /></el-icon>
          <span>审计日志</span>
        </el-menu-item>

        <!-- 阈值配置 — 需要 config:threshold 权限 -->
        <el-menu-item v-if="hasPerm('config:threshold')" index="/config">
          <el-icon><Setting /></el-icon>
          <span>阈值配置</span>
        </el-menu-item>

        <!-- 智能问答 — 需要 assistant:qa 权限 -->
        <el-menu-item v-if="hasPerm('assistant:qa')" index="/assistant">
          <el-icon><ChatDotRound /></el-icon>
          <span>智能问答</span>
        </el-menu-item>

        <!-- 账号管理 — 需要 user:manage 权限 -->
        <el-menu-item v-if="hasPerm('user:manage')" index="/users">
          <el-icon><UserFilled /></el-icon>
          <span>账号管理</span>
        </el-menu-item>

        <!-- 权限管理 — 仅系统管理员（super_admin）可见 -->
        <el-menu-item v-if="isSuperAdmin" index="/permissions">
          <el-icon><Lock /></el-icon>
          <span>权限管理</span>
        </el-menu-item>
      </el-menu>

      <!-- 底部用户信息 -->
      <div class="user-info">
        <div class="user-avatar">
          {{ (currentUser.real_name || currentUser.username || '用').charAt(0) }}
        </div>
        <div class="user-meta">
          <div class="user-name">{{ currentUser.real_name || currentUser.username || '未登录' }}</div>
          <div class="user-role">{{ currentRole.role_name || '' }}</div>
        </div>
        <button class="logout-btn" @click="handleLogout">退出</button>
      </div>
    </aside>

    <main class="main-content">
      <router-view />
    </main>
  </div>
</template>

<style scoped>
.app-container {
  display: flex;
  min-height: 100vh;
}

.sidebar {
  width: 220px;
  background-color: #304156;
  color: white;
  display: flex;
  flex-direction: column;
}

.logo {
  padding: 20px;
  text-align: center;
  border-bottom: 1px solid #3d4e60;
}

.logo h2 {
  margin: 0;
  font-size: 20px;
  color: #409eff;
}

.logo p {
  margin: 5px 0 0 0;
  font-size: 12px;
  color: #bfcbd9;
}

.sidebar-menu {
  border-right: none;
  background-color: #304156;
  flex: 1;
}

.user-info {
  padding: 14px 16px;
  border-top: 1px solid #3d4e60;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  color: #bfcbd9;
}

.user-avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: #409eff;
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  font-weight: 600;
  flex-shrink: 0;
}

.user-meta {
  flex: 1;
  min-width: 0;
  line-height: 1.3;
}

.user-name {
  font-size: 13px;
  font-weight: 500;
  color: #e0e6ed;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.user-role {
  font-size: 11px;
  color: #8a95a7;
}

.logout-btn {
  background: none;
  border: 1px solid #bfcbd9;
  color: #bfcbd9;
  padding: 3px 10px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  flex-shrink: 0;
}

.logout-btn:hover {
  border-color: #f56c6c;
  color: #f56c6c;
}

.main-content {
  flex: 1;
  background-color: #f0f2f5;
  overflow-y: auto;
}
</style>
