<script setup>
/**
 * 首页大屏组件
 *
 * 作用：展示设备的整体状态
 * 包含：
 * - 设备统计（总数、在线、离线、故障）
 * - 图表展示
 * - 设备列表
 *
 * 关键点：使用 Pinia Store 获取数据，实现数据同步
 */

// ============================================
// 1. 导入需要的工具
// ============================================
import { onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// 导入图表组件
import LightTrendChart from '@/components/LightTrendChart.vue'
import DeviceStatusPie from '@/components/DeviceStatusPie.vue'

// 导入设备卡片组件
import DeviceCard from '@/components/DeviceCard.vue'

// ============================================
// 2. 获取 Store 和路由
// ============================================
const deviceStore = useDeviceStore()
const router = useRouter()

// ============================================
// 3. 处理设备控制
// ============================================
const handleControl = async (data) => {
  console.log('控制设备：', data)

  // 调用 Store 的控制方法
  const result = await deviceStore.controlDevice(data.deviceId, data.action)

  if (result.success) {
    alert(`设备 ${data.deviceId} 执行 ${data.action} 操作成功！`)
  } else {
    alert(`控制失败：${result.message}`)
  }
}

// ============================================
// 4. 处理设备点击（跳转到详情页）
// ============================================
const handleDeviceClick = (deviceId) => {
  console.log('查看设备详情：', deviceId)
  router.push(`/device/${deviceId}`)
}

// ============================================
// 5. 组件挂载时执行
// ============================================
onMounted(() => {
  console.log('Dashboard 组件已挂载')
  // 启动自动轮询（每5秒刷新设备状态）
  deviceStore.startPolling()
})

onUnmounted(() => {
  // 离开页面时停止轮询，避免内存泄漏
  deviceStore.stopPolling()
})
</script>

<template>
  <div class="dashboard">
    <!-- ============================================ -->
    <!-- 页面标题 -->
    <!-- ============================================ -->
    <header class="page-header">
      <h1>🏮 智慧路灯管理系统</h1>
      <p class="subtitle">IoT 设备监控平台</p>
    </header>

    <!-- ============================================ -->
    <!-- 统计卡片 -->
    <!-- ============================================ -->
    <section class="stats-section">
      <div class="stat-card">
        <span class="stat-value">{{ deviceStore.deviceTotal }}</span>
        <span class="stat-label">设备总数</span>
      </div>
      <div class="stat-card lamp-on">
        <span class="stat-value">{{ deviceStore.lampOnCount }}</span>
        <span class="stat-label">💡 已开灯</span>
      </div>
      <div class="stat-card lamp-off">
        <span class="stat-value">{{ deviceStore.lampOffCount }}</span>
        <span class="stat-label">🌑 已关灯</span>
      </div>
      <div class="stat-card online">
        <span class="stat-value">{{ deviceStore.onlineCount }}</span>
        <span class="stat-label">在线设备</span>
      </div>
      <div class="stat-card offline">
        <span class="stat-value">{{ deviceStore.offlineCount }}</span>
        <span class="stat-label">离线设备</span>
      </div>
      <div class="stat-card fault">
        <span class="stat-value">{{ deviceStore.faultCount }}</span>
        <span class="stat-label">故障设备</span>
      </div>
    </section>

    <!-- ============================================ -->
    <!-- 图表区域 -->
    <!-- ============================================ -->
    <section class="charts-section">
      <el-row :gutter="20">
        <!-- 光照趋势图 -->
        <el-col :span="16">
          <el-card class="chart-card">
            <LightTrendChart />
          </el-card>
        </el-col>

        <!-- 设备状态饼图 -->
        <el-col :span="8">
          <el-card class="chart-card">
            <DeviceStatusPie />
          </el-card>
        </el-col>
      </el-row>
    </section>

    <!-- ============================================ -->
    <!-- 设备列表 -->
    <!-- ============================================ -->
    <section class="device-section">
      <h2>设备列表</h2>

      <div class="device-grid">
        <!-- v-for：循环渲染设备卡片 -->
        <!-- :key：每个设备的唯一标识 -->
        <!-- :device-id：传递设备ID给子组件 -->
        <!-- @control：监听子组件触发的控制事件 -->
        <!-- @click：监听子组件触发的点击事件 -->
        <DeviceCard
          v-for="device in deviceStore.deviceList"
          :key="device.id"
          :device-id="device.id"
          :device-name="device.name"
          :initial-status="device.status"
          :initial-lamp="device.lamp"
          :initial-mode="device.mode"
          @control="handleControl"
          @click="handleDeviceClick"
        />
      </div>
    </section>
  </div>
</template>

<style scoped>
.dashboard {
  padding: 20px;
  max-width: 1200px;
  margin: 0 auto;
}

.page-header {
  text-align: center;
  padding: 40px 0;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border-radius: 12px;
  margin-bottom: 30px;
}

.page-header h1 {
  font-size: 32px;
  margin-bottom: 8px;
}

.subtitle {
  font-size: 16px;
  opacity: 0.9;
}

.stats-section {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  margin-bottom: 30px;
}

.stat-card {
  background: white;
  padding: 20px;
  border-radius: 8px;
  text-align: center;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.stat-value {
  display: block;
  font-size: 32px;
  font-weight: bold;
  color: #333;
}

.stat-label {
  display: block;
  font-size: 14px;
  color: #666;
  margin-top: 4px;
}

.stat-card.online .stat-value {
  color: #4caf50;
}

.stat-card.offline .stat-value {
  color: #9e9e9e;
}

.stat-card.fault .stat-value {
  color: #f44336;
}

.stat-card.lamp-on .stat-value {
  color: #ff9800;
}

.stat-card.lamp-off .stat-value {
  color: #9e9e9e;
}

.charts-section {
  margin-bottom: 30px;
}

.chart-card {
  margin-bottom: 20px;
}

.device-section h2 {
  font-size: 24px;
  color: #333;
  margin-bottom: 20px;
}

.device-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}
</style>
