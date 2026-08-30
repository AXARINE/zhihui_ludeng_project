<script setup>
/**
 * 地图大屏页面
 *
 * 功能：
 * - 在高德底图上展示全部路灯设备点位（在线/灯态着色）
 * - 点击点位弹出设备详情（状态、灯态、模式、最新光照、最后在线）
 * - 弹窗内可跳转设备详情页
 * - 10 秒自动轮询刷新（可手动关闭）
 * - 未定位设备（无坐标）单独列出，不丢失
 * - RTS 式框选批量控灯：框选模式拖矩形选中多台，一键批量开/关/自动
 *   （Shift 追加选择，Esc 取消；后端无批量接口，前端逐台并发下发）
 * - 手动移动定位：管理员可开启"移动定位"模式，直接拖拽点位到真实位置，
 *   松手确认后经 GCJ-02 → WGS84 反算调 PATCH /api/devices/{id} 入库
 *   （进入移动模式自动暂停轮询，防止刷新把拖到一半的点位拽回原位；
 *    拖到地图边缘时自动平移跟随 —— Leaflet Marker 的 autoPan）
 *
 * 坐标系说明（重要）：
 * - 后端 /api/map/devices 返回 WGS84 坐标
 * - 高德底图是 GCJ-02，直接打点会偏移 100~700 米
 * - 渲染前统一经 utils/coord.js 的 wgs84ToGcj02 转换
 * - 拖拽保存前用 gcj02ToWgs84 转回 WGS84（后端约定统一存 WGS84）
 *
 * 权限：查看需要 device:status（菜单已控制）；批量控灯需要 control:manual；
 * 移动定位需要 device:manage（路灯管理员 admin 及以上，市政人员不可见）
 */
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Refresh, Select, Position } from '@element-plus/icons-vue'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import { getMapDevices, controlLamp, updateDevice } from '@/api/device'
import { mockMapDeviceList, mockResponse } from '@/mock/device'
import { wgs84ToGcj02, gcj02ToWgs84, formatLatDms, formatLngDms } from '@/utils/coord'
import { formatBeijingTime } from '@/utils/time'

const router = useRouter()

// 与 store 同款开关：VITE_USE_MOCK=false 时走真实后端
const USE_MOCK = import.meta.env.VITE_USE_MOCK !== 'false'

// ---- 权限判断（与 DeviceList 同款写法） ----
function hasPerm(code) {
  try {
    const perms = JSON.parse(localStorage.getItem('permissions') || '[]')
    const role = JSON.parse(localStorage.getItem('role') || '{}')
    if (role.role_code === 'super_admin') return true
    return perms.includes(code)
  } catch { return false }
}
const canControl = computed(() => hasPerm('control:manual'))
const canRelocate = computed(() => hasPerm('device:manage'))

// ---- 数据 ----
const devices = ref([])
const loading = ref(false)
const lastUpdated = ref('')
const autoRefresh = ref(true)

// 有坐标的设备（打点用）；没坐标的单独列出
const located = computed(() =>
  devices.value.filter(d => d.latitude != null && d.longitude != null)
)
const unlocated = computed(() =>
  devices.value.filter(d => d.latitude == null || d.longitude == null)
)

const onlineCount = computed(() =>
  located.value.filter(d => (d.status || '').toLowerCase() === 'online').length
)
const lampOnCount = computed(() =>
  located.value.filter(d => (d.lamp || '').toLowerCase() === 'on').length
)

// ---- Leaflet 实例与标记缓存 ----
let map = null
const markerMap = new Map()   // device id -> L.Marker
let pollTimer = null
let firstFitDone = false

// ---- RTS 式框选批量控灯 ----
const boxSelectMode = ref(false)   // 框选模式开关（关闭地图拖拽，拖矩形选中点位）
const selectedIds = ref([])        // 已选中的设备 ID 列表
const batchLoading = ref(false)    // 批量指令下发中
let selStart = null                // 框选起点（地图容器像素坐标）
let selRect = null                 // 框选矩形图层

