<script setup>
/**
 * 参数配置页面（阈值 + 调光）
 *
 * 作用：设置路灯自动开关与调光参数
 * 包含：
 * - 设备选择
 * - 光照阈值设置（config:threshold 权限）
 * - 调光配置：手动亮度 + 照度-亮度曲线（config:dimming 权限）
 * - 保存/重置按钮
 *
 * 关键点：使用 Pinia Store 获取数据，实现数据同步
 */

import { ref, computed, onMounted, watch } from 'vue'
import { ElMessage } from 'element-plus'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// 亮度曲线编辑器组件（锚点编辑 + 对数/线性预览图）
import DimCurveEditor from '@/components/DimCurveEditor.vue'

// ============================================
// 1. 获取 Store
// ============================================
const deviceStore = useDeviceStore()

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
// 5. 调光配置（手动亮度 + 亮度曲线）
// ============================================
const brightness = ref(100)
const originalBrightness = ref(100)
const dimCurve = ref('')
const originalDimCurve = ref('')

// 曲线编辑器引用（读取其校验状态）
const curveEditorRef = ref(null)
const curveEditorValid = computed(() => curveEditorRef.value?.curveValid ?? false)

// ============================================
// 6. 保存状态
// ============================================
const saving = ref(false)          // 阈值
const savingBrightness = ref(false) // 亮度
const savingCurve = ref(false)     // 曲线

// ============================================
// 7. 检查是否有修改
// ============================================
const hasChanges = computed(() => {
  return threshold.value !== originalThreshold.value
})

const hasBrightnessChanges = computed(() => {
  return Math.round(brightness.value) !== originalBrightness.value
})

const hasCurveChanges = computed(() => {
  return dimCurve.value !== originalDimCurve.value
})

// ============================================
// 8. 获取阈值配置
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
// 9. 获取调光配置
// ============================================
const fetchDimming = async () => {
  if (!selectedDeviceId.value) return

  // 演示灯：本地默认调光配置（不调后端，避免 404），按设备分别记录
  const demoDev = deviceStore.deviceList.find(d => d.id === selectedDeviceId.value && d.demo)
  if (demoDev) {
    const demo = deviceStore.demoDimming[selectedDeviceId.value] || {}
    brightness.value = demo.brightness ?? 100
    originalBrightness.value = brightness.value
    dimCurve.value = demo.dim_curve ?? ''
    originalDimCurve.value = dimCurve.value
    return
  }

  // 从 Store 获取配置
  await deviceStore.fetchDimmingConfig(selectedDeviceId.value)

  brightness.value = deviceStore.dimmingConfig.brightness ?? 100
  originalBrightness.value = brightness.value
  dimCurve.value = deviceStore.dimmingConfig.dim_curve ?? ''
  originalDimCurve.value = dimCurve.value
}

// ============================================
// 10. 保存阈值配置
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
// 11. 保存手动亮度（设值后设备进手动模式并亮到该档，0=关灯）
// ============================================
const saveBrightness = async () => {
  if (!selectedDeviceId.value) {
    ElMessage.warning('请先选择设备')
    return
  }

  savingBrightness.value = true
  try {
    const result = await deviceStore.saveDimmingConfig(selectedDeviceId.value, {
      brightness: Math.round(brightness.value)
    })

    if (result.success) {
      ElMessage.success('手动亮度已保存并下发')
      originalBrightness.value = Math.round(brightness.value)
    } else {
      ElMessage.error(result.message)
    }
  } catch (error) {
    console.log('保存亮度失败：', error)
    ElMessage.error('保存失败')
  } finally {
    savingBrightness.value = false
  }
}

