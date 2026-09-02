<script setup>
/**
 * 亮度曲线编辑器（照度 → 亮度）
 *
 * 作用：编辑/预览调光曲线 dim_curve（格式 "lux:pct,lux:pct,..."，≤4 点，
 * lux 严格递增 0~100000，pct 0~100，空串 = 停用曲线）。
 *
 * 端点固定模式（路灯调光语义）：
 * - 第一点固定在亮度轴（照度恒 0 lux），只可上下调亮度 —— 全暗时最亮
 * - 最后一点固定在照度轴（亮度恒 0%），只可左右调照度 —— 足够亮时熄灭
 * - 两端点之间的中间点（≤2 个）自由增删、任意拖动
 *
 * 预览图轴刻度：
 * - 默认 symlog 拟合横轴（0~100 lux 线性、100 以上对数，ECharts 无原生
 *   symlog 轴，用 value 轴 + 双向变换实现）；轴范围随锚点照度自适应
 *   （拖点期间轴冻结，光标推出右边缘按住时连续扩量程）；0 lux 是正常坐标
 * - 可切换线性轴核对固件真实行为（固件 CurveEval 在 lux 线性域上分段线性插值）
 * - 不做缩放/平移：对数轴 + 少量可拖节点的编辑器（同参数均衡器 EQ 界面），
 *   业界惯例是自适应全量程 + 节点自由拖动；且 ECharts dataZoom 在对数轴上
 *   按线性值空间计算窗口（echarts#20927），缩放手感必然别扭
 */

import { ref, watch, onMounted, onUnmounted, computed, nextTick } from 'vue'
import * as echarts from 'echarts'

// ============================================
// Props / Emits
// ============================================
const props = defineProps({
  // dim_curve 字符串（v-model 双向绑定）
  modelValue: { type: String, default: '' }
})
const emit = defineEmits(['update:modelValue'])

// ============================================
// 锚点数组 [{lux, pct}] —— 编辑真源
// ============================================
const anchors = ref([])

// 首次启用曲线时预填的示例（黄昏渐进调光：全暗全亮 → 300lux 微亮 → 1000lux 熄灭）
const EXAMPLE_CURVE = '0:100,300:20,1000:0'

// ============================================
// 解析 "lux:pct,..." → 锚点数组（容错：非法点丢弃）
// ============================================
function parseCurve(str) {
  if (!str || typeof str !== 'string') return []
  return str
    .split(',')
    .filter(s => s.includes(':'))
    .map(s => {
      const [lux, pct] = s.split(':')
      return { lux: Number(lux), pct: Number(pct) }
    })
    .filter(p => Number.isFinite(p.lux) && Number.isFinite(p.pct))
}

// ============================================
// 端点归一化：第一点固定 0 lux（亮度轴）、末点固定 0%（照度轴），
// 按照度排序并保证严格递增；单点曲线补成两端点
// ============================================
function normalize(list) {
  const pts = list
    .map(p => ({ lux: Math.round(p.lux), pct: Math.round(p.pct) }))
    .sort((a, b) => a.lux - b.lux)
  if (pts.length === 0) return []
  if (pts.length === 1) {
    return [
      { lux: 0, pct: pts[0].pct },
      { lux: pts[0].lux > 0 ? pts[0].lux : 100, pct: 0 }
    ]
  }
  pts[0].lux = 0
  pts[pts.length - 1].pct = 0
  for (let i = 1; i < pts.length; i++) {
    if (pts[i].lux <= pts[i - 1].lux) pts[i].lux = pts[i - 1].lux + 1
  }
  return pts
}

// ============================================
// 序列化 锚点数组 → 字符串
// ============================================
function serializeCurve(list) {
  return list.map(p => `${Math.round(p.lux)}:${Math.round(p.pct)}`).join(',')
}

