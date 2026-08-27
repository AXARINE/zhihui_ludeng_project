<script setup>
/**
 * 告警列表页面
 *
 * 作用：展示所有告警信息，支持筛选和处理
 * 包含：
 * - 告警统计
 * - 筛选条件
 * - 告警表格
 * - 处理按钮
 *
 * 关键点：使用 Pinia Store 获取数据，实现数据同步
 */

import { ref, computed, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import { useDeviceStore } from '@/stores/deviceStore'
import { resolveAlarm, unresolveAlarm } from '@/api/device'
import { formatBeijingTime } from '@/utils/time'

// ============================================
// 1. 获取 Store
// ============================================
const deviceStore = useDeviceStore()

// ============================================
// 2. 筛选条件
// ============================================
const filters = ref({
  status: ''       // 告警状态
})

// ============================================
// 3. 筛选后的告警列表
// ============================================
const filteredAlarmList = computed(() => {
  let list = deviceStore.alarmList

  // 按状态筛选
  if (filters.value.status === 'pending') {
    list = list.filter(alarm => alarm.resolved_at === null)
  } else if (filters.value.status === 'resolved') {
    list = list.filter(alarm => alarm.resolved_at !== null)
  }

  return list
})

// ============================================
// 4. 告警统计
// ============================================
const alarmStats = computed(() => {
  const list = deviceStore.alarmList
  return {
    total: list.length,
    pending: list.filter(a => a.resolved_at === null).length,
    resolved: list.filter(a => a.resolved_at !== null).length
  }
})

// ============================================
// 5. 获取状态文本
// ============================================
const getStatusText = (alarm) => {
  return alarm.resolved_at === null ? '待处理' : '已解决'
}

// ============================================
// 6. 获取状态标签类型
// ============================================
const getStatusType = (alarm) => {
  return alarm.resolved_at === null ? 'danger' : 'success'
}

// ============================================
// 7. 告警处理
// ============================================
async function handleResolve(id) {
  try {
    await resolveAlarm(id)
    ElMessage.success('告警已标记为已处理')
    deviceStore.fetchAlarmList()
  } catch (e) {
    ElMessage.error('处理失败：' + (e?.response?.data || e.message))
  }
}

async function handleUnresolve(id) {
  try {
    await unresolveAlarm(id)
    ElMessage.success('告警已恢复为未处理')
    deviceStore.fetchAlarmList()
  } catch (e) {
    ElMessage.error('操作失败：' + (e?.response?.data || e.message))
  }
}

// ============================================
// 8. 组件挂载时获取数据
// ============================================
onMounted(() => {
  console.log('AlarmList 组件已挂载')
  deviceStore.fetchAlarmList()
})
</script>

<template>
  <div class="alarm-list-page">
    <!-- ============================================ -->
    <!-- 页面标题 -->
    <!-- ============================================ -->
    <div class="page-header">
      <h2>告警列表</h2>
      <p>查看和处理设备告警信息</p>
    </div>

    <!-- ============================================ -->
    <!-- 告警统计 -->
    <!-- ============================================ -->
    <div class="alarm-stats">
      <el-card class="stat-card">
        <div class="stat-value">{{ alarmStats.total }}</div>
        <div class="stat-label">告警总数</div>
      </el-card>
      <el-card class="stat-card pending">
        <div class="stat-value">{{ alarmStats.pending }}</div>
        <div class="stat-label">待处理</div>
      </el-card>
      <el-card class="stat-card resolved">
        <div class="stat-value">{{ alarmStats.resolved }}</div>
        <div class="stat-label">已解决</div>
      </el-card>
    </div>

    <!-- ============================================ -->
    <!-- 筛选条件 -->
    <!-- ============================================ -->
    <el-card class="filter-card">
      <el-form :inline="true" :model="filters">
        <el-form-item label="告警状态">
          <el-select v-model="filters.status" placeholder="全部" clearable>
            <el-option label="待处理" value="pending" />
            <el-option label="已解决" value="resolved" />
          </el-select>
        </el-form-item>

        <el-form-item>
          <el-button type="primary" @click="deviceStore.fetchAlarmList()">
            刷新
          </el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- ============================================ -->
    <!-- 告警表格 -->
    <!-- ============================================ -->
    <el-card class="table-card">
      <el-table
        :data="filteredAlarmList"
        v-loading="deviceStore.loading"
        stripe
        style="width: 100%"
      >
        <!-- 告警ID -->
        <el-table-column prop="id" label="告警ID" width="80" />

        <!-- 设备ID -->
        <el-table-column prop="device_id" label="设备ID" width="120" />

        <!-- 告警信息 -->
        <el-table-column prop="message" label="告警信息" min-width="200" />

        <!-- 状态 -->
        <el-table-column label="状态" width="100">
          <template #default="{ row }">
            <el-tag :type="getStatusType(row)">
              {{ getStatusText(row) }}
            </el-tag>
          </template>
        </el-table-column>

        <!-- 创建时间 -->
        <el-table-column label="创建时间" width="180">
          <template #default="{ row }">
            {{ formatBeijingTime(row.created_at) }}
          </template>
        </el-table-column>

        <!-- 解决时间 -->
        <el-table-column label="解决时间" width="180">
          <template #default="{ row }">
            <span v-if="row.resolved_at">{{ formatBeijingTime(row.resolved_at) }}</span>
            <span v-else class="pending-text">-</span>
          </template>
        </el-table-column>

        <!-- 操作 -->
        <el-table-column label="操作" width="140">
          <template #default="{ row }">
            <el-button
              v-if="!row.resolved_at"
              size="small"
              type="success"
              @click="handleResolve(row.id)"
            >
              标记已处理
            </el-button>
            <el-button
              v-else
              size="small"
              type="warning"
              @click="handleUnresolve(row.id)"
            >
              恢复未处理
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<style scoped>
.alarm-list-page {
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

.alarm-stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  margin-bottom: 20px;
}

.stat-card {
  text-align: center;
}

.stat-value {
  font-size: 30px;
  font-weight: 600;
  font-family: var(--font-mono);
  color: #1f1c19;
}

.stat-label {
  font-size: 13px;
  color: #8a837b;
  margin-top: 6px;
}

.stat-card.pending .stat-value {
  color: #be4b40;
}

.stat-card.resolved .stat-value {
  color: #5f8f5a;
}

.filter-card {
  margin-bottom: 20px;
}

/* 筛选控件：默认宽度太窄，选中项显示不下 */
.filter-card .el-select {
  width: 180px;
}

.filter-card .el-form-item {
  margin-bottom: 0;
}

.table-card {
  margin-bottom: 20px;
}

.pending-text {
  color: #b4ada3;
}
</style>
