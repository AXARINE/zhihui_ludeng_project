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
import { ElMessage } from 'element-plus'

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
    ElMessage.success(`设备 ${data.action === 'on' ? '开灯' : data.action === 'off' ? '关灯' : '自动'} 操作成功`)
  } else {
    ElMessage.error(`控制失败：${result.message}`)
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
      <p class="eyebrow">Smart Street Light</p>
      <h1>智慧路灯管理系统</h1>
      <p class="subtitle">IoT 设备监控平台</p>
    </header>

    <!-- ============================================ -->
    <!-- 统计卡片 -->
    <!-- ============================================ -->
    <section class="stats-section">
      <div class="stat-card">
        <span class="stat-label">设备总数</span>
        <span class="stat-value">{{ deviceStore.deviceTotal }}</span>
      </div>
      <div class="stat-card lamp-on">
        <span class="stat-label">已开灯</span>
        <span class="stat-value">{{ deviceStore.lampOnCount }}</span>
      </div>
      <div class="stat-card lamp-off">
        <span class="stat-label">已关灯</span>
        <span class="stat-value">{{ deviceStore.lampOffCount }}</span>
      </div>
      <div class="stat-card online">
        <span class="stat-label">在线设备</span>
        <span class="stat-value">{{ deviceStore.onlineCount }}</span>
      </div>
      <div class="stat-card offline">
        <span class="stat-label">离线设备</span>
        <span class="stat-value">{{ deviceStore.offlineCount }}</span>
      </div>
      <div class="stat-card fault">
        <span class="stat-label">故障设备</span>
        <span class="stat-value">{{ deviceStore.faultCount }}</span>
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
  padding: 24px;
  max-width: 1200px;
  margin: 0 auto;
}

/* 页头 hero：深墨 + 赤陶辉光 */
.page-header {
  position: relative;
  overflow: hidden;
  text-align: center;
  padding: 52px 24px 48px;
  background: #1c1b1a;
  color: #f5f3ee;
  border-radius: 14px;
  margin-bottom: 24px;
}

.page-header::before {
  content: '';
  position: absolute;
  inset: 0;
  background: radial-gradient(640px 220px at 78% -10%, rgba(201, 106, 74, 0.32), transparent 62%);
  pointer-events: none;
}

.eyebrow {
  position: relative;
  font-size: 11px;
  letter-spacing: 0.3em;
  text-transform: uppercase;
  color: #e09b83;
  margin-bottom: 14px;
}

.page-header h1 {
  position: relative;
  font-size: 32px;
  font-family: var(--font-serif);
  font-weight: 600;
  letter-spacing: 0.04em;
  margin-bottom: 12px;
}

.subtitle {
  position: relative;
  font-size: 13px;
  letter-spacing: 0.22em;
  color: #a8a29c;
}

.stats-section {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  margin-bottom: 24px;
}

/* 统计卡：左对齐编辑风，标签 + 大数字 */
.stat-card {
  background: #ffffff;
  border: 1px solid #e8e4dc;
  padding: 18px 20px 16px;
  border-radius: 12px;
  text-align: left;
  box-shadow: var(--shadow-sm);
  transition: box-shadow 0.2s, transform 0.2s;
}

.stat-card:hover {
  box-shadow: var(--shadow-md);
  transform: translateY(-2px);
}

.stat-label {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  letter-spacing: 0.12em;
  color: #8a837b;
}

.stat-label::before {
  content: '';
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #c96a4a;
  flex-shrink: 0;
}

.stat-value {
  display: block;
  font-size: 32px;
  font-weight: 600;
  font-family: var(--font-mono);
  color: #1f1c19;
  margin-top: 10px;
}

.stat-card.online .stat-label::before {
  background: #5f8f5a;
}

.stat-card.offline .stat-label::before {
  background: #c7c0b6;
}

.stat-card.fault .stat-label::before {
  background: #be4b40;
}

.stat-card.lamp-on .stat-label::before {
  background: #dda15e;
}

.stat-card.lamp-off .stat-label::before {
  background: #c7c0b6;
}

.charts-section {
  margin-bottom: 24px;
}

.chart-card {
  margin-bottom: 20px;
}

/* 小节标题：衬线 + 赤陶标尺 */
.device-section h2 {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 22px;
  font-family: var(--font-serif);
  font-weight: 600;
  color: #1f1c19;
  margin-bottom: 16px;
}

.device-section h2::before {
  content: '';
  width: 4px;
  height: 20px;
  background: #c96a4a;
  border-radius: 2px;
}

.device-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}
</style>