// 外部值变化 → 重新解析（保存成功/重置后同步）
watch(() => props.modelValue, (val) => {
  const parsed = normalize(parseCurve(val))
  // 避免自己输入时被回写打断
  if (serializeCurve(parsed) !== serializeCurve(anchors.value)) {
    anchors.value = parsed
  }
  // 已有曲线则开关置为启用
  if (parsed.length > 0) {
    enabled.value = true
    lastNonEmpty = val
  }
}, { immediate: true })

// 锚点变化 → 上抛字符串
watch(anchors, (list) => {
  emit('update:modelValue', serializeCurve(list))
}, { deep: true })

// ============================================
// 曲线启用开关
// ============================================
const enabled = ref(false)
// 停用前的最后一条非空曲线（重新启用时恢复）
let lastNonEmpty = ''

watch(enabled, (on) => {
  if (on) {
    // 当前有锚点则不变；否则恢复上次曲线或预填示例
    if (anchors.value.length === 0) {
      anchors.value = normalize(parseCurve(lastNonEmpty || EXAMPLE_CURVE))
    }
  } else {
    const cur = serializeCurve(anchors.value)
    if (cur) lastNonEmpty = cur
    anchors.value = []
  }
})

// ============================================
// 锚点编辑：端点固定（不可删、不可离轴），只能在中间加/删
// ============================================
const MAX_POINTS = 4

// 中间点数量（端点之外）
const midCount = computed(() => Math.max(anchors.value.length - 2, 0))

function addPoint() {
  const n = anchors.value.length
  if (n >= MAX_POINTS) return
  const first = anchors.value[0]
  const last = anchors.value[n - 1]
  // 找相邻照度间隙最大的位置插入中间点
  const bounds = [first.lux, ...anchors.value.slice(1, n - 1).map(m => m.lux), last.lux]
  let bestGap = -1
  let bestIdx = 1
  let bestPrev = bounds[0]
  let bestNext = bounds[bounds.length - 1]
  for (let i = 0; i < bounds.length - 1; i++) {
    const gap = bounds[i + 1] - bounds[i]
    if (gap > bestGap) {
      bestGap = gap
      bestIdx = i + 1
      bestPrev = bounds[i]
      bestNext = bounds[i + 1]
    }
  }
  const lux = Math.min(
    Math.max(Math.round((bestPrev + bestNext) / 2 / 10) * 10, 10),
    100000 - 10
  )
  const pct = Math.round(first.pct / 2)
  anchors.value.splice(bestIdx, 0, { lux, pct })
}

function removePoint(index) {
  // 端点不可删
  if (index === 0 || index === anchors.value.length - 1) return
  anchors.value.splice(index, 1)
}

// ============================================
// 前端校验（与后端 validate_dim_curve 一致，后端仍会兜底校验）
// ============================================
const errors = computed(() => {
  const list = []
  let prevLux = -1
  anchors.value.forEach((p, i) => {
    const lux = Math.round(p.lux)
    const pct = Math.round(p.pct)
    if (!Number.isInteger(lux) || lux < 0 || lux > 100000) {
      list.push(`第 ${i + 1} 点照度需为 0~100000 的整数`)
    }
    if (!Number.isInteger(pct) || pct < 0 || pct > 100) {
      list.push(`第 ${i + 1} 点亮度需为 0~100 的整数`)
    }
    if (lux <= prevLux) {
      list.push(`第 ${i + 1} 点照度必须严格大于前一点`)
    }
    prevLux = lux
  })
  const str = serializeCurve(anchors.value)
  if (str.length > 64) {
    list.push('曲线串总长不能超过 64 字符')
  }
  return list
})

const curveValid = computed(() => errors.value.length === 0)

// 暴露校验状态给父组件（保存按钮禁用条件）
defineExpose({ curveValid })

// ============================================
// ECharts 预览图
// ============================================
const chartRef = ref(null)
let chartInstance = null
const scaleMode = ref('log') // 'log' 对数轴（默认） | 'linear' 线性轴

