<script setup>
/**
 * 光照趋势图表组件
 *
 * 作用：展示光照强度的历史变化趋势
 * 支持 4 个时间周期：1小时、24小时、7天、30天
 * 使用 ECharts 绘制折线图，数据来自后端真实 API
 */

import { ref, onMounted, onUnmounted, watch } from 'vue'
import * as echarts from 'echarts'
import { getLuxHistory } from '@/api/device'
import { useDeviceStore } from '@/stores/deviceStore'

// 图表容器引用
const chartRef = ref(null)

// 图表实例
let chartInstance = null

// 设备 store
const deviceStore = useDeviceStore()

// 当前选中的时间周期
const activePeriod = ref('24h')

// 加载状态
const loading = ref(false)

// ============================================
// 时间周期配置
// ============================================
const periods = [
  { key: '1h',  label: '1小时',  ms: 60 * 60 * 1000,              buckets: 60,  format: 'HH:mm' },
  { key: '24h', label: '24小时', ms: 24 * 60 * 60 * 1000,          buckets: 144, format: 'HH:mm' },
  { key: '7d',  label: '7天',    ms: 7 * 24 * 60 * 60 * 1000,      buckets: 168, format: 'MM-DD HH:mm' },
  { key: '30d', label: '30天',   ms: 30 * 24 * 60 * 60 * 1000,     buckets: 180, format: 'MM-DD' }
]

// ============================================
// 格式化时间
// ============================================
function formatTime(date, pattern) {
  const MM = String(date.getMonth() + 1).padStart(2, '0')
  const DD = String(date.getDate()).padStart(2, '0')
  const HH = String(date.getHours()).padStart(2, '0')
  const mm = String(date.getMinutes()).padStart(2, '0')
  return pattern
    .replace('MM', MM)
    .replace('DD', DD)
    .replace('HH', HH)
    .replace('mm', mm)
}

// ============================================
// 数据聚合：把原始数据按时间桶取平均值
// ============================================
function aggregateData(records, period) {
  if (!records || records.length === 0) return { times: [], values: [] }

  const now = Date.now()
  const from = now - period.ms
  const bucketSize = period.ms / period.buckets

  // 创建桶
  const buckets = new Array(period.buckets).fill(null).map(() => ({
    sum: 0,
    count: 0
  }))

  // 把每条数据放进对应的桶
  records.forEach(record => {
    const t = new Date(record.created_at).getTime()
    const idx = Math.floor((t - from) / bucketSize)
    if (idx >= 0 && idx < period.buckets) {
      buckets[idx].sum += record.lux
      buckets[idx].count += 1
    }
  })

  // 计算每个桶的平均值
  const times = []
  const values = []
  buckets.forEach((bucket, i) => {
    if (bucket.count > 0) {
      const t = new Date(from + i * bucketSize + bucketSize / 2)
      times.push(formatTime(t, period.format))
      values.push(Math.round(bucket.sum / bucket.count * 10) / 10)
    }
  })

  return { times, values }
}

// ============================================
// 获取当前设备 ID（优先第一个在线设备——离线设备往往无光照数据，图表会空）
// ============================================
function getDeviceId() {
  const devices = deviceStore.deviceList
  if (devices && devices.length > 0) {
    const online = devices.find(d => d.status === 'ONLINE')
    return (online || devices[0]).id
  }
  return null
}

// ============================================
// 加载数据并更新图表
// ============================================
async function loadData() {
  const deviceId = getDeviceId()
  if (!deviceId) {
    console.log('没有设备，跳过加载')
    return
  }

  const period = periods.find(p => p.key === activePeriod.value)
  if (!period) return

  loading.value = true
  try {
    const now = new Date()
    const from = new Date(now.getTime() - period.ms)

    const res = await getLuxHistory(deviceId, {
      from: from.toISOString(),
      to: now.toISOString()
    })

    // res 是数组：[{ id, device_id, lux, created_at }, ...]
    const records = Array.isArray(res) ? res : []
    const { times, values } = aggregateData(records, period)

    updateChart(times, values, period)
  } catch (err) {
    console.error('加载光照数据失败：', err)
  } finally {
    loading.value = false
  }
}

// ============================================
// 初始化图表
// ============================================
const initChart = () => {
  const chartDom = chartRef.value
  if (!chartDom) return

  chartInstance = echarts.init(chartDom)
  loadData()
}

