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
import { gcj02ToWgs84 } from '@/utils/coord'

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
  // 演示灯已由 deviceStore 统一合并（首页大屏/设备列表/阈值等全局可见）
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
  location: '',
  latitude: '',      // 字符串方便输入，空串 = 清除/未填
  longitude: ''
})
// 输入的坐标是否来自高德拾取器（GCJ-02）；保存时自动转 WGS84 入库
const isGcj02Input = ref(false)

function handleEditDevice(device) {
  // 演示灯为虚拟设备，不落库，不可编辑
  if (device.demo) {
    ElMessage.info('演示灯为虚拟设备，不可编辑（仅用于功能演示）')
    return
  }
  editForm.value = {
    id: device.id,
    name: device.name || '',
    location: device.location || '',
    latitude: device.latitude != null ? String(device.latitude) : '',
    longitude: device.longitude != null ? String(device.longitude) : ''
  }
  isGcj02Input.value = false
  editVisible.value = true
}

async function handleSaveDevice() {
  editLoading.value = true
  try {
    const data = {}
    if (editForm.value.name.trim()) data.name = editForm.value.name.trim()
    if (editForm.value.location.trim()) data.location = editForm.value.location.trim()

    // 坐标：两字段成对填写（都空 = 不改）
    const latStr = editForm.value.latitude.trim()
    const lngStr = editForm.value.longitude.trim()
    if (latStr || lngStr) {
      if (!latStr || !lngStr) {
        ElMessage.warning('纬度和经度需成对填写')
        return
      }
      let lat = parseFloat(latStr)
      let lng = parseFloat(lngStr)
      if (isNaN(lat) || isNaN(lng)) {
        ElMessage.warning('坐标格式不正确，请输入数字')
        return
      }
      // 高德拾取器给出的是 GCJ-02，先转 WGS84（后端约定统一存 WGS84）
      if (isGcj02Input.value) {
        const wgs = gcj02ToWgs84(lng, lat)
        lng = wgs.lng
        lat = wgs.lat
      }
      if (lat < -90 || lat > 90 || lng < -180 || lng > 180) {
        ElMessage.warning('坐标超出范围（纬度 -90~90，经度 -180~180）')
        return
      }
      data.latitude = lat
      data.longitude = lng
    }

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
    <el-dialog v-model="editVisible" title="编辑设备信息" width="480px">
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
        <el-form-item label="纬度">
          <el-input v-model="editForm.latitude" placeholder="如 31.0245（与经度成对填写）" />
        </el-form-item>
        <el-form-item label="经度">
          <el-input v-model="editForm.longitude" placeholder="如 121.4372（与纬度成对填写）" />
        </el-form-item>
        <el-form-item>
          <el-checkbox v-model="isGcj02Input">
            坐标来自高德拾取器（GCJ-02，保存时自动转 WGS84）
          </el-checkbox>
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
  padding: 24px;
}

.page-header {
  margin-bottom: 20px;
  padding-bottom: 14px;
  border-bottom: 1px solid #efebe3;
}

.page-header h2 {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 0 8px 0;
  font-size: 24px;
  font-family: var(--font-serif);
  font-weight: 600;
  color: #1f1c19;
}

.page-header h2::before {
  content: '';
  width: 4px;
  height: 0.95em;
  background: #c96a4a;
  border-radius: 2px;
}

.page-header p {
  margin: 0;
  color: #8a837b;
}

.filter-card {
  margin-bottom: 20px;
}

.table-card {
  margin-bottom: 20px;
}

.lamp-on {
  color: #c08340;
  font-weight: 600;
}

.lamp-off {
  color: #a8a29c;
}

/* 筛选控件：默认宽度太窄，选中项显示不下 */
.filter-card .el-select {
  width: 180px;
}

.filter-card .el-input {
  width: 220px;
}

/* 筛选栏单行紧凑 */
.filter-card .el-form-item {
  margin-bottom: 0;
}
</style>