// 横轴范围随锚点自适应（高照度端按末点 ×2 留边距、封顶 100000，低照度端
// 恒 0；线性轴同理 ×1.1）；拖点期间轴完全冻结（值映射稳定，否则轴随被拖点
// 伸缩会与光标互相追、来回振荡）；光标推出右边缘按住时连续扩量程
// （不跳档），松手后轴重新自适应贴合
//
// "对数轴"实为 symlog 拟合：0~100 lux 线性（t = lux/100），100 以上对数
// （t = 1 + log10(lux/100)）。低照度段有真实分辨率，0 lux 是正常坐标
// （不再需要"0 画在轴左端点"的特例，左边也天然无需扩程——照度不可能 <0）。
// ECharts 无原生 symlog 轴，用 value 轴 + 双向变换实现，
// 刻度按整数 t 标注（0、100、1k、10k、100k）
function luxToT(lux) {
  const l = Math.max(lux, 0)
  return l <= 100 ? l / 100 : 1 + Math.log10(l / 100)
}
function tToLux(t) {
  return t <= 1 ? t * 100 : 100 * Math.pow(10, t - 1)
}

let currentRange = [0, 100000]
let dragRange = null
// 越界扩量程：rAF 逐帧连续扩展（GSAP Draggable autoScroll / Konva 边缘拖拽
// 滚动同款模式），速度随光标越界深度线性增加——轻推慢扩、深推快扩，
// 增量按真实帧间隔积分，无档位感
const EXTEND_BASE_RATE = 0.08 // 刚越界时的速度（数量级/秒；线性轴 ×(1+r·2)/秒）
const EXTEND_PER_PX = 0.005 // 每多越界 1px 增加的速度
const EXTEND_MAX_RATE = 0.9 // 速度上限
let extendRaf = null
let lastExtendTs = 0
let lastDragPx = 0
let lastDragPy = 0

function computeRange(pts, isLog) {
  const lastLux = pts[pts.length - 1].lux
  const xMax = isLog
    ? Math.min(Math.max(lastLux * 2, 1000), 100000)
    : Math.min(Math.max(lastLux * 1.1, 1000), 100000)
  return [0, xMax]
}

// 锚点 → 图坐标（对数模式经 symlog 变换；0 lux 是正常坐标）
function toPlotX(lux, isLog) {
  const l = Math.max(Math.round(lux), 0)
  return isLog ? luxToT(l) : l
}

