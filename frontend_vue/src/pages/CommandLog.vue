<script setup>
import { ref, onMounted } from 'vue'
import { getCommands } from '@/api/device'

const commands = ref([])
const loading = ref(false)
const filter = ref({ device_id: '' })

async function loadCommands() {
  loading.value = true
  try {
    const params = { limit: 100 }
    if (filter.value.device_id) params.device_id = filter.value.device_id
    commands.value = await getCommands(params) || []
  } catch (e) {
    console.error('加载审计日志失败：', e)
  } finally {
    loading.value = false
  }
}

function getSourceType(source) {
  return source === 'auto' ? 'warning' : 'primary'
}

function getSourceText(source) {
  return source === 'auto' ? '自动' : '手动'
}

function getStatusType(status) {
  const map = { sent: 'info', success: 'success', failed: 'danger' }
  return map[status] || 'info'
}

function getStatusText(status) {
  const map = { sent: '已发送', success: '成功', failed: '失败' }
  return map[status] || status
}

onMounted(loadCommands)
</script>

<template>
  <div class="command-log-page">
    <div class="page-header">
      <h2>控制指令审计日志</h2>
      <p>查看所有路灯控制指令的执行记录</p>
    </div>

    <el-card class="filter-card">
      <el-form :inline="true" :model="filter">
        <el-form-item label="设备ID">
          <el-input v-model="filter.device_id" placeholder="按设备ID筛选" clearable />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="loadCommands">查询</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <el-card>
      <el-table :data="commands" v-loading="loading" stripe style="width: 100%">
        <el-table-column prop="id" label="ID" width="70" />
        <el-table-column prop="device_id" label="设备ID" width="160" />
        <el-table-column prop="command_type" label="指令" width="80" />
        <el-table-column label="来源" width="80">
          <template #default="{ row }">
            <el-tag :type="getSourceType(row.source)" size="small">
              {{ getSourceText(row.source) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="80">
          <template #default="{ row }">
            <el-tag :type="getStatusType(row.status)" size="small">
              {{ getStatusText(row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="message" label="备注" min-width="200" />
        <el-table-column prop="created_at" label="时间" width="180" />
      </el-table>
    </el-card>
  </div>
</template>

<style scoped>
.command-log-page { padding: 20px; }
.page-header { margin-bottom: 20px; }
.page-header h2 { margin: 0 0 8px; font-size: 24px; color: #333; }
.page-header p { margin: 0; color: #666; }
.filter-card { margin-bottom: 20px; }
</style>