// ============================================
// 12. 保存亮度曲线（空串 = 停用曲线）
// ============================================
const saveCurve = async () => {
  if (!selectedDeviceId.value) {
    ElMessage.warning('请先选择设备')
    return
  }
  if (!curveEditorValid.value) {
    ElMessage.error('曲线存在错误，请先修正锚点')
    return
  }

  savingCurve.value = true
  try {
    const result = await deviceStore.saveDimmingConfig(selectedDeviceId.value, {
      dim_curve: dimCurve.value
    })

    if (result.success) {
      ElMessage.success('亮度曲线已保存并下发')
      originalDimCurve.value = dimCurve.value
    } else {
      ElMessage.error(result.message)
    }
  } catch (error) {
    console.log('保存曲线失败：', error)
    ElMessage.error('保存失败')
  } finally {
    savingCurve.value = false
  }
}

// ============================================
// 13. 重置配置
// ============================================
const resetThreshold = () => {
  threshold.value = originalThreshold.value
  ElMessage.info('已重置')
}

const resetDimming = () => {
  brightness.value = originalBrightness.value
  dimCurve.value = originalDimCurve.value
  ElMessage.info('已重置')
}

// ============================================
// 14. 监听设备选择变化
// ============================================
watch(selectedDeviceId, () => {
  if (selectedDeviceId.value) {
    fetchThreshold()
    fetchDimming()
  }
})

// ============================================
// 15. 组件挂载时获取设备列表
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
      <h2>参数配置</h2>
      <p>设置路灯自动开关阈值、手动亮度与照度-亮度曲线</p>
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
    <!-- 阈值配置表单（config:threshold 权限） -->
    <!-- ============================================ -->
    <el-card
      v-if="selectedDeviceId && hasPerm('config:threshold')"
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
    <!-- 调光配置（config:dimming 权限） -->
    <!-- ============================================ -->
    <el-card
      v-if="selectedDeviceId && hasPerm('config:dimming')"
      class="config-card"
    >
      <template #header>
        <div class="card-header">
          <h3>调光配置</h3>
          <el-tag type="info">
            当前设备：{{ selectedDeviceId }}
          </el-tag>
        </div>
      </template>

      <el-form label-width="120px">
        <!-- 手动亮度 -->
        <el-form-item label="手动亮度">
          <div class="brightness-row">
            <el-slider
              v-model="brightness"
              :min="0"
              :max="100"
              :step="1"
              show-input
              class="brightness-slider"
            />
            <el-button
              type="primary"
              :loading="savingBrightness"
              :disabled="!hasBrightnessChanges"
              @click="saveBrightness"
            >
              保存亮度
            </el-button>
          </div>
        </el-form-item>
      </el-form>

      <el-divider content-position="left">照度-亮度曲线</el-divider>

      <!-- 亮度曲线编辑器 -->
      <DimCurveEditor ref="curveEditorRef" v-model="dimCurve" />

      <!-- 曲线操作按钮 -->
      <div class="curve-actions">
        <el-button
          type="primary"
          :loading="savingCurve"
          :disabled="!hasCurveChanges || !curveEditorValid"
          @click="saveCurve"
        >
          保存曲线
        </el-button>
        <el-button :disabled="!hasCurveChanges && !hasBrightnessChanges" @click="resetDimming">
          重置
        </el-button>
      </div>
    </el-card>

    <!-- ============================================ -->
    <!-- 无调光权限提示 -->
    <!-- ============================================ -->
    <el-card
      v-if="selectedDeviceId && !hasPerm('config:dimming')"
      class="config-card"
    >
      <template #header>
        <h3>调光配置</h3>
      </template>
      <el-alert
        title="您没有调光配置权限"
        type="info"
        description="请联系管理员获取 config:dimming 权限"
        show-icon
        :closable="false"
      />
    </el-card>

    <!-- ============================================ -->
    <!-- 未选择设备提示 -->
    <!-- ============================================ -->
    <el-card v-if="!selectedDeviceId" class="info-card">
      <div class="no-device">
        <p>👈 请先选择要配置的设备</p>
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

.brightness-row {
  display: flex;
  align-items: center;
  gap: 16px;
  width: 100%;
}

.brightness-slider {
  flex: 1;
}

.curve-actions {
  margin-top: 16px;
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
</style>