function buildOption() {
  const pts = anchors.value.map(p => ({ lux: Math.round(p.lux), pct: Math.round(p.pct) }))
  const isLog = scaleMode.value === 'log'

  // 无锚点：空态
  if (pts.length === 0) {
    return {
      xAxis: { show: false }, yAxis: { show: false },
      series: [],
      graphic: [{
        type: 'text', left: 'center', top: 'middle',
        style: { text: '曲线未启用（回退施密特开关灯）', fontSize: 13, fill: '#5b6678' }
      }]
    }
  }

  // 轴范围：拖点期间完全冻结（含越界连续扩展后的新范围），平时随锚点自适应
  const [xMin, xMax] = dragIndex >= 0 && dragRange ? dragRange : computeRange(pts, isLog)
  currentRange = [xMin, xMax]

  // 图坐标数据（含真实值供 tooltip）
  const plotPts = pts.map(p => ({
    x: toPlotX(p.lux, isLog),
    y: p.pct,
    realLux: p.lux,
    pct: p.pct
  }))

  const isSingle = pts.length === 1

  // 轴坐标范围（对数模式经 symlog 变换；currentRange 保持 lux 域供扩程/钳制）
  const aMin = isLog ? luxToT(xMin) : xMin
  const aMax = isLog ? luxToT(xMax) : xMax

  // 单锚点 = 恒定亮度：向轴两端延伸成水平线
  const lineData = isSingle
    ? [
        { x: aMin, y: pts[0].pct, realLux: null, pct: pts[0].pct },
        { x: aMax, y: pts[0].pct, realLux: null, pct: pts[0].pct }
      ]
    : plotPts

  return {
    animation: false,
    tooltip: {
      trigger: 'item',
      backgroundColor: '#162032',
      borderColor: 'rgba(148, 168, 200, 0.2)',
      borderWidth: 1,
      textStyle: { color: '#eae4d3', fontSize: 12 },
      extraCssText: 'box-shadow: 0 4px 16px rgba(0,0,0,0.5); border-radius: 8px;',
      formatter: (p) => {
        const d = p.data
        if (d.realLux == null) return ''
        return `照度 <b>${d.realLux}</b> lux → 亮度 <b>${d.pct}</b>%`
      }
    },
    grid: { left: 48, right: 24, top: 28, bottom: 40, containLabel: false },
    xAxis: {
      // 两种模式都用 value 轴：对数模式的坐标是 symlog 变换后的 t
      // （0~100 lux 线性、以上对数），线性模式直接用 lux
      type: 'value',
      name: isLog ? '环境照度 lux（100 以下线性，以上对数）' : '环境照度 lux',
      nameLocation: 'middle',
      nameGap: 26,
      nameTextStyle: { color: '#8e9bb0', fontSize: 11 },
      min: aMin,
      max: aMax,
      // 对数模式刻度按整数 t 标注：0、100、1k、10k、100k
      interval: isLog ? 1 : undefined,
      minInterval: isLog ? 1 : undefined,
      axisLabel: {
        color: '#8e9bb0',
        fontSize: 11,
        // 一律整数标注：对数模式非整数刻度（如布局抖动时的 t=2.37）直接不显示，
        // 避免冒出 23.442k 这种小数标签；线性模式刻度值取整
        formatter: isLog
          ? (t) => {
              if (Math.abs(t - Math.round(t)) > 1e-6) return ''
              const v = tToLux(Math.round(t))
              return v >= 1000 ? `${Math.round(v / 1000)}k` : String(Math.round(v))
            }
          : (v) => String(Math.round(v))
      },
      axisLine: { lineStyle: { color: 'rgba(148, 168, 200, 0.25)' } },
      splitLine: { show: true, lineStyle: { color: 'rgba(148, 168, 200, 0.08)', type: 'dashed' } }
    },
    yAxis: {
      type: 'value',
      name: '亮度 %',
      nameTextStyle: { color: '#8e9bb0', fontSize: 11 },
      min: 0,
      max: 100,
      interval: 20,
      axisLabel: { color: '#8e9bb0', fontSize: 11, formatter: '{value}%' },
      axisLine: { lineStyle: { color: 'rgba(148, 168, 200, 0.25)' } },
      splitLine: { lineStyle: { color: 'rgba(148, 168, 200, 0.08)', type: 'dashed' } }
    },
    series: [
      {
        // 锚点间连线（symlog 轴上接近固件 lux 线性插值的真实形状）
        type: 'line',
        data: lineData.map(d => ({ value: [d.x, d.y], realLux: d.realLux, pct: d.pct })),
        symbol: 'none',
        silent: true,
        lineStyle: { color: '#e8a33d', width: 2.5 },
        z: 2
      },
      {
        // 锚点圆点（可拖拽：mousedown 命中后跟随鼠标移动，实时反算坐标）
        type: 'scatter',
        data: plotPts.map(d => ({ value: [d.x, d.y], realLux: d.realLux, pct: d.pct })),
        symbolSize: 10,
        itemStyle: { color: '#e8a33d', borderColor: '#131c2d', borderWidth: 2 },
        label: {
          show: true,
          position: 'top',
          distance: 6,
          color: '#b6bfce',
          fontSize: 11,
          formatter: (p) => `${p.data.pct}%`
        },
        z: 3
      }
    ]
  }
}

function renderChart() {
  if (!chartInstance) return
  // 合并模式（非 notMerge）：轴/系列对象复用，扩程时每帧只是更新 min/max
  // 和数据，不会整图销毁重建——否则标签布局反复重排，肉眼就是"抖"
  chartInstance.setOption(buildOption())
}

