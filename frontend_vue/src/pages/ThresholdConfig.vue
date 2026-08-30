<script setup>
/**
 * 阈值配置页面
 *
 * 作用：设置路灯自动开关的参数
 * 包含：
 * - 设备选择
 * - 光照阈值设置
 * - 保存/重置按钮
 *
 * 关键点：使用 Pinia Store 获取数据，实现数据同步
 */

import { ref, computed, onMounted, watch } from 'vue'
import { ElMessage } from 'element-plus'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// ============================================
// 1. 获取 Store
// ============================================
const deviceStore = useDeviceStore()

// ============================================
// 2. 当前选择的设备ID
// ============================================
const selectedDeviceId = ref('')

// ============================================
// 3. 阈值配置
// ============================================
const threshold = ref(40.0)

// ============================================
// 4. 原始阈值（用于重置）
// ============================================
const originalThreshold = ref(40.0)

// ============================================
// 5. 保存状态
// ============================================
const saving = ref(false)

// ============================================
// 6. 检查是否有修改
// ============================================
const hasChanges = computed(() => {
  return threshold.value !== originalThreshold.value
})

// ============================================
// 7. 获取阈值配置
// ============================================
const fetchThreshold = async () => {
  if (!selectedDeviceId.value) return

  // 演示灯：本地返回阈值（不调后端，避免 404）
  const demoDev = deviceStore.deviceList.find(d => d.id === selectedDeviceId.value && d.demo)
  if (demoDev) {
    threshold.value = 90
    originalThreshold.value = 90
    return
  }

  // 从 Store 获取配置
  await deviceStore.fetchThresholdConfig(selectedDeviceId.value)

  // 复制到本地（避免直接修改 Store）
  threshold.value = deviceStore.thresholdConfig.threshold || 40.0
  originalThreshold.value = threshold.value
}

// ============================================
// 8. 保存阈值配置
// ============================================
const saveThreshold = async () => {
  if (!selectedDeviceId.value) {
    ElMessage.warning('请先选择设备')
    return
  }

  saving.value = true
  try {
    // 调用 Store 的保存方法
    const result = await deviceStore.saveThresholdConfig(
      selectedDeviceId.value,
      threshold.value
    )

    if (result.success) {
      ElMessage.success('保存成功')
      originalThreshold.value = threshold.value
    } else {
      ElMessage.error(result.message)
    }
  } catch (error) {
    console.log('保存配置失败：', error)
    ElMessage.error('保存失败')
  } finally {
    saving.value = false
  }
}

// ============================================
// 9. 重置配置
// ============================================
const resetThreshold = () => {
  threshold.value = originalThreshold.value
  ElMessage.info('已重置')
}

// ============================================
// 10. 监听设备选择变化
// ============================================
watch(selectedDeviceId, () => {
  if (selectedDeviceId.value) {
    fetchThreshold()
  }
})

// ============================================
// 11. 组件挂载时获取设备列表
// ============================================
onMounted(() => {
  console.log('ThresholdConfig 组件已挂载')
  // 获取设备列表
  deviceStore.fetchDeviceList()
})
</script>

<template>
  <div class="threshold-config-page">
    <!-- ============================================ -->
    <!-- 页面标题 -->
    <!-- ============================================ -->
    <div class="page-header">
      <h2>阈值配置</h2>
      <p>设置路灯自动开关的光照阈值</p>
    </div>

    <!-- ============================================ -->
    <!-- 设备选择 -->
    <!-- ============================================ -->
    <el-card class="device-select-card">
      <template #header>
        <h3>选择设备</h3>
      </template>

      <el-select
        v-model="selectedDeviceId"
        placeholder="请选择要配置的设备"
        style="width: 100%"
      >
        <el-option
          v-for="device in deviceStore.deviceList"
          :key="device.id"
          :label="device.name"
          :value="device.id"
        />
      </el-select>
    </el-card>

    <!-- ============================================ -->
    <!-- 阈值配置表单 -->
    <!-- ============================================ -->
    <el-card
      v-if="selectedDeviceId"
      class="config-card"
      v-loading="deviceStore.loading"
    >
      <template #header>
        <div class="card-header">
          <h3>光照阈值配置</h3>
          <el-tag type="info">
            当前设备：{{ selectedDeviceId }}
          </el-tag>
        </div>
      </template>

      <el-form label-width="120px">
        <!-- 光照阈值 -->
        <el-form-item label="光照阈值">
          <el-slider
            v-model="threshold"
            :min="0"
            :max="300"
            :step="1"
            show-input
          />
          <div class="form-tip">
            光照强度低于此值时自动开灯（单位：lux）
          </div>
        </el-form-item>

        <!-- 操作按钮 -->
        <el-form-item>
          <el-button
            type="primary"
            @click="saveThreshold"
            :loading="saving"
            :disabled="!hasChanges"
          >
            保存配置
          </el-button>
          <el-button @click="resetThreshold">
            重置
          </el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- ============================================ -->
    <!-- 未选择设备提示 -->
    <!-- ============================================ -->
    <el-card v-else class="info-card">
      <div class="no-device">
        <p>👈 请先选择要配置的设备</p>
      </div>
    </el-card>

    <!-- ============================================ -->
    <!-- 配置说明 -->
    <!-- ============================================ -->
    <el-card class="info-card">
      <template #header>
        <h3>配置说明</h3>
      </template>

      <div class="info-content">
        <h4>光照阈值</h4>
        <p>
          光照阈值是控制路灯自动开关的关键参数。当环境光照强度低于设定的阈值时，
          系统会自动开启路灯；当光照强度高于阈值时，系统会自动关闭路灯。
        </p>

        <h4>自动模式</h4>
        <p>
          在设备卡片中，点击"自动"按钮可以切换设备的工作模式。
          自动模式下，设备会根据光照阈值自动控制路灯开关。
        </p>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.threshold-config-page {
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

.device-select-card {
  margin-bottom: 20px;
}

.config-card {
  margin-bottom: 20px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.card-header h3 {
  margin: 0;
  font-size: 18px;
}

.form-tip {
  font-size: 12px;
  color: #b4ada3;
  margin-top: 4px;
}

.info-card {
  margin-bottom: 20px;
}

.no-device {
  text-align: center;
  padding: 40px;
  color: #8a837b;
  font-size: 16px;
}

.info-content h4 {
  margin: 20px 0 10px 0;
  color: #1f1c19;
}

.info-content h4:first-child {
  margin-top: 0;
}

.info-content p {
  margin: 0 0 15px 0;
  color: #57504a;
  line-height: 1.7;
}
</style>
