<script setup>
/**
 * 设备列表页面
 *
 * 功能：
 * - 展示所有设备，支持筛选、排序
 * - 控制设备（开灯/关灯/自动）— 需要 control:manual 权限
 * - 编辑设备信息（名称、位置）— 需要 device:manage 权限
 * - 查看设备详情
 *
 * 权限控制：
 * - device:status: 查看设备列表
 * - control:manual: 控制灯开关
 * - device:manage: 编辑/删除设备
 */
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { useDeviceStore } from '@/stores/deviceStore'
import { updateDevice } from '@/api/device'
import { formatBeijingTime } from '@/utils/time'

const deviceStore = useDeviceStore()
const router = useRouter()

// ---- 权限判断 ----
function hasPerm(code) {
  try {
    const perms = JSON.parse(localStorage.getItem('permissions') || '[]')
    const role = JSON.parse(localStorage.getItem('role') || '{}')
    if (role.role_code === 'super_admin') return true
    return perms.includes(code)
  } catch { return false }
}

// ---- 筛选条件 ----
const filters = ref({
  status: '',
  keyword: ''
})

const filteredDeviceList = computed(() => {
  let list = deviceStore.deviceList
  if (filters.value.status) {
    list = list.filter(device => device.status === filters.value.status)
  }
  if (filters.value.keyword) {
    const keyword = filters.value.keyword.toLowerCase()
    list = list.filter(device =>
      device.name.toLowerCase().includes(keyword) ||
      device.id.toLowerCase().includes(keyword)
    )
  }
  return list
})

// ---- 编辑设备对话框 ----
const editVisible = ref(false)
const editLoading = ref(false)
const editForm = ref({
  id: '',
  name: '',
  location: ''
})

function handleEditDevice(device) {
  editForm.value = {
    id: device.id,
    name: device.name || '',
    location: device.location || ''
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
const controlDevice = async (deviceId, action) => {
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

const viewDeviceDetail = (deviceId) => {
  router.push(`/device/${deviceId}`)
}

onMounted(() => {
  deviceStore.fetchDeviceList()
})
</script>

<template>
  <div class="device-list-page">
    <div class="page-header">
      <h2>设备列表</h2>
      <p>管理和监控所有路灯设备</p>
    </div>

    <!-- 筛选条件 -->
    <el-card class="filter-card">
      <el-form :inline="true" :model="filters">
        <el-form-item label="设备状态">
          <el-select v-model="filters.status" placeholder="全部" clearable>
            <el-option label="在线" value="ONLINE" />
            <el-option label="离线" value="OFFLINE" />
            <el-option label="故障" value="FAULT" />
          </el-select>
        </el-form-item>
        <el-form-item label="搜索">
          <el-input v-model="filters.keyword" placeholder="设备名称或ID" clearable />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="deviceStore.fetchDeviceList()">刷新</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- 设备表格 -->
    <el-card class="table-card">
      <el-table :data="filteredDeviceList" v-loading="deviceStore.loading" stripe style="width: 100%">
        <el-table-column prop="id" label="设备ID" min-width="200" show-overflow-tooltip />
        <el-table-column prop="name" label="设备名称" width="150" />
        <el-table-column prop="location" label="位置" width="120">
          <template #default="{ row }">
            {{ row.location || '-' }}
          </template>
        </el-table-column>
        <el-table-column label="状态" width="100">
          <template #default="{ row }">
            <el-tag :type="getStatusType(row.status)">
              {{ getStatusText(row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="灯状态" width="100">
          <template #default="{ row }">
            <span :class="row.lamp === 'ON' ? 'lamp-on' : 'lamp-off'">
              {{ row.lamp === 'ON' ? '💡 开' : '🌑 关' }}
            </span>
          </template>
        </el-table-column>
        <el-table-column label="模式" width="100">
          <template #default="{ row }">
            <span>{{ row.mode === 'AUTO' ? '🔄 自动' : '✋ 手动' }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="last_seen_at" label="最后在线" width="180">
          <template #default="{ row }">
            {{ row.last_seen_at ? formatBeijingTime(row.last_seen_at) : '从未' }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="280" fixed="right">
          <template #default="{ row }">
            <el-button size="small" @click="viewDeviceDetail(row.id)">
              详情
            </el-button>
            <!-- 编辑按钮 — 需要 device:manage 权限 -->
            <el-button
              v-if="hasPerm('device:manage')"
              size="small"
              type="warning"
              @click="handleEditDevice(row)"
            >
              编辑
            </el-button>
            <!-- 控制按钮 — 需要 control:manual 权限 -->
            <el-button
              v-if="hasPerm('control:manual')"
              size="small"
              type="success"
              @click="controlDevice(row.id, 'on')"
              :disabled="row.lamp === 'ON'"
            >
              开灯
            </el-button>
            <el-button
              v-if="hasPerm('control:manual')"
              size="small"
              type="danger"
              @click="controlDevice(row.id, 'off')"
              :disabled="row.lamp === 'OFF'"
            >
              关灯
            </el-button>
            <!-- 无控制权限时显示提示 -->
            <el-tooltip v-if="!hasPerm('control:manual')" content="您没有控制路灯的权限" placement="top">
              <el-button size="small" type="info" disabled>控制</el-button>
            </el-tooltip>
          </template>
        </el-table-column>
      </el-table>
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
.device-list-page {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0 0 8px 0;
  font-size: 24px;
  color: #333;
}

.page-header p {
  margin: 0;
  color: #666;
}

.filter-card {
  margin-bottom: 20px;
}

.table-card {
  margin-bottom: 20px;
}

.lamp-on {
  color: #ff9800;
  font-weight: bold;
}

.lamp-off {
  color: #9e9e9e;
}
</style>
