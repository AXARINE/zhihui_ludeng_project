<script setup>
/**
 * 设备详情页面
 *
 * 作用：展示单个设备的详细信息
 * 包含：
 * - 设备基本信息
 * - 实时状态
 * - 控制按钮
 * - 历史数据图表
 *
 * 关键点：使用 Pinia Store 获取数据，实现数据同步
 */

import { ref, computed, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// ============================================
// 1. 获取 Store 和路由
// ============================================
const deviceStore = useDeviceStore()
const route = useRoute()
const router = useRouter()

// 获取设备ID
const deviceId = route.params.id

// ============================================
// 2. 计算属性：获取当前设备
// ============================================
const device = computed(() => {
  return deviceStore.getDeviceById(deviceId)
})

// ============================================
// 3. 控制设备
// ============================================
const controlDevice = async (action) => {
  console.log('控制设备：', deviceId, action)

  // 调用 Store 的控制方法
  const result = await deviceStore.controlDevice(deviceId, action)

  if (result.success) {
    alert(`设备 ${deviceId} 执行 ${action} 操作成功！`)
  } else {
    alert(`控制失败：${result.message}`)
  }
}

// ============================================
// 4. 获取状态标签类型
// ============================================
const getStatusType = (status) => {
  const typeMap = {
    ONLINE: 'success',
    OFFLINE: 'info',
    FAULT: 'danger'
  }
  return typeMap[status] || 'info'
}

// ============================================
// 5. 获取状态文本
// ============================================
const getStatusText = (status) => {
  const textMap = {
    ONLINE: '在线',
    OFFLINE: '离线',
    FAULT: '故障'
  }
  return textMap[status] || '未知'
}

// ============================================
// 6. 组件挂载时获取数据
// ============================================
onMounted(() => {
  console.log('DeviceDetail 组件已挂载，设备ID：', deviceId)
  // 从 Store 获取设备列表（如果还没有获取的话）
  if (deviceStore.deviceList.length === 0) {
    deviceStore.fetchDeviceList()
  }
})
</script>

<template>
  <div class="device-detail-page">
    <!-- ============================================ -->
    <!-- 返回按钮 -->
    <!-- ============================================ -->
    <div class="back-button">
      <el-button @click="router.push('/devices')">
        ← 返回设备列表
      </el-button>
    </div>

    <!-- ============================================ -->
    <!-- 设备信息卡片 -->
    <!-- ============================================ -->
    <el-card v-if="device" class="device-info-card">
      <template #header>
        <div class="card-header">
          <h3>{{ device.name }}</h3>
          <el-tag :type="getStatusType(device.status)">
            {{ getStatusText(device.status) }}
          </el-tag>
        </div>
      </template>

      <el-descriptions :column="2" border>
        <el-descriptions-item label="设备ID">
          {{ device.id }}
        </el-descriptions-item>
        <el-descriptions-item label="设备名称">
          {{ device.name }}
        </el-descriptions-item>
        <el-descriptions-item label="灯状态">
          <span :class="device.lamp === 'ON' ? 'lamp-on' : 'lamp-off'">
            {{ device.lamp === 'ON' ? '💡 已开启' : '🌑 已关闭' }}
          </span>
        </el-descriptions-item>
        <el-descriptions-item label="工作模式">
          {{ device.mode === 'AUTO' ? '🔄 自动模式' : '✋ 手动模式' }}
        </el-descriptions-item>
        <el-descriptions-item label="最后在线">
          {{ device.last_seen_at }}
        </el-descriptions-item>
        <el-descriptions-item label="创建时间">
          {{ device.created_at }}
        </el-descriptions-item>
      </el-descriptions>
    </el-card>

    <!-- ============================================ -->
    <!-- 设备不存在提示 -->
    <!-- ============================================ -->
    <el-card v-else class="device-info-card">
      <div class="not-found">
        <h3>设备不存在</h3>
        <p>未找到ID为 {{ deviceId }} 的设备</p>
        <el-button type="primary" @click="router.push('/devices')">
          返回设备列表
        </el-button>
      </div>
    </el-card>

    <!-- ============================================ -->
    <!-- 控制面板 -->
    <!-- ============================================ -->
    <el-card v-if="device" class="control-card">
      <template #header>
        <h3>设备控制</h3>
      </template>

      <div class="control-buttons">
        <el-button
          type="success"
          size="large"
          @click="controlDevice('on')"
          :disabled="device.lamp === 'ON'"
        >
          💡 开灯
        </el-button>

        <el-button
          type="danger"
          size="large"
          @click="controlDevice('off')"
          :disabled="device.lamp === 'OFF'"
        >
          🌑 关灯
        </el-button>

        <el-button
          type="warning"
          size="large"
          @click="controlDevice('auto')"
          :disabled="device.mode === 'AUTO'"
        >
          🔄 自动模式
        </el-button>
      </div>
    </el-card>

    <!-- ============================================ -->
    <!-- 历史数据图表（占位） -->
    <!-- ============================================ -->
    <el-card v-if="device" class="chart-card">
      <template #header>
        <h3>历史数据</h3>
      </template>

      <div class="chart-placeholder">
        <p>📊 图表功能开发中...</p>
        <p>这里将显示设备的历史光照数据图表</p>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.device-detail-page {
  padding: 20px;
}

.back-button {
  margin-bottom: 20px;
}

.device-info-card {
  margin-bottom: 20px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.card-header h3 {
  margin: 0;
  font-size: 20px;
}

.lamp-on {
  color: #ff9800;
  font-weight: bold;
}

.lamp-off {
  color: #9e9e9e;
}

.not-found {
  text-align: center;
  padding: 40px;
}

.not-found h3 {
  margin-bottom: 10px;
  color: #f56c6c;
}

.not-found p {
  margin-bottom: 20px;
  color: #666;
}

.control-card {
  margin-bottom: 20px;
}

.control-buttons {
  display: flex;
  gap: 20px;
  justify-content: center;
}

.chart-card {
  margin-bottom: 20px;
}

.chart-placeholder {
  text-align: center;
  padding: 40px;
  color: #666;
}

.chart-placeholder p {
  margin: 10px 0;
}
</style>