// 锚点或轴模式变化 → 重画（拖动时实时触发；拖点期间轴范围冻结所以不跳动）
watch([anchors, scaleMode], renderChart, { deep: true })

// ============================================
// 锚点拖拽：mousedown 命中圆点（像素距离 ≤ 22）→ mousemove 反算坐标 →
// 实时更新 anchors（输入框联动、曲线重画）；照度 clamp 在相邻锚点之间
// （严格递增不破坏），亮度 clamp 0~100；拖点开始即冻结轴范围（dragRange），
// 光标推出右边缘按住时连续扩量程，松手后轴重新自适应贴合锚点
// ============================================
let dragIndex = -1
const DRAG_HIT_PX = 22

function hitAnchor(e) {
  if (!chartInstance || anchors.value.length === 0) return -1
  const px = e.offsetX != null ? e.offsetX : e.event?.offsetX
  const py = e.offsetY != null ? e.offsetY : e.event?.offsetY
  const isLog = scaleMode.value === 'log'
  for (let i = 0; i < anchors.value.length; i++) {
    const p = anchors.value[i]
    const [sx, sy] = chartInstance.convertToPixel(
      { xAxisIndex: 0, yAxisIndex: 0 },
      [toPlotX(p.lux, isLog), Math.round(p.pct)]
    )
    if (Math.hypot(sx - px, sy - py) <= DRAG_HIT_PX) return i
  }
  return -1
}

function onDragMove(e) {
  if (dragIndex < 0 || !chartInstance || !chartRef.value) return
  const rect = chartRef.value.getBoundingClientRect()
  lastDragPx = e.clientX - rect.left
  lastDragPy = e.clientY - rect.top
  applyDragValue()
}

// 光标推出右边缘按住时，rAF 循环连续扩量程（光标不动也会继续扩，收回
// 绘图区内即停）；速度随越界深度增加（EXTEND_* 常量），按真实帧间隔积分。
// 扩完让被拖点重新求值，越界时 lux 会被钳到新的量程上限，
// 点和输入框跟着边缘平滑增长
function startExtendLoop() {
  lastExtendTs = performance.now()
  const step = (ts) => {
    extendRaf = null
    if (dragIndex < 0 || !dragRange || !chartInstance) return
    // 帧间隔（掉帧时钳到 100ms，避免大步长跳变）
    const dt = Math.min((ts - lastExtendTs) / 1000, 0.1)
    lastExtendTs = ts
    tickExtend(dt)
    extendRaf = requestAnimationFrame(step)
  }
  extendRaf = requestAnimationFrame(step)
}

function stopExtendLoop() {
  if (extendRaf != null) {
    cancelAnimationFrame(extendRaf)
    extendRaf = null
  }
}

function tickExtend(dt) {
  if (dragRange[1] >= 100000) return
  const isLog = scaleMode.value === 'log'
  const edgePx = chartInstance.convertToPixel({ xAxisIndex: 0 }, toPlotX(dragRange[1], isLog))
  if (!Number.isFinite(edgePx)) return
  const overshoot = lastDragPx - edgePx
  if (overshoot <= 0) return
  const rate = Math.min(EXTEND_BASE_RATE + overshoot * EXTEND_PER_PX, EXTEND_MAX_RATE)
  const factor = isLog ? Math.pow(10, rate * dt) : 1 + rate * dt * 2
  dragRange = [dragRange[0], Math.min(dragRange[1] * factor, 100000)]
  currentRange = dragRange.slice()
  // 先让被拖点按新上限重新求值，再统一渲染一次（顺序反过来会一帧内
  // 渲染两次，点在"边缘内侧→边缘"之间来回跳）
  applyDragValue()
  renderChart()
}