// ---- 手动移动定位（device:manage） ----
const moveMode = ref(false)        // 移动定位模式：点位可拖拽，松手确认入库
let draggingId = null              // 正在被拖拽的设备 ID（轮询刷新时跳过它）
let dragOrigin = null              // 拖拽起点（GCJ-02 latlng），取消保存时回弹

// 简易 HTML 转义（设备名/位置是用户输入，防注入）
function escapeHtml(s) {
  return String(s ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
}

// 根据设备状态返回标记的 CSS 类名
function markerClass(d) {
  const status = (d.status || '').toLowerCase()
  const lamp = (d.lamp || '').toLowerCase()
  if (status !== 'online') return 'pin-offline'
  if (lamp === 'on') return 'pin-on'
  return 'pin-online-off'
}

// 构建点位图标（divIcon：纯 CSS 圆点，不依赖图片资源）
// 选中的设备（框选）加 selected 类：放大 + 赤陶描边
function buildIcon(d) {
  const cls = markerClass(d)
  const selected = selectedIds.value.includes(d.id)
  const label = escapeHtml(d.name || d.id)
  return L.divIcon({
    className: 'lamp-marker',
    html: `<div class="lamp-pin ${cls}${selected ? ' selected' : ''}"><span class="pin-core"></span></div><span class="pin-label">${label}</span>`,
    iconSize: [22, 36],
    iconAnchor: [11, 11]
  })
}

// 弹窗坐标行：北纬/东经各一行（度分秒）；鼠标悬停 title 显示十进制原始值
function formatLatLine(d) {
  if (d.latitude == null || d.longitude == null) return '未定位'
  return `${formatLatDms(d.latitude)}<br>${formatLngDms(d.longitude)}`
}

// 构建弹窗内容
function buildPopup(d) {
  const status = (d.status || '').toLowerCase()
  const lamp = (d.lamp || '').toLowerCase()
  const mode = (d.mode || '').toLowerCase()
  const statusText = status === 'online' ? '在线' : status === 'offline' ? '离线' : '未知'
  const statusCls = status === 'online' ? 'ok' : 'bad'
  const lampText = lamp === 'on' ? '💡 亮' : lamp === 'off' ? '🌑 灭' : '未知'
  const modeText = mode === 'auto' ? '自动' : mode === 'manual' ? '手动' : '未知'
  return `
    <div class="popup-box">
      <div class="popup-title">${escapeHtml(d.name || d.id)}</div>
      <div class="popup-row"><span>设备ID</span><code>${escapeHtml(d.id)}</code></div>
      <div class="popup-row"><span>位置</span><span>${escapeHtml(d.location) || '-'}</span></div>
      <div class="popup-row"><span>坐标</span><span class="popup-coord" title="${d.latitude}, ${d.longitude}（WGS84）">${formatLatLine(d)}</span></div>
      <div class="popup-row"><span>状态</span><span class="popup-tag ${statusCls}">${statusText}</span></div>
      <div class="popup-row"><span>灯态</span><span>${lampText}</span></div>
      <div class="popup-row"><span>模式</span><span>${modeText}</span></div>
      <div class="popup-row"><span>光照</span><span>${d.lux != null ? d.lux + ' lx' : '无数据'}</span></div>
      <div class="popup-row"><span>最后在线</span><span>${d.last_seen_at ? formatBeijingTime(d.last_seen_at) : '从未'}</span></div>
      <button class="popup-detail-btn" data-id="${escapeHtml(d.id)}">查看详情</button>
    </div>`
}

// 拉取数据并增量更新标记（保留已打开的弹窗，不闪屏）
async function fetchMapDevices() {
  loading.value = true
  try {
    const res = USE_MOCK ? await mockResponse(mockMapDeviceList) : await getMapDevices()
    devices.value = res || []
    lastUpdated.value = formatBeijingTime(new Date().toISOString(), 'time')
    renderMarkers()

    // 首次加载把视野适配到所有点位
    if (!firstFitDone && located.value.length > 0) {
      const bounds = L.latLngBounds(located.value.map(d => {
        const c = wgs84ToGcj02(d.longitude, d.latitude)
        return [c.lat, c.lng]
      }))
      map.fitBounds(bounds.pad(0.3), { maxZoom: 17 })
      firstFitDone = true
    }
  } catch (e) {
    console.log('地图点位加载失败：', e)
    ElMessage.error('地图点位加载失败：' + (e?.response?.data || e.message))
  } finally {
    loading.value = false
  }
}

// 增量更新：已有标记就更新位置/图标/弹窗，新增的补，消失的删
function renderMarkers() {
  const alive = new Set()

  for (const d of located.value) {
    const c = wgs84ToGcj02(d.longitude, d.latitude)
    const latlng = [c.lat, c.lng]
    let marker = markerMap.get(d.id)

    if (!marker) {
      marker = L.marker(latlng, {
        icon: buildIcon(d),
        title: d.name || d.id,
        // 拖拽定位时点位贴近边缘，地图自动平移跟随（仅拖拽中生效，平时无副作用）
        autoPan: true,
        autoPanPadding: [60, 60],
        autoPanSpeed: 8
      })
      marker.deviceId = d.id
      // 拖拽改定位（仅 moveMode 下 dragging 才被 enable，处理器常挂无副作用）
      marker.on('dragstart', () => {
        draggingId = d.id
        dragOrigin = { id: d.id, latlng: marker.getLatLng() }
        // 常驻 tooltip 实时显示拖拽位置（度分秒经纬度）
        marker.bindTooltip('', {
          permanent: true, direction: 'top',
          offset: [0, -14], className: 'drag-coord-tip'
        }).openTooltip()
      })
      marker.on('drag', () => {
        const p = marker.getLatLng()
        const w = gcj02ToWgs84(p.lng, p.lat)
        marker.setTooltipContent(`${formatLatDms(w.lat)} · ${formatLngDms(w.lng)}`)
      })
      marker.on('dragend', () => {
        marker.unbindTooltip()
        onMarkerDragEnd(marker)
      })
      marker.addTo(map)
      marker.bindPopup(buildPopup(d))
      markerMap.set(d.id, marker)
    } else {
      // 正在拖拽的点位不动（虽然移动模式下已停轮询，防手动刷新干扰）
      if (d.id !== draggingId) marker.setLatLng(latlng)
      marker.setIcon(buildIcon(d))
      marker.setPopupContent(buildPopup(d))
    }
    alive.add(d.id)
  }

  // 已被删除的设备：移除标记，并从选中列表剔除
  for (const [id, marker] of markerMap) {
    if (!alive.has(id)) {
      map.removeLayer(marker)
      markerMap.delete(id)
    }
  }
  if (selectedIds.value.length > 0) {
    selectedIds.value = selectedIds.value.filter(id => alive.has(id))
  }
}

// 选中状态变化后，刷新所有标记的图标（放大/描边效果）
function refreshMarkerStyles() {
  for (const [id, marker] of markerMap) {
    const d = devices.value.find(x => x.id === id)
    if (d) marker.setIcon(buildIcon(d))
  }
}

// ---- 框选模式：进入/退出 ----
function toggleBoxSelect() {
  // 与移动定位模式互斥：进入框选先退出移动模式
  if (!boxSelectMode.value && moveMode.value) {
    moveMode.value = false
    applyMoveMode()
  }
  boxSelectMode.value = !boxSelectMode.value
  applyBoxSelectMode()
}

function applyBoxSelectMode() {
  if (!map) return
  if (boxSelectMode.value) {
    map.dragging.disable()   // 拖拽让位给框选
    map.closePopup()
    map.getContainer().classList.add('box-selecting')
  } else {
    map.dragging.enable()
    map.getContainer().classList.remove('box-selecting')
    clearSelRect()
    clearSelection()
  }
}

// 清除临时框选矩形
function clearSelRect() {
  if (selRect) {
    map.removeLayer(selRect)
    selRect = null
  }
  selStart = null
}

// ---- 手动移动定位：进入/退出 ----
function toggleMoveMode() {
  // 与框选模式互斥：进入移动模式先退出框选
  if (!moveMode.value && boxSelectMode.value) {
    boxSelectMode.value = false
    applyBoxSelectMode()
  }
  moveMode.value = !moveMode.value
  applyMoveMode()
}

function applyMoveMode() {
  if (!map) return
  const container = map.getContainer()
  if (moveMode.value) {
    map.closePopup()
    container.classList.add('move-mode')
    for (const [, marker] of markerMap) marker.dragging?.enable()
    stopPolling()   // 停轮询：防止刷新把拖到一半的点位重置回旧坐标
  } else {
    container.classList.remove('move-mode')
    for (const [, marker] of markerMap) marker.dragging?.disable()
    draggingId = null
    dragOrigin = null
    if (autoRefresh.value) startPolling()   // 恢复原轮询状态
  }
}

// 取消/失败时把点位回弹到拖拽前的位置
function revertMarker(id) {
  if (!dragOrigin || dragOrigin.id !== id) return
  const marker = markerMap.get(id)
  if (marker) marker.setLatLng(dragOrigin.latlng)
  dragOrigin = null
}

// 拖拽松手：确认 → GCJ-02 反算 WGS84 → PATCH 入库；取消则回弹
async function onMarkerDragEnd(marker) {
  const id = marker.deviceId
  const d = devices.value.find(x => x.id === id)
  draggingId = null
  if (!d) { dragOrigin = null; return }

  const gcj = marker.getLatLng()
  const wgs = gcj02ToWgs84(gcj.lng, gcj.lat)
  const lat = Number(wgs.lat.toFixed(6))
  const lng = Number(wgs.lng.toFixed(6))

  try {
    await ElMessageBox.confirm(
      `将「${d.name || d.id}」的定位修改为：<br><b>${formatLatDms(lat)} · ${formatLngDms(lng)}</b><br>` +
      `<code>${lat}, ${lng}</code>（WGS84）`,
      '修改设备定位',
      {
        type: 'warning',
        dangerouslyUseHTMLString: true,
        confirmButtonText: '保存',
        cancelButtonText: '取消'
      }
    )
  } catch {
    revertMarker(id)
    return
  }

  try {
    if (USE_MOCK) await mockResponse({ success: true })
    else await updateDevice(id, { latitude: lat, longitude: lng })
    d.latitude = lat
    d.longitude = lng
    dragOrigin = null
    ElMessage.success(`「${d.name || d.id}」定位已更新`)
  } catch (e) {
    ElMessage.error('定位更新失败：' + (e?.response?.data || e.message))
    revertMarker(id)
  }
}

function clearSelection() {
  if (selectedIds.value.length === 0) return
  selectedIds.value = []
  refreshMarkerStyles()
}

// ---- 框选三个鼠标事件（mousedown 起点 / mousemove 拉框 / mouseup 判定） ----
function onSelMouseDown(e) {
  if (!boxSelectMode.value) return
  clearSelRect()
  selStart = e.containerPoint
}

function onSelMouseMove(e) {
  if (!boxSelectMode.value || !selStart) return
  const p1 = selStart
  const p2 = e.containerPoint
  // 移动距离太小不画框（避免手抖画出一条线）
  if (Math.abs(p1.x - p2.x) < 3 || Math.abs(p1.y - p2.y) < 3) return
  const bounds = L.latLngBounds(
    map.containerPointToLatLng(p1),
    map.containerPointToLatLng(p2)
  )
  if (!selRect) {
    selRect = L.rectangle(bounds, {
      color: '#c96a4a', weight: 1.5,
      fillColor: '#c96a4a', fillOpacity: 0.12,
      interactive: false
    }).addTo(map)
  } else {
    selRect.setBounds(bounds)
  }
}

function onSelMouseUp(e) {
  if (!boxSelectMode.value || !selStart) return
  const p1 = selStart
  const p2 = e.containerPoint
  const tiny = Math.abs(p1.x - p2.x) < 5 && Math.abs(p1.y - p2.y) < 5
  const bounds = L.latLngBounds(
    map.containerPointToLatLng(p1),
    map.containerPointToLatLng(p2)
  )
  clearSelRect()

  if (tiny) return   // 视为单击，不改变选择

  // 矩形覆盖到的点位入选；Shift = 追加（RTS 惯例）
  const hits = []
  for (const [id, marker] of markerMap) {
    if (bounds.contains(marker.getLatLng())) hits.push(id)
  }
  const prev = e.originalEvent.shiftKey ? selectedIds.value : []
  selectedIds.value = [...new Set([...prev, ...hits])]
  refreshMarkerStyles()
}

// Esc 取消选择 / 退出框选模式 / 退出移动定位模式
function onKeydown(e) {
  if (e.key !== 'Escape') return
  if (selectedIds.value.length > 0) {
    clearSelection()
  } else if (boxSelectMode.value) {
    boxSelectMode.value = false
    applyBoxSelectMode()
  } else if (moveMode.value) {
    moveMode.value = false
    applyMoveMode()
  }
}

// ---- 批量控灯：逐台并发下发（后端单设备接口，控灯幂等） ----
async function batchControl(action) {
  const ids = [...selectedIds.value]
  if (ids.length === 0 || batchLoading.value) return
  const actionText = { on: '开灯', off: '关灯', auto: '恢复自动' }[action]

  try {
    await ElMessageBox.confirm(
      `确定对选中的 ${ids.length} 台设备执行「${actionText}」？`,
      '批量控制确认',
      { type: 'warning', confirmButtonText: '下发', cancelButtonText: '取消' }
    )
  } catch { return }

  batchLoading.value = true
  try {
    const results = await Promise.allSettled(
      ids.map(id => USE_MOCK ? mockResponse({ success: true }) : controlLamp(id, action))
    )
    const failed = results.filter(r => r.status === 'rejected')
    const okCount = results.length - failed.length
    if (failed.length === 0) {
      ElMessage.success(`「${actionText}」指令已下发：${okCount} 台`)
    } else {
      ElMessage.warning(`「${actionText}」：${okCount} 台成功，${failed.length} 台失败（详情见审计日志）`)
    }
    clearSelection()
  } finally {
    batchLoading.value = false
  }

  // 板子约 5 秒上报真实状态，稍后刷新（同时立刻刷一次拿指令受理结果）
  fetchMapDevices()
  setTimeout(fetchMapDevices, 6000)
}

// 初始化地图
async function initMap() {
  await nextTick()

  map = L.map('map-container', {
    center: [35.0, 105.0],   // 无点位时的默认视野（中国全图）
    zoom: 4,
    zoomControl: true
  })

  // 高德栅格瓦片（GCJ-02）。免 key 的公开瓦片端点，国内访问快；
  // 如需正式商用，替换为高德 JS API + key 即可（坐标转换逻辑不变）
  L.tileLayer('https://webrd0{s}.is.autonavi.com/appmaptile?lang=zh_cn&size=1&scale=1&style=8&x={x}&y={y}&z={z}', {
    subdomains: ['1', '2', '3', '4'],
    maxZoom: 18,
    attribution: '© 高德地图'
  }).addTo(map)

  // 容器尺寸晚于地图初始化变化时，强制重算瓦片布局
  setTimeout(() => map.invalidateSize(), 200)

  // 弹窗内"查看详情"按钮的事件委托（弹窗 DOM 由 Leaflet 动态创建）
  map.on('popupopen', (e) => {
    const btn = e.popup.getElement()?.querySelector('.popup-detail-btn')
    if (btn) {
      btn.addEventListener('click', () => {
        router.push(`/device/${btn.dataset.id}`)
      })
    }
  })

  // 框选三件套 + Esc
  map.on('mousedown', onSelMouseDown)
  map.on('mousemove', onSelMouseMove)
  map.on('mouseup', onSelMouseUp)
  window.addEventListener('keydown', onKeydown)
}

function toggleAutoRefresh() {
  autoRefresh.value = !autoRefresh.value
  if (autoRefresh.value) {
    startPolling()
  } else {
    stopPolling()
  }
}

function startPolling() {
  if (pollTimer) return
  pollTimer = setInterval(fetchMapDevices, 10000)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

onMounted(async () => {
  await initMap()
  await fetchMapDevices()
  startPolling()
})

onBeforeUnmount(() => {
  stopPolling()
  window.removeEventListener('keydown', onKeydown)
  if (map) {
    map.remove()
    map = null
  }
  markerMap.clear()
})
</script>

<template>
  <div class="map-page">
    <div class="page-header">
      <h2>设备地图</h2>
      <p>路灯点位实时分布 — 点坐标显示设备详情</p>
    </div>

    <div class="map-card">
      <div id="map-container" v-loading="loading"></div>

      <!-- 顶部工具条：框选批量控制 / 移动定位（按权限显示对应入口） -->
      <div class="float-panel batch-bar" v-if="canControl || canRelocate">
        <template v-if="boxSelectMode && selectedIds.length === 0">
          <span class="batch-hint">拖动框选设备 · Shift 加选 · Esc 退出</span>
          <el-button size="small" @click="toggleBoxSelect">退出框选</el-button>
        </template>
        <template v-else-if="boxSelectMode">
          <span class="batch-count">已选 <b>{{ selectedIds.length }}</b> 台</span>
          <el-button size="small" type="success" :disabled="batchLoading" @click="batchControl('on')">开灯</el-button>
          <el-button size="small" type="danger" :disabled="batchLoading" @click="batchControl('off')">关灯</el-button>
          <el-button size="small" type="warning" plain :disabled="batchLoading" @click="batchControl('auto')">恢复自动</el-button>
          <el-button size="small" :disabled="batchLoading" @click="clearSelection">取消</el-button>
        </template>
        <template v-else-if="moveMode">
          <span class="batch-hint">拖动点位修改定位 · Esc 退出</span>
          <el-button size="small" @click="toggleMoveMode">退出</el-button>
        </template>
        <template v-else>
          <el-button size="small" :icon="Select" @click="toggleBoxSelect" v-if="canControl">框选控制</el-button>
          <el-button size="small" :icon="Position" type="warning" plain @click="toggleMoveMode" v-if="canRelocate">移动定位</el-button>
        </template>
      </div>

      <!-- 浮动统计面板 -->
      <div class="float-panel stats-panel">
        <div class="stats-row total">
          <span class="stats-num">{{ located.length }}</span>
          <span class="stats-label">已定位</span>
        </div>
        <div class="stats-row">
          <span class="stats-num ok">{{ onlineCount }}</span>
          <span class="stats-label">在线</span>
        </div>
        <div class="stats-row">
          <span class="stats-num warn">{{ lampOnCount }}</span>
          <span class="stats-label">亮灯</span>
        </div>
        <div class="stats-row">
          <span class="stats-num bad">{{ located.length - onlineCount }}</span>
          <span class="stats-label">离线</span>
        </div>
        <div class="panel-actions">
          <el-button size="small" :icon="Refresh" @click="fetchMapDevices" :loading="loading">刷新</el-button>
          <el-button size="small" :type="autoRefresh ? 'primary' : 'info'" plain @click="toggleAutoRefresh">
            {{ autoRefresh ? '轮询中' : '已暂停' }}
          </el-button>
        </div>
        <div class="updated-at" v-if="lastUpdated">更新于 {{ lastUpdated }}</div>
      </div>

      <!-- 图例 -->
      <div class="float-panel legend-panel">
        <div class="legend-item"><span class="dot pin-on"></span>在线 · 亮灯</div>
        <div class="legend-item"><span class="dot pin-online-off"></span>在线 · 灭灯</div>
        <div class="legend-item"><span class="dot pin-offline"></span>离线</div>
      </div>

      <!-- 未定位设备提示 -->
      <div class="float-panel unlocated-panel" v-if="unlocated.length > 0">
        <div class="unlocated-title">未定位 {{ unlocated.length }} 台（无坐标）</div>
        <el-tag
          v-for="d in unlocated"
          :key="d.id"
          size="small"
          type="info"
          class="unlocated-tag"
          @click="router.push(`/device/${d.id}`)"
        >
          {{ d.name || d.id }}
        </el-tag>
      </div>
    </div>
  </div>
</template>

<style scoped>
.map-page {
  padding: 24px;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.page-header {
  margin-bottom: 16px;
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

/* ---- 地图卡片：填满剩余高度 ---- */
.map-card {
  position: relative;
  flex: 1;
  min-height: 480px;
  border: 1px solid #e8e4dc;
  border-radius: 10px;
  overflow: hidden;
  background: #fff;
  box-shadow: var(--shadow-sm);
}

#map-container {
  width: 100%;
  height: 100%;
  z-index: 0;   /* 浮动面板 z-index 需高于 leaflet 控件层 */
}

/* ---- 浮动面板通用 ---- */
.float-panel {
  position: absolute;
  z-index: 1000;   /* leaflet 控件层是 1000 以内，压过它 */
  background: rgba(255, 255, 255, 0.94);
  border: 1px solid #e8e4dc;
  border-radius: 10px;
  box-shadow: var(--shadow-md);
  backdrop-filter: blur(4px);
}

/* 统计面板：右上 */
.stats-panel {
  top: 12px;
  right: 12px;
  padding: 12px 14px;
  width: 150px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.stats-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.stats-row.total .stats-num {
  color: #1f1c19;
}

.stats-num {
  font-size: 20px;
  font-weight: 600;
  font-family: var(--font-mono);
  color: #57504a;
}

.stats-num.ok { color: #5f8f5a; }
.stats-num.warn { color: #c08340; }
.stats-num.bad { color: #be4b40; }

.stats-label {
  font-size: 12px;
  color: #8a837b;
}

.panel-actions {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}

.panel-actions .el-button {
  flex: 1;
  margin: 0;
}

.updated-at {
  font-size: 11px;
  color: #b4ada3;
  text-align: center;
}

/* ---- 框选/批量控制条：顶部中间 ---- */
.batch-bar {
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  padding: 6px 10px;
  display: flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}

.batch-hint {
  font-size: 12px;
  color: #8a837b;
}

.batch-count {
  font-size: 13px;
  color: #57504a;
}

.batch-count b {
  color: #c96a4a;
  font-family: var(--font-mono);
  font-size: 15px;
}

/* 框选模式：十字光标（ID 选择器优先级压过 leaflet 的 grab 光标） */
#map-container.box-selecting {
  cursor: crosshair;
}

/* 移动定位模式：点位显示可拖光标 */
#map-container.move-mode .lamp-marker,
#map-container.move-mode .lamp-pin,
#map-container.move-mode .pin-label {
  cursor: move;
}

/* 图例：左下 */
.legend-panel {
  bottom: 12px;
  left: 12px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: #57504a;
}

/* 图例小圆点（颜色复用全局 .pin-on / .pin-online-off / .pin-offline） */
.dot {
  display: inline-block;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 2px solid #fff;
  box-shadow: 0 1px 3px rgba(31, 28, 25, 0.3);
  flex-shrink: 0;
}

/* 未定位面板：右下 */
.unlocated-panel {
  bottom: 12px;
  right: 12px;
  padding: 10px 12px;
  max-width: 260px;
}

.unlocated-title {
  font-size: 12px;
  color: #8a837b;
  margin-bottom: 8px;
}

.unlocated-tag {
  cursor: pointer;
  margin: 2px;
}

/* ---- 点位 / 弹窗样式 ---- */
/* 这些 DOM 由 Leaflet 动态创建，scoped 的 data 属性匹配不到， */
/* 且 scoped 会重命名 @keyframes 导致动画失效，所以放独立全局块（类名已带前缀，不会污染） */
</style>

<style>
.map-card .lamp-marker {
  background: none;
  border: none;
  text-align: center;
}

.map-card .lamp-pin {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0 auto;
  border: 2px solid #fff;
  box-shadow: 0 1px 4px rgba(31, 28, 25, 0.35);
  transition: transform 0.15s;
}

.map-card .lamp-marker:hover .lamp-pin {
  transform: scale(1.25);
}

/* 框选中的点位：放大 + 赤陶描边光圈（放在 hover 之后，同特异性后者生效） */
.map-card .lamp-pin.selected {
  transform: scale(1.35);
  border-color: #fff;
  box-shadow: 0 0 0 3px rgba(201, 106, 74, 0.6), 0 2px 8px rgba(31, 28, 25, 0.4);
}

/* 在线 · 亮灯：琥珀 + 光晕呼吸动画 */
.map-card .pin-on {
  background: #dda15e;
}

.map-card .pin-on .pin-core {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #fff3dd;
  animation: lamp-glow 2s ease-in-out infinite;
}

/* 在线 · 灭灯：绿 */
.map-card .pin-online-off {
  background: #5f8f5a;
}

/* 离线：灰 */
.map-card .pin-offline {
  background: #a8a29c;
}

/* 点位下方的小标签 */
.map-card .pin-label {
  display: inline-block;
  margin-top: 2px;
  font-size: 11px;
  line-height: 1.2;
  color: #1f1c19;
  background: rgba(255, 255, 255, 0.85);
  padding: 1px 5px;
  border-radius: 4px;
  white-space: nowrap;
  max-width: 110px;
  overflow: hidden;
  text-overflow: ellipsis;
  box-shadow: 0 1px 2px rgba(31, 28, 25, 0.15);
}

@keyframes lamp-glow {
  0%, 100% { box-shadow: 0 0 0 0 rgba(221, 161, 94, 0.7); }
  50% { box-shadow: 0 0 0 6px rgba(221, 161, 94, 0); }
}

/* ---- 弹窗内容 ---- */
.map-card .leaflet-popup-content {
  margin: 12px 14px;
  min-width: 200px;
}

.popup-box {
  font-size: 13px;
}

.popup-title {
  font-weight: 600;
  font-size: 14px;
  color: #1f1c19;
  margin-bottom: 8px;
  padding-bottom: 6px;
  border-bottom: 1px solid #efebe3;
}

.popup-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 3px 0;
  color: #57504a;
}

.popup-row > span:first-child {
  color: #8a837b;
  font-size: 12px;
  flex-shrink: 0;
}

.popup-row code {
  font-family: var(--font-mono);
  font-size: 12px;
}

.popup-coord {
  font-family: var(--font-mono);
  font-size: 12px;
  text-align: right;
  line-height: 1.5;
}

/* 拖拽定位时的实时坐标提示 */
.map-card .drag-coord-tip {
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: nowrap;
}

.popup-tag {
  font-size: 12px;
  padding: 1px 8px;
  border-radius: 999px;
}

.popup-tag.ok {
  color: #5f8f5a;
  background: #f0f6ee;
}

.popup-tag.bad {
  color: #be4b40;
  background: #f9ece9;
}

.popup-detail-btn {
  width: 100%;
  margin-top: 10px;
  padding: 6px 0;
  border: none;
  border-radius: 6px;
  background: #c96a4a;
  color: #fff7f2;
  font-size: 13px;
  cursor: pointer;
  transition: background 0.15s;
}

.popup-detail-btn:hover {
  background: #a8532f;
}
</style>
