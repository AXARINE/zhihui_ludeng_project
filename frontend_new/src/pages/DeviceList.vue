<script setup>
/**
 * 设备列表页面
 *
 * 作用：展示所有设备，支持筛选、排序、批量操作
 * 包含：
 * - 筛选条件
 * - 设备表格
 * - 批量操作按钮
 *
 * 关键点：使用 Pinia Store 获取数据，实现数据同步
 */

import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// ============================================
// 1. 获取 Store 和路由
// ============================================
const deviceStore = useDeviceStore()
const router = useRouter()

// ============================================
// 2. 筛选条件
// ============================================
const filters = ref({
  status: '',       // 设备状态
  keyword: ''       // 搜索关键词
})

// ============================================
// 3. 筛选后的设备列表
// ============================================
const filteredDeviceList = computed(() => {
  let list = deviceStore.deviceList

  // 按状态筛选
  if (filters.value.status) {
    list = list.filter(device => device.status === filters.value.status)
  }

  // 按关键词搜索
  if (filters.value.keyword) {
    const keyword = filters.value.keyword.toLowerCase()
    list = list.filter(device =>
      device.name.toLowerCase().includes(keyword) ||
      device.id.toLowerCase().includes(keyword)
    )
  }

  return list
})

// ============================================
// 4. 查看设备详情
// ============================================
const viewDeviceDetail = (deviceId) => {
  router.push(`/device/${deviceId}`)
}

// ============================================
// 5. 控制设备
// ============================================
const controlDevice = async (deviceId, action) => {
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
// 6. 获取状态标签类型
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
// 7. 获取状态文本
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
// 8. 组件挂载时获取数据
// ============================================
onMounted(() => {
  console.log('DeviceList 组件已挂载')
  // 从 Store 获取设备列表
  deviceStore.fetchDeviceList()
})
</script>

<template>
  <div class="device-list-page">
    <!-- ============================================ -->
    <!-- 页面标题 -->
    <!-- ============================================ -->
    <div class="page-header">
      <h2>设备列表</h2>
      <p>管理和监控所有路灯设备</p>
    </div>

    <!-- ============================================ -->
    <!-- 筛选条件 -->
    <!-- ============================================ -->
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
          <el-input
            v-model="filters.keyword"
            placeholder="设备名称或ID"
            clearable
          />
        </el-form-item>

        <el-form-item>
          <el-button type="primary" @click="deviceStore.fetchDeviceList()">
            刷新
          </el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- ============================================ -->
    <!-- 设备表格 -->
    <!-- ============================================ -->
    <el-card class="table-card">
      <el-table
        :data="filteredDeviceList"
        v-loading="deviceStore.loading"
        stripe
        style="width: 100%"
      >
        <!-- 设备ID -->
        <el-table-column prop="id" label="设备ID" width="100" />

        <!-- 设备名称 -->
        <el-table-column prop="name" label="设备名称" width="150" />

        <!-- 状态 -->
        <el-table-column label="状态" width="100">
          <template #default="{ row }">
            <el-tag :type="getStatusType(row.status)">
              {{ getStatusText(row.status) }}
            </el-tag>
          </template>
        </el-table-column>

        <!-- 灯状态 -->
        <el-table-column label="灯状态" width="100">
          <template #default="{ row }">
            <span :class="row.lamp === 'ON' ? 'lamp-on' : 'lamp-off'">
              {{ row.lamp === 'ON' ? '💡 开' : '🌑 关' }}
            </span>
          </template>
        </el-table-column>

        <!-- 工作模式 -->
        <el-table-column label="模式" width="100">
          <template #default="{ row }">
            <span>{{ row.mode === 'AUTO' ? '🔄 自动' : '✋ 手动' }}</span>
          </template>
        </el-table-column>

        <!-- 最后在线 -->
        <el-table-column prop="last_seen_at" label="最后在线" width="180" />

        <!-- 操作 -->
        <el-table-column label="操作" width="250">
          <template #default="{ row }">
            <el-button size="small" @click="viewDeviceDetail(row.id)">
              详情
            </el-button>
            <el-button
              size="small"
              type="success"
              @click="controlDevice(row.id, 'on')"
              :disabled="row.lamp === 'ON'"
            >
              开灯
            </el-button>
            <el-button
              size="small"
              type="danger"
              @click="controlDevice(row.id, 'off')"
              :disabled="row.lamp === 'OFF'"
            >
              关灯
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
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