function applyDragValue() {
  const [xF, pctF] = chartInstance.convertFromPixel(
    { xAxisIndex: 0, yAxisIndex: 0 },
    [lastDragPx, lastDragPy]
  )
  if (!Number.isFinite(xF) || !Number.isFinite(pctF)) return

  // 对数模式轴坐标是 symlog 变换后的 t，反算回 lux；取整到整数 lux / %
  // （与输入框一致），拖动全程平滑不吸附
  const isLog = scaleMode.value === 'log'
  let lux = Math.round(isLog ? tToLux(xF) : xF)
  let pct = Math.round(pctF)
  const n = anchors.value.length
  const isFirst = dragIndex === 0
  const isLast = dragIndex === n - 1
  // 端点固定在自己的轴上：首点只动亮度（照度恒 0），末点只动照度（亮度恒 0%）
  if (isFirst) lux = 0
  if (isLast) pct = 0
  const prev = dragIndex > 0 ? Math.round(anchors.value[dragIndex - 1].lux) : null
  const next = dragIndex < n - 1 ? Math.round(anchors.value[dragIndex + 1].lux) : null
  if (prev != null) lux = Math.max(lux, prev + 1)
  if (next != null) lux = Math.min(lux, next - 1)
  // 值钳在当前（冻结或扩展后的）视野内
  lux = Math.min(Math.max(lux, 0), Math.min(currentRange[1], 100000))
  pct = Math.min(Math.max(pct, 0), 100)

  const p = anchors.value[dragIndex]
  if (p.lux !== lux || p.pct !== pct) {
    p.lux = lux
    p.pct = pct
  }
}

function onDragEnd() {
  if (dragIndex < 0) return
  dragIndex = -1
  dragRange = null
  stopExtendLoop()
  if (chartRef.value) chartRef.value.style.cursor = ''
  // 解冻轴范围，重画时按新锚点自适应贴合（收缩在此时生效）
  renderChart()
}

// 图表容器在"启用曲线"分支内（v-if），存在才初始化
function ensureChart() {
  if (!chartInstance && chartRef.value) {
    chartInstance = echarts.init(chartRef.value)
    renderChart()
    // 调试/自动化钩子
    window.__dimChart = chartInstance
    // 拖拽事件绑定（zrender 只在 canvas 内派发 mousedown/hover）
    chartInstance.getZr().on('mousedown', (e) => {
      if (anchors.value.length === 0) return
      dragIndex = hitAnchor(e)
      if (dragIndex >= 0 && chartRef.value) {
        chartRef.value.style.cursor = 'grabbing'
        // 冻结轴范围；启动越界扩量程 rAF 循环（光标推出右边缘时连续扩展）
        dragRange = currentRange.slice()
        lastDragPx = e.offsetX ?? 0
        lastDragPy = e.offsetY ?? 0
        startExtendLoop()
      }
    })
    chartInstance.getZr().on('mousemove', (e) => {
      if (dragIndex >= 0) {
        onDragMove({ clientX: chartRef.value.getBoundingClientRect().left + (e.offsetX ?? 0), clientY: chartRef.value.getBoundingClientRect().top + (e.offsetY ?? 0) })
      } else if (chartRef.value) {
        chartRef.value.style.cursor = hitAnchor(e) >= 0 ? 'grab' : ''
      }
    })
    // 鼠标移出图表后继续拖：全局 mousemove/mouseup
    window.addEventListener('mousemove', onDragMove)
    window.addEventListener('mouseup', onDragEnd)
  }
}

onMounted(() => {
  ensureChart()
  window.addEventListener('resize', handleResize)
})

// 启用开关打开后容器才渲染，等 DOM 更新后补初始化；关闭则销毁实例
watch(enabled, async (on) => {
  if (on) {
    await nextTick()
    ensureChart()
  } else if (chartInstance) {
    chartInstance.dispose()
    chartInstance = null
  }
})

onUnmounted(() => {
  window.removeEventListener('resize', handleResize)
  window.removeEventListener('mousemove', onDragMove)
  window.removeEventListener('mouseup', onDragEnd)
  stopExtendLoop()
  if (chartInstance) {
    chartInstance.dispose()
    chartInstance = null
  }
})

