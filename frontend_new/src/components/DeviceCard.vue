<script setup>
/**
 * 设备卡片组件
 *
 * 作用：展示单个设备的信息和控制按钮
 * 关键点：从 Pinia Store 获取数据，实时同步
 */

import { ref, computed, onMounted, onUnmounted } from 'vue'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// ============================================
// 1. 定义 props
// ============================================
const props = defineProps({
  deviceId: {
    type: String,
    required: true
  },
  deviceName: {
    type: String,
    default: '智慧路灯'
  },
  initialStatus: {
    type: String,
    default: 'OFFLINE'
  },
  initialLamp: {
    type: String,
    default: 'OFF'
  },
  initialMode: {
    type: String,
    default: 'MANUAL'
  }
})

// ============================================
// 2. 定义 emit
// ============================================
const emit = defineEmits(['control', 'click'])

// ============================================
// 3. 获取 Store
// ============================================
const deviceStore = useDeviceStore()

// ============================================
// 4. 计算属性：从 Store 获取最新数据
// ============================================
const device = computed(() => {
  return deviceStore.getDeviceById(props.deviceId)
})

// 使用 Store 中的数据，如果存在的话
const status = computed(() => {
  return device.value ? device.value.status : props.initialStatus
})

const lamp = computed(() => {
  return device.value ? device.value.lamp : props.initialLamp
})

const mode = computed(() => {
  return device.value ? device.value.mode : props.initialMode
})

const lastSeenAt = computed(() => {
  return device.value ? device.value.last_seen_at : ''
})

// ============================================
// 5. 方法
// ============================================
const handleTurnOn = async () => {
  console.log('开灯：', props.deviceId)

  // 调用 Store 的控制方法
  const result = await deviceStore.controlDevice(props.deviceId, 'on')

  if (result.success) {
    emit('control', {
      deviceId: props.deviceId,
      action: 'on',
      timestamp: new Date().toISOString()
    })
  }
}

const handleTurnOff = async () => {
  console.log('关灯：', props.deviceId)

  // 调用 Store 的控制方法
  const result = await deviceStore.controlDevice(props.deviceId, 'off')

  if (result.success) {
    emit('control', {
      deviceId: props.deviceId,
      action: 'off',
      timestamp: new Date().toISOString()
    })
  }
}

const handleAutoMode = async () => {
  console.log('自动模式：', props.deviceId)

  // 调用 Store 的控制方法
  const result = await deviceStore.controlDevice(props.deviceId, 'auto')

  if (result.success) {
    emit('control', {
      deviceId: props.deviceId,
      action: 'auto',
      timestamp: new Date().toISOString()
    })
  }
}

const handleClick = () => {
  emit('click', props.deviceId)
}

// ============================================
// 6. 生命周期
// ============================================
onMounted(() => {
  console.log(`设备 ${props.deviceName} 组件已挂载`)
})

onUnmounted(() => {
  console.log(`设备 ${props.deviceName} 组件已卸载`)
})
</script>

<template>
  <div
    class="device-card"
    :class="[`status-${status.toLowerCase()}`]"
    @click="handleClick"
  >
    <!-- 设备头部 -->
    <div class="device-header">
      <h3 class="device-name">{{ deviceName }}</h3>
      <span class="device-id">ID: {{ deviceId }}</span>
    </div>

    <!-- 状态显示 -->
    <div class="device-status">
      <span class="status-dot" :class="status.toLowerCase()"></span>
      <span class="status-text">
        {{ status === 'ONLINE' ? '在线' : status === 'OFFLINE' ? '离线' : '故障' }}
      </span>
    </div>

    <!-- 灯状态显示 -->
    <div class="lamp-info">
      <span class="label">灯状态：</span>
      <span class="value" :class="lamp === 'ON' ? 'on' : 'off'">
        {{ lamp === 'ON' ? '💡 已开启' : '🌑 已关闭' }}
      </span>
    </div>

    <!-- 工作模式 -->
    <div class="mode-info">
      <span class="label">模式：</span>
      <span class="value">
        {{ mode === 'AUTO' ? '🔄 自动' : '✋ 手动' }}
      </span>
    </div>

    <!-- 控制按钮 -->
    <div class="device-controls">
      <button
        class="btn btn-on"
        @click.stop="handleTurnOn"
        :disabled="lamp === 'ON'"
      >
        💡 开灯
      </button>
      <button
        class="btn btn-off"
        @click.stop="handleTurnOff"
        :disabled="lamp === 'OFF'"
      >
        🌑 关灯
      </button>
      <button
        class="btn btn-auto"
        @click.stop="handleAutoMode"
        :disabled="mode === 'AUTO'"
      >
        🔄 自动
      </button>
    </div>

    <!-- 最后在线时间 -->
    <div class="update-time" v-if="lastSeenAt">
      最后在线：{{ lastSeenAt }}
    </div>
  </div>
</template>

<style scoped>
.device-card {
  border: 2px solid #e0e0e0;
  border-radius: 12px;
  padding: 16px;
  margin: 8px;
  background: linear-gradient(135deg, #ffffff 0%, #f5f5f5 100%);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  transition: all 0.3s ease;
  cursor: pointer;
}

.device-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
}

.device-card.status-online {
  border-color: #4caf50;
}

.device-card.status-offline {
  border-color: #9e9e9e;
}

.device-card.status-fault {
  border-color: #f44336;
}

.device-header {
  margin-bottom: 12px;
}

.device-name {
  margin: 0 0 4px 0;
  font-size: 18px;
  color: #333;
}

.device-id {
  font-size: 12px;
  color: #666;
  display: block;
}

.device-status {
  display: flex;
  align-items: center;
  margin-bottom: 8px;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  margin-right: 8px;
}

.status-dot.online {
  background-color: #4caf50;
  box-shadow: 0 0 8px rgba(76, 175, 80, 0.5);
}

.status-dot.offline {
  background-color: #9e9e9e;
}

.status-dot.fault {
  background-color: #f44336;
  box-shadow: 0 0 8px rgba(244, 67, 54, 0.5);
}

.lamp-info, .mode-info {
  margin-bottom: 8px;
  font-size: 14px;
}

.lamp-info .label, .mode-info .label {
  color: #666;
}

.lamp-info .value {
  font-weight: bold;
}

.lamp-info .value.on {
  color: #ff9800;
}

.lamp-info .value.off {
  color: #9e9e9e;
}

.mode-info .value {
  font-weight: bold;
  color: #2196f3;
}

.device-controls {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}

.btn {
  flex: 1;
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  font-size: 14px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-on {
  background-color: #4caf50;
  color: white;
}

.btn-on:hover:not(:disabled) {
  background-color: #45a049;
}

.btn-off {
  background-color: #f44336;
  color: white;
}

.btn-off:hover:not(:disabled) {
  background-color: #da190b;
}

.btn-auto {
  background-color: #2196f3;
  color: white;
}

.btn-auto:hover:not(:disabled) {
  background-color: #1976d2;
}

.update-time {
  font-size: 12px;
  color: #999;
  text-align: right;
}
</style>
