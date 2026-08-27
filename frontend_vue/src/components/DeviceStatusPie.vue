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
            itemStyle: { color: '#5f8f5a' }
        },
        {
            value: deviceStore.offlineCount,
            name: '离线',
            itemStyle: { color: '#b4ada3' }
        },
        {
            value: deviceStore.faultCount,
            name: '故障',
            itemStyle: { color: '#be4b40' }
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
    const total = currentData.reduce((sum, d) => sum + d.value, 0)

    const option = {
        animation: true,

        // 环形中心：设备总数大字
        title: {
            text: String(total),
            subtext: '设备总数',
            left: 'center',
            top: '30%',
            itemGap: 6,
            textStyle: { fontSize: 28, fontWeight: 600, color: '#1f1c19' },
            subtextStyle: { fontSize: 12, color: '#8a837b' }
        },

        tooltip: {
            trigger: 'item',
            backgroundColor: '#ffffff',
            borderColor: '#e8e4dc',
            borderWidth: 1,
            padding: [8, 12],
            textStyle: { color: '#1f1c19', fontSize: 12 },
            extraCssText: 'box-shadow: 0 4px 16px rgba(60, 50, 40, 0.12); border-radius: 8px;',
            formatter: '{b}：{c} 台（{d}%）'
        },

        // 底部横向圆点图例
        legend: {
            bottom: 0,
            left: 'center',
            icon: 'circle',
            itemWidth: 8,
            itemHeight: 8,
            itemGap: 18,
            textStyle: { color: '#57504a', fontSize: 12 }
        },

        series: [
            {
                name: '设备状态',
                type: 'pie',
                radius: ['56%', '78%'],  // 细环形
                center: ['50%', '42%'],
                padAngle: 2,             // 分段间隙
                avoidLabelOverlap: false,
                itemStyle: {
                    borderRadius: 6,       // 圆角分段
                    borderColor: '#ffffff',
                    borderWidth: 2
                },
                label: { show: false },  // 不要外引标签，靠图例 + tooltip
                emphasis: {
                    scale: true,
                    scaleSize: 4
                },
                data: currentData
            }
        ],

        // 无设备时提示
        graphic: total === 0 ? [{
            type: 'text',
            left: 'center',
            top: '40%',
            style: {
                text: '暂无设备',
                fontSize: 14,
                fill: '#b4ada3'
            }
        }] : []
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
        <!-- 标题 -->
        <h3 class="chart-title">设备状态分布</h3>
        <!-- 图表容器 -->
        <div ref="chartRef" class="chart-container"></div>
    </div>
</template>

<style scoped>
.device-status-pie {
    width: 100%;
}

.chart-title {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px 4px;
    font-size: 15px;
    font-weight: 600;
    color: #1f1c19;
}

.chart-title::before {
    content: '';
    width: 3px;
    height: 14px;
    background: #c96a4a;
    border-radius: 2px;
}

.chart-container {
    width: 100%;
    height: 300px;
}
</style>