function handleResize() {
  if (chartInstance) chartInstance.resize()
}
</script>

<template>
  <div class="dim-curve-editor">
    <!-- 启用开关 -->
    <div class="curve-switch-row">
      <el-switch v-model="enabled" active-text="启用照度-亮度曲线" />
    </div>

    <!-- 锚点编辑 -->
    <div v-if="enabled" class="curve-editor-body">
      <div class="anchor-head">
        <el-button
          size="small"
          type="primary"
          plain
          :disabled="anchors.length >= MAX_POINTS"
          @click="addPoint"
        >
          + 添加中间点
        </el-button>
      </div>

      <div v-for="(p, i) in anchors" :key="i" class="anchor-row">
        <span class="anchor-index">#{{ i + 1 }}</span>
        <el-input-number
          v-model="p.lux"
          :min="0"
          :max="100000"
          :step="1"
          step-strictly
          controls-position="right"
          class="anchor-lux"
          :disabled="i === 0"
        />
        <span class="anchor-arrow" v-if="i === 0">亮度轴 · 0 lux →</span>
        <span class="anchor-arrow" v-else>lux →</span>
        <el-input-number
          v-model="p.pct"
          :min="0"
          :max="100"
          :step="1"
          step-strictly
          controls-position="right"
          class="anchor-pct"
          :disabled="i === anchors.length - 1"
        />
        <span class="anchor-unit" v-if="i === anchors.length - 1">% · 照度轴</span>
        <span class="anchor-unit" v-else>%</span>
        <el-button
          v-if="i > 0 && i < anchors.length - 1"
          size="small"
          type="danger"
          text
          @click="removePoint(i)"
        >
          删除
        </el-button>
      </div>

      <!-- 校验错误 -->
      <el-alert
        v-if="errors.length > 0"
        type="error"
        :closable="false"
        show-icon
        class="curve-errors"
      >
        <div v-for="(e, i) in errors" :key="i">• {{ e }}</div>
      </el-alert>

      <!-- 预览图 -->
      <div class="chart-head">
        <span class="chart-title">曲线预览</span>
        <div class="scale-switch">
          <button
            :class="['scale-btn', { active: scaleMode === 'log' }]"
            @click="scaleMode = 'log'"
          >
            对数轴
          </button>
          <button
            :class="['scale-btn', { active: scaleMode === 'linear' }]"
            @click="scaleMode = 'linear'"
          >
            线性轴
          </button>
        </div>
      </div>
      <div ref="chartRef" class="curve-chart"></div>
    </div>
  </div>
</template>

<style scoped>
.dim-curve-editor {
  width: 100%;
}

.curve-switch-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}

.curve-editor-body {
  border: 1px dashed var(--border-color-dark);
  border-radius: 12px;
  padding: 16px;
  background: var(--bg-inset);
}

.anchor-head {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  margin-bottom: 10px;
}

.anchor-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.anchor-index {
  width: 28px;
  font-size: 12px;
  color: var(--primary-color);
  font-weight: 600;
}

.anchor-lux {
  width: 150px;
}

.anchor-pct {
  width: 130px;
}

.anchor-arrow,
.anchor-unit {
  font-size: 12px;
  color: var(--text-secondary);
}

.curve-errors {
  margin-bottom: 14px;
}

.chart-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin: 14px 0 6px;
}

.chart-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.scale-switch {
  display: flex;
  gap: 6px;
}

.scale-btn {
  padding: 3px 12px;
  border: 1px solid var(--border-color-dark);
  border-radius: 8px;
  background: transparent;
  color: var(--text-regular);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.scale-btn:hover {
  border-color: var(--primary-color);
  color: var(--primary-color);
}

.scale-btn.active {
  background: var(--primary-color);
  border-color: var(--primary-color);
  color: #1a1408;
}

.curve-chart {
  width: 100%;
  height: 240px;
}
</style>
