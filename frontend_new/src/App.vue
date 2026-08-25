<script setup>
import { useRouter, useRoute } from 'vue-router'
import { ref, computed } from 'vue'
import { Monitor, List, Bell, Setting, Document, User, ChatDotRound } from '@element-plus/icons-vue'

const router = useRouter()
const route = useRoute()
const activeMenu = ref('/dashboard')

// 登录页面不显示侧边栏
const showSidebar = computed(() => route.meta.public !== true)

// 当前用户
const currentUser = computed(() => {
  try { return JSON.parse(localStorage.getItem('user') || '{}') } catch { return {} }
})

const handleMenuSelect = (index) => {
  activeMenu.value = index
  router.push(index)
}

function handleLogout() {
  localStorage.removeItem('token')
  localStorage.removeItem('user')
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
        <el-menu-item index="/dashboard">
          <el-icon><Monitor /></el-icon>
          <span>首页大屏</span>
        </el-menu-item>
        <el-menu-item index="/devices">
          <el-icon><List /></el-icon>
          <span>设备列表</span>
        </el-menu-item>
        <el-menu-item index="/alarms">
          <el-icon><Bell /></el-icon>
          <span>告警列表</span>
        </el-menu-item>
        <el-menu-item index="/commands">
          <el-icon><Document /></el-icon>
          <span>审计日志</span>
        </el-menu-item>
        <el-menu-item index="/config">
          <el-icon><Setting /></el-icon>
          <span>阈值配置</span>
        </el-menu-item>
        <el-menu-item index="/assistant">
          <el-icon><ChatDotRound /></el-icon>
          <span>智能问答</span>
        </el-menu-item>
      </el-menu>

      <!-- 底部用户信息 -->
      <div class="user-info">
        <el-icon><User /></el-icon>
        <span>{{ currentUser.real_name || currentUser.username || '未登录' }}</span>
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
  padding: 12px 20px;
  border-top: 1px solid #3d4e60;
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #bfcbd9;
}

.logout-btn {
  margin-left: auto;
  background: none;
  border: 1px solid #bfcbd9;
  color: #bfcbd9;
  padding: 2px 8px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
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
