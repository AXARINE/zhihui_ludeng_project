<script setup>
/**
 * 设备详情页面
 *
 * 功能：
 * - 展示单个设备的详细信息
 * - 控制设备（开灯/关灯/自动）— 需要 control:manual 权限
 * - 编辑设备信息 — 需要 device:manage 权限
 */
import { ref, computed, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { useDeviceStore } from '@/stores/deviceStore'
import { updateDevice } from '@/api/device'
import { formatCoordDms } from '@/utils/coord'
import { formatBeijingTime } from '@/utils/time'

const deviceStore = useDeviceStore()
const route = useRoute()
const router = useRouter()
const deviceId = route.params.id

// ---- 权限判断 ----
function hasPerm(code) {
  try {
    const perms = JSON.parse(localStorage.getItem('permissions') || '[]')
    const role = JSON.parse(localStorage.getItem('role') || '{}')
    if (role.role_code === 'super_admin') return true
    return perms.includes(code)
  } catch { return false }
}

const device = computed(() => deviceStore.getDeviceById(deviceId))

// ---- 编辑设备 ----
const editVisible = ref(false)
const editLoading = ref(false)
const editForm = ref({ id: '', name: '', location: '' })

function handleEdit() {
  if (!device.value) return
  editForm.value = {
    id: device.value.id,
    name: device.value.name || '',
    location: device.value.location || ''
  }
  editVisible.value = true
}

async function handleSaveDevice() {
  editLoading.value = true
  try {
    const data = {}
    if (editForm.value.name.trim()) data.name = editForm.value.name.trim()
    if (editForm.value.location.trim()) data.location = editForm.value.location.trim()
    if (Object.keys(data).length === 0) {
      ElMessage.warning('没有可更新的字段')
      return
    }
    await updateDevice(editForm.value.id, data)
    ElMessage.success('设备信息已更新')
    editVisible.value = false
    deviceStore.fetchDeviceList()
  } catch (e) {
    ElMessage.error('更新失败：' + (e?.response?.data || e.message))
  } finally {
    editLoading.value = false
  }
}

// ---- 控制设备 ----
const controlDevice = async (action) => {
  const result = await deviceStore.controlDevice(deviceId, action)
  if (result.success) {
    ElMessage.success(`设备 ${action === 'on' ? '开灯' : action === 'off' ? '关灯' : '自动'} 操作成功`)
  } else {
    ElMessage.error(`控制失败：${result.message}`)
  }
}

// ---- 状态显示 ----
const getStatusType = (status) => {
  const typeMap = { ONLINE: 'success', OFFLINE: 'info', FAULT: 'danger' }
  return typeMap[status] || 'info'
}

const getStatusText = (status) => {
  const textMap = { ONLINE: '在线', OFFLINE: '离线', FAULT: '故障' }
  return textMap[status] || '未知'
}

onMounted(() => {
  if (deviceStore.deviceList.length === 0) {
    deviceStore.fetchDeviceList()
  }
})
</script>

<template>
  <div class="device-detail-page">
    <!-- 返回按钮 -->
    <div class="back-button">
      <el-button @click="router.push('/devices')">← 返回设备列表</el-button>
    </div>

    <!-- 设备信息卡片 -->
    <el-card v-if="device" class="device-info-card">
      <template #header>
        <div class="card-header">
          <h3>{{ device.name }}</h3>
          <div>
            <el-tag :type="getStatusType(device.status)">
              {{ getStatusText(device.status) }}
            </el-tag>
            <!-- 编辑按钮 — 需要 device:manage 权限 -->
            <el-button
              v-if="hasPerm('device:manage')"
              type="warning"
              size="small"
              style="margin-left: 12px"
              @click="handleEdit"
            >
              编辑设备
            </el-button>
          </div>
        </div>
      </template>

      <el-descriptions :column="2" border>
        <el-descriptions-item label="设备ID">{{ device.id }}</el-descriptions-item>
        <el-descriptions-item label="设备名称">{{ device.name }}</el-descriptions-item>
        <el-descriptions-item label="位置">{{ device.location || '-' }}</el-descriptions-item>
        <el-descriptions-item label="经纬度">
          <template v-if="device.latitude != null && device.longitude != null">
            <div>{{ formatCoordDms(device.latitude, device.longitude) }}</div>
            <div class="coord-decimal">{{ device.latitude.toFixed(6) }}, {{ device.longitude.toFixed(6) }}（WGS84）</div>
          </template>
          <template v-else>未定位</template>
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
          {{ device.last_seen_at ? formatBeijingTime(device.last_seen_at) : '从未' }}
        </el-descriptions-item>
      </el-descriptions>
    </el-card>

    <!-- 设备不存在提示 -->
    <el-card v-else class="device-info-card">
      <div class="not-found">
        <h3>设备不存在</h3>
        <p>未找到ID为 {{ deviceId }} 的设备</p>
        <el-button type="primary" @click="router.push('/devices')">返回设备列表</el-button>
      </div>
    </el-card>

    <!-- 控制面板 — 需要 control:manual 权限 -->
    <el-card v-if="device && hasPerm('control:manual')" class="control-card">
      <template #header>
        <h3>设备控制</h3>
      </template>
      <div class="control-buttons">
        <el-button type="success" size="large" @click="controlDevice('on')" :disabled="device.lamp === 'ON'">
          💡 开灯
        </el-button>
        <el-button type="danger" size="large" @click="controlDevice('off')" :disabled="device.lamp === 'OFF'">
          🌑 关灯
        </el-button>
        <el-button type="warning" size="large" @click="controlDevice('auto')" :disabled="device.mode === 'AUTO'">
          🔄 自动模式
        </el-button>
      </div>
    </el-card>

    <!-- 无控制权限提示 -->
    <el-card v-if="device && !hasPerm('control:manual')" class="control-card">
      <template #header>
        <h3>设备控制</h3>
      </template>
      <el-alert title="您没有控制路灯的权限" type="info" description="请联系管理员获取 control:manual 权限" show-icon :closable="false" />
    </el-card>

    <!-- 历史数据图表（占位） -->
    <el-card v-if="device" class="chart-card">
      <template #header>
        <h3>历史数据</h3>
      </template>
      <div class="chart-placeholder">
        <p>📊 图表功能开发中...</p>
        <p>这里将显示设备的历史光照数据图表</p>
      </div>
    </el-card>

    <!-- 编辑设备对话框 -->
    <el-dialog v-model="editVisible" title="编辑设备信息" width="420px">
      <el-form label-width="80px">
        <el-form-item label="设备ID">
          <el-input :value="editForm.id" disabled />
        </el-form-item>
        <el-form-item label="设备名称">
          <el-input v-model="editForm.name" placeholder="请输入设备名称" />
        </el-form-item>
        <el-form-item label="位置">
          <el-input v-model="editForm.location" placeholder="请输入设备位置" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="editVisible = false">取消</el-button>
        <el-button type="primary" :loading="editLoading" @click="handleSaveDevice">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.device-detail-page {
  padding: 24px;
}

.coord-decimal {
  font-size: 12px;
  color: #8a837b;
  font-family: var(--font-mono);
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
  font-family: var(--font-serif);
  font-weight: 600;
  color: #1f1c19;
}

.lamp-on {
  color: #c08340;
  font-weight: 600;
}

.lamp-off {
  color: #a8a29c;
}

.not-found {
  text-align: center;
  padding: 40px;
}

.not-found h3 {
  margin-bottom: 10px;
  color: #be4b40;
}

.not-found p {
  margin-bottom: 20px;
  color: #8a837b;
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
  color: #8a837b;
}

.chart-placeholder p {
  margin: 10px 0;
}
</style>
