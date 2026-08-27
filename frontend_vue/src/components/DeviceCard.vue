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
import { formatBeijingTime } from '@/utils/time'

// ---- 权限判断 ----
function hasPerm(code) {
  try {
    const perms = JSON.parse(localStorage.getItem('permissions') || '[]')
    const role = JSON.parse(localStorage.getItem('role') || '{}')
    if (role.role_code === 'super_admin') return true
    return perms.includes(code)
  } catch { return false }
}

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
  const time = device.value ? device.value.last_seen_at : ''
  return time ? formatBeijingTime(time) : ''
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
      <span class="status-pill" :class="status.toLowerCase()">
        {{ status === 'ONLINE' ? '在线' : status === 'OFFLINE' ? '离线' : '故障' }}
      </span>
    </div>

    <!-- 灯状态显示 -->
    <div class="lamp-info">
      <span class="label">灯状态</span>
      <span class="value" :class="lamp === 'ON' ? 'on' : 'off'">
        {{ lamp === 'ON' ? '已开启' : '已关闭' }}
      </span>
    </div>

    <!-- 工作模式 -->
    <div class="mode-info">
      <span class="label">模式</span>
      <span class="value">
        {{ mode === 'AUTO' ? '自动' : '手动' }}
      </span>
    </div>

    <!-- 控制按钮 — 需要 control:manual 权限 -->
    <div v-if="hasPerm('control:manual')" class="device-controls">
      <button
        class="btn btn-on"
        @click.stop="handleTurnOn"
        :disabled="lamp === 'ON'"
      >
        开灯
      </button>
      <button
        class="btn btn-off"
        @click.stop="handleTurnOff"
        :disabled="lamp === 'OFF'"
      >
        关灯
      </button>
      <button
        class="btn btn-auto"
        @click.stop="handleAutoMode"
        :disabled="mode === 'AUTO'"
      >
        自动
      </button>
    </div>
    <!-- 无权限提示 -->
    <div v-else class="device-controls no-perm">
      <span class="no-perm-text">无控制权限</span>
    </div>

    <!-- 最后在线时间 -->
    <div class="update-time" v-if="lastSeenAt">
      最后在线：{{ lastSeenAt }}
    </div>
  </div>
</template>

<style scoped>
.device-card {
  border: 1px solid #e8e4dc;
  border-top: 3px solid #ded9cf;
  border-radius: 12px;
  padding: 18px;
  margin: 8px;
  background: #ffffff;
  box-shadow: var(--shadow-sm);
  transition: box-shadow 0.2s, transform 0.2s;
  cursor: pointer;
}

.device-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

/* 状态色收到卡片顶部饰条 */
.device-card.status-online {
  border-top-color: #5f8f5a;
}

.device-card.status-offline {
  border-top-color: #d5cfc4;
}

.device-card.status-fault {
  border-top-color: #be4b40;
}

.device-header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 12px;
}

.device-name {
  margin: 0;
  font-size: 17px;
  font-family: var(--font-serif);
  font-weight: 600;
  color: #1f1c19;
}

.device-id {
  font-size: 11px;
  font-family: var(--font-mono);
  color: #a8a29c;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.device-status {
  margin-bottom: 12px;
}

/* 状态药丸徽章 */
.status-pill {
  display: inline-block;
  font-size: 12px;
  font-weight: 500;
  padding: 2px 10px;
  border-radius: 999px;
}

.status-pill.online {
  background: #f0f6ee;
  color: #4f7a4b;
}

.status-pill.offline {
  background: #f3f1ec;
  color: #8a837b;
}

.status-pill.fault {
  background: #f9ece9;
  color: #a03b31;
}

/* 键值行：发丝线分隔 */
.lamp-info, .mode-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 9px 0;
  border-top: 1px solid #f5f2ec;
  font-size: 13px;
}

.lamp-info .label, .mode-info .label {
  color: #8a837b;
}

.lamp-info .value {
  font-weight: 600;
}

.lamp-info .value.on {
  color: #c08340;
}

.lamp-info .value.off {
  color: #a8a29c;
}

.mode-info .value {
  font-weight: 600;
  color: #c96a4a;
}

/* 幽灵按钮：描边 + 悬停填充 */
.device-controls {
  display: flex;
  gap: 8px;
  margin: 14px 0 4px;
}

.btn {
  flex: 1;
  padding: 7px 0;
  border: 1px solid;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  background: transparent;
  transition: all 0.15s ease;
}

.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.btn-on {
  border-color: #5f8f5a;
  color: #4f7a4b;
}

.btn-on:hover:not(:disabled) {
  background: #5f8f5a;
  color: #f7faf6;
}

.btn-off {
  border-color: #be4b40;
  color: #a03b31;
}

.btn-off:hover:not(:disabled) {
  background: #be4b40;
  color: #fdf5f4;
}

.btn-auto {
  border-color: #c96a4a;
  color: #b85a3c;
}

.btn-auto:hover:not(:disabled) {
  background: #c96a4a;
  color: #fff7f2;
}

.update-time {
  font-size: 11px;
  font-family: var(--font-mono);
  color: #b4ada3;
  text-align: right;
  margin-top: 8px;
}

.no-perm {
  justify-content: center;
}

.no-perm-text {
  font-size: 12px;
  color: #b4ada3;
  padding: 8px 0;
}
</style>
