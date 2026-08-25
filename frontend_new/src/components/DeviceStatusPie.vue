<script setup>
/**
 * 设备状态饼图组件
 *
 * 作用：展示设备状态的分布情况
 * 使用 ECharts 绘制饼图
 *
 * 关键点：从 Pinia Store 获取数据，实时更新
 */

import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import * as echarts from 'echarts'

// 导入 Pinia Store
import { useDeviceStore } from '@/stores/deviceStore'

// ============================================
// 1. 获取 Store
// ============================================
const deviceStore = useDeviceStore()

// ============================================
// 2. 图表容器引用
// ============================================
const chartRef = ref(null)

// ============================================
// 3. 图表实例
// ============================================
let chartInstance = null

// ============================================
// 4. 计算属性：从 Store 获取设备状态数据
// ============================================
const statusData = computed(() => {
  const data = [
    {
      value: deviceStore.onlineCount,
      name: '在线',
      itemStyle: { color: '#67c23a' }
    },
    {
      value: deviceStore.offlineCount,
      name: '离线',
      itemStyle: { color: '#909399' }
    },
    {
      value: deviceStore.faultCount,
      name: '故障',
      itemStyle: { color: '#f56c6c' }
    }
  ]
  console.log('状态数据更新：', data)
  return data
})

// ============================================
// 5. 初始化图表
// ============================================
const initChart = () => {
  const chartDom = chartRef.value
  if (!chartDom) return

  chartInstance = echarts.init(chartDom)
  updateChart()
}

// ============================================
// 6. 更新图表配置
// ============================================
const updateChart = () => {
  if (!chartInstance) return

  // 【重要】使用 statusData.value 获取最新数据
  const currentData = statusData.value

  const option = {
    title: {
      text: '设备状态分布',
      left: 'center'
    },

    tooltip: {
      trigger: 'item',
      formatter: '{a} <br/>{b}: {c} ({d}%)'
    },

    legend: {
      orient: 'vertical',
      left: 'left',
      top: 'middle'
    },

    series: [
      {
        name: '设备状态',
        type: 'pie',
        radius: ['40%', '70%'],  // 环形图
        avoidLabelOverlap: false,
        label: {
          show: true,
          formatter: '{b}: {c}台'
        },
        emphasis: {
          label: {
            show: true,
            fontSize: '16',
            fontWeight: 'bold'
          }
        },
        data: currentData
      }
    ]
  }

  // 【重要】使用 setOption 的 notMerge 参数，完全替换旧数据
  chartInstance.setOption(option, true)
}

// ============================================
// 7. 监听数据变化，自动更新图表
// ============================================
watch(
  statusData,
  (newData, oldData) => {
    console.log('设备状态数据变化，更新图表')
    console.log('旧数据：', oldData)
    console.log('新数据：', newData)
    updateChart()
  },
  { deep: true }
)

// ============================================
// 8. 监听窗口大小变化
// ============================================
const handleResize = () => {
  if (chartInstance) {
    chartInstance.resize()
  }
}

// ============================================
// 9. 组件挂载时初始化图表
// ============================================
onMounted(() => {
  console.log('DeviceStatusPie 组件已挂载')

  // 获取设备列表（如果还没有获取的话）
  if (deviceStore.deviceList.length === 0) {
    deviceStore.fetchDeviceList()
  }

  // 【重要】延迟初始化图表，确保 DOM 已渲染
  setTimeout(() => {
    initChart()
  }, 100)

  window.addEventListener('resize', handleResize)
})

// ============================================
// 10. 组件卸载时销毁图表
// ============================================
onUnmounted(() => {
  if (chartInstance) {
    chartInstance.dispose()
    chartInstance = null
  }
  window.removeEventListener('resize', handleResize)
})
</script>

<template>
  <div class="device-status-pie">
    <!-- 图表容器 -->
    <div ref="chartRef" class="chart-container"></div>
  </div>
</template>

<style scoped>
.device-status-pie {
  width: 100%;
}

.chart-container {
  width: 100%;
  height: 300px;
}
</style>