// ============================================
// 更新图表配置
// ============================================
const updateChart = (times, values, period) => {
  if (!chartInstance) return

  // 获取当前阈值（从 store 的 thresholdConfig 获取，默认 120）
  const threshold = deviceStore.thresholdConfig?.threshold || 120

  const option = {
    // 页面每 5 秒轮询会触发整图重绘，关闭动画防止参考线/折线反复播入场动画
    animation: false,

    // 提示框：深夜面板底 + 冷调描边 + 重投影
    tooltip: {
      trigger: 'axis',
      backgroundColor: '#162032',
      borderColor: 'rgba(148, 168, 200, 0.2)',
      borderWidth: 1,
      padding: [8, 12],
      textStyle: { color: '#eae4d3', fontSize: 12 },
      extraCssText: 'box-shadow: 0 4px 16px rgba(0,0,0,0.5); border-radius: 8px;',
      formatter: function(params) {
        const p = params[0]
        return `${p.axisValue}<br/>光照强度：<b>${p.value}</b> lux`
      }
    },

    xAxis: {
      type: 'category',
      data: times,
      boundaryGap: false,
      axisLabel: {
        rotate: times.length > 30 ? 45 : 0,
        fontSize: 10,
        color: '#8e9bb0'
      },
      axisLine: { show: false },
      axisTick: { show: false }
    },

    yAxis: {
      type: 'value',
      name: 'lux',
      nameTextStyle: { color: '#8e9bb0', fontSize: 11 },
      axisLabel: { color: '#8e9bb0', fontSize: 11 },
      splitLine: { lineStyle: { color: 'rgba(148, 168, 200, 0.08)', type: 'dashed' } },
      min: 0
    },

    series: [
      {
        name: '光照强度',
        type: 'line',
        data: values,
        smooth: true,
        showSymbol: values.length < 50,
        symbol: 'circle',
        symbolSize: 5,
        areaStyle: {
          color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: 'rgba(232, 163, 61, 0.25)' },
            { offset: 1, color: 'rgba(232, 163, 61, 0.02)' }
          ])
        },
        lineStyle: { color: '#e8a33d', width: 2.5 },
        itemStyle: { color: '#e8a33d', borderColor: '#131c2d', borderWidth: 1.5 },
        // 阈值参考线
        markLine: {
          silent: true,
          symbol: 'none',
          data: [
            {
              yAxis: threshold,
              label: { formatter: `阈值 ${threshold}`, position: 'insideEndTop', color: '#f2c979', fontSize: 11 },
              lineStyle: { color: '#e8a33d', type: 'dashed', width: 1 }
            }
          ]
        }
      }
    ],

    grid: {
      left: '2%',
      right: '3%',
      bottom: '2%',
      top: 28,
      containLabel: true
    },

    // 数据为空时提示
    graphic: values.length === 0 ? [{
      type: 'text',
      left: 'center',
      top: 'middle',
      style: {
        text: '暂无数据',
        fontSize: 14,
        fill: '#5b6678'
      }
    }] : []
  }

  chartInstance.setOption(option, true)
}

// ============================================
// 切换时间周期
// ============================================
function switchPeriod(key) {
  activePeriod.value = key
  loadData()
}

// ============================================
// 监听设备列表变化，重新加载数据
// ============================================
watch(() => deviceStore.deviceList, () => {
  loadData()
}, { deep: true })

// ============================================
// 监听窗口大小变化
// ============================================
const handleResize = () => {
  if (chartInstance) {
    chartInstance.resize()
  }
}

// ============================================
// 组件挂载 / 卸载
// ============================================
onMounted(() => {
  setTimeout(() => {
    initChart()
  }, 100)
  window.addEventListener('resize', handleResize)
})

onUnmounted(() => {
  if (chartInstance) {
    chartInstance.dispose()
    chartInstance = null
  }
  window.removeEventListener('resize', handleResize)
})
</script>

<template>
  <div class="light-trend-chart">
    <!-- 头部：标题 + 时间周期选择器 -->
    <div class="chart-head">
      <h3 class="chart-title">光照强度趋势</h3>
      <div class="period-selector">
        <button
          v-for="p in periods"
          :key="p.key"
          :class="['period-btn', { active: activePeriod === p.key }]"
          @click="switchPeriod(p.key)"
        >
          {{ p.label }}
        </button>
        <span v-if="loading" class="loading-text">加载中...</span>
      </div>
    </div>

    <!-- 图表容器 -->
    <div ref="chartRef" class="chart-container"></div>
  </div>
</template>

<style scoped>
.light-trend-chart {
  width: 100%;
}

/* 头部：左标题 + 右周期按钮 */
.chart-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
  padding: 0 4px;
}

.chart-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.chart-title::before {
  content: '';
  width: 3px;
  height: 14px;
  background: var(--primary-color);
  border-radius: 2px;
}

.period-selector {
  display: flex;
  align-items: center;
  gap: 8px;
}

.period-btn {
  padding: 4px 14px;
  border: 1px solid var(--border-color-dark);
  border-radius: 8px;
  background: transparent;
  color: var(--text-regular);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.period-btn:hover {
  border-color: var(--primary-color);
  color: var(--primary-color);
}

.period-btn.active {
  background: var(--primary-color);
  border-color: var(--primary-color);
  color: #1a1408;
}

.loading-text {
  font-size: 12px;
  color: var(--text-placeholder);
  margin-left: 8px;
}

.chart-container {
  width: 100%;
  height: 380px;
}
</style>
