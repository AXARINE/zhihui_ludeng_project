<script setup>
/**
 * 一盏真灯 — 首页签名 Hero
 *
 * 设计意图：让"路灯"成为首页的主角。SVG 路灯的光锥/光晕/地面光池
 * 由真实数据驱动：
 * - 灯亮（lamp === 'ON'）→ 琥珀光晕展开，光锥落地
 * - 灯灭 → 只剩夜色里的一盏剪影
 * - 大数字显示该灯当前环境照度（lux）
 *
 * 数据来源：Pinia store 的设备列表（5s 轮询），取"代表灯"：
 * 优先第一盏亮着的灯，其次第一个在线设备，再次第一个设备。
 * 列表项自带 lux 字段时直接用（演示灯都有）；真实设备没有 lux 字段时
 * 调 fetchLatestLux 补一轮。
 *
 * 动效：一次编排的入场（灯先亮、读数随后浮现），之后数据变化走 1.2s
 * 缓慢过渡——5s 轮询不会显得闪。prefers-reduced-motion 由全局样式兜底。
 */

import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useDeviceStore } from '@/stores/deviceStore'

const deviceStore = useDeviceStore()

// ---- 代表灯选择 ----
const heroDevice = computed(() => {
  const list = deviceStore.deviceList
  if (!list || list.length === 0) return null
  return (
    list.find(d => d.lamp === 'ON' && d.status === 'ONLINE') ||
    list.find(d => d.status === 'ONLINE') ||
    list[0]
  )
})

// ---- lux 读数：列表自带 lux 直接用，否则轮询最新光照补 ----
const lux = ref(null)
let luxTimer = null

async function refreshLux() {
  const d = heroDevice.value
  if (!d) return
  if (typeof d.lux === 'number') {
    lux.value = d.lux
    return
  }
  const res = await deviceStore.fetchLatestLux(d.id)
  if (res && typeof res.lux === 'number') lux.value = res.lux
}

watch(heroDevice, refreshLux, { immediate: true })

onMounted(() => {
  luxTimer = setInterval(refreshLux, 5000)
  // 入场编排：先夜空，灯稍后点亮（等一帧让过渡生效）
  requestAnimationFrame(() => requestAnimationFrame(() => { entered.value = true }))
})

onUnmounted(() => {
  if (luxTimer) clearInterval(luxTimer)
})

// ---- 展示状态 ----
const entered = ref(false)                       // 入场编排开关
const lampOn = computed(() => heroDevice.value?.lamp === 'ON')
const threshold = computed(() => deviceStore.thresholdConfig?.threshold ?? 120)

// 灯灭时读数也要可见，只是光晕收起
const luxText = computed(() => (lux.value == null ? '--' : String(Math.round(lux.value))))
</script>

<template>
  <section class="lamp-hero" :class="{ 'is-lit': lampOn, 'has-entered': entered }">
    <!-- 夜空星点（纯 CSS，两层径向渐变随机感） -->
    <div class="stars" aria-hidden="true"></div>

    <div class="hero-inner">
      <!-- 左：路灯 -->
      <div class="lamp-stage">
        <svg viewBox="0 0 320 360" class="lamp-svg" role="img" aria-label="路灯状态示意">
          <defs>
            <radialGradient id="halo" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stop-color="#f2c979" stop-opacity="0.9" />
              <stop offset="35%" stop-color="#e8a33d" stop-opacity="0.45" />
              <stop offset="100%" stop-color="#e8a33d" stop-opacity="0" />
            </radialGradient>
            <linearGradient id="cone" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#f2c979" stop-opacity="0.5" />
              <stop offset="100%" stop-color="#e8a33d" stop-opacity="0" />
            </linearGradient>
            <radialGradient id="pool" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stop-color="#f2c979" stop-opacity="0.55" />
              <stop offset="100%" stop-color="#e8a33d" stop-opacity="0" />
            </radialGradient>
          </defs>

          <!-- 地面 -->
          <line x1="20" y1="332" x2="300" y2="332" stroke="rgba(148,168,200,0.25)" stroke-width="1" />

          <!-- 光层：锥 + 光晕 + 地面光池（亮灯时展开） -->
          <g class="glow-layer">
            <polygon class="light-cone" points="206,122 226,122 282,332 150,332" fill="url(#cone)" />
            <ellipse class="light-pool" cx="216" cy="332" rx="95" ry="9" fill="url(#pool)" />
            <circle class="light-halo" cx="216" cy="118" r="70" fill="url(#halo)" />
          </g>

          <!-- 灯杆与灯臂 -->
          <rect x="86" y="128" width="8" height="204" rx="3" fill="#2a3752" />
          <path d="M90 128 Q90 104 128 104 L212 104" fill="none" stroke="#2a3752" stroke-width="7" stroke-linecap="round" />
          <!-- 灯头 -->
          <rect x="204" y="100" width="26" height="12" rx="5" fill="#2a3752" />
          <circle cx="216" cy="116" r="6" :fill="lampOn ? '#f2c979' : '#3a4a6a'" class="bulb" />
        </svg>
      </div>

      <!-- 右：读数 -->
      <div class="hero-readout">
        <p class="eyebrow">Smart Street Light · 实时</p>
        <h1 class="hero-title">启晖智慧路灯</h1>

        <div class="lux-block">
          <span class="lux-value">{{ luxText }}</span>
          <span class="lux-unit">lux</span>
        </div>
        <p class="lux-label">当前环境照度</p>

        <div class="hero-meta">
          <span class="meta-device" v-if="heroDevice">{{ heroDevice.name || heroDevice.id }}</span>
          <span class="meta-pill" :class="lampOn ? 'on' : 'off'">{{ lampOn ? '灯亮着' : '灯灭着' }}</span>
          <span class="meta-threshold">低于 {{ threshold }} lux 自动开灯</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.lamp-hero {
  position: relative;
  overflow: hidden;
  border-radius: 12px;
  border: 1px solid var(--border-color);
  background:
    radial-gradient(720px 300px at 22% 108%, rgba(232, 163, 61, 0.1), transparent 62%),
    linear-gradient(180deg, #0a101d 0%, #0d1526 58%, #101a2e 100%);
  box-shadow: var(--shadow-md);
  margin-bottom: 24px;
}

/* 星点：两组错位的径向点阵，够暗、不抢戏 */
.stars {
  position: absolute;
  inset: 0;
  background-image:
    radial-gradient(1px 1px at 12% 22%, rgba(234, 228, 211, 0.5) 50%, transparent 51%),
    radial-gradient(1px 1px at 34% 12%, rgba(234, 228, 211, 0.35) 50%, transparent 51%),
    radial-gradient(1.5px 1.5px at 58% 26%, rgba(234, 228, 211, 0.45) 50%, transparent 51%),
    radial-gradient(1px 1px at 74% 10%, rgba(234, 228, 211, 0.3) 50%, transparent 51%),
    radial-gradient(1px 1px at 88% 30%, rgba(234, 228, 211, 0.4) 50%, transparent 51%),
    radial-gradient(1.5px 1.5px at 45% 40%, rgba(79, 179, 200, 0.35) 50%, transparent 51%),
    radial-gradient(1px 1px at 6% 48%, rgba(234, 228, 211, 0.28) 50%, transparent 51%);
  pointer-events: none;
}

.hero-inner {
  position: relative;
  display: flex;
  align-items: center;
  gap: 32px;
  padding: 28px 44px 24px;
}

.lamp-stage {
  flex: 0 0 300px;
}

.lamp-svg {
  display: block;
  width: 100%;
  height: auto;
}

/* ---- 光层：默认收起，亮灯 + 入场后展开 ---- */
.glow-layer {
  opacity: 0;
  transform-origin: 216px 118px;
  transform: scale(0.6);
  transition: opacity 1.2s ease, transform 1.2s ease;
}

.has-entered.is-lit .glow-layer {
  opacity: 1;
  transform: scale(1);
}

.bulb {
  transition: fill 1.2s ease;
}

/* ---- 右侧读数 ---- */
.hero-readout {
  flex: 1;
  min-width: 0;
  opacity: 0;
  transform: translateY(10px);
  transition: opacity 0.9s ease 0.25s, transform 0.9s ease 0.25s;
}

.has-entered .hero-readout {
  opacity: 1;
  transform: none;
}

.eyebrow {
  font-family: var(--font-mono);
  font-size: 11px;
  letter-spacing: 0.3em;
  text-transform: uppercase;
  color: var(--night-cyan);
  margin-bottom: 10px;
}

.hero-title {
  font-family: var(--font-serif);
  font-size: 40px;
  font-weight: 600;
  letter-spacing: 0.08em;
  color: var(--text-primary);
  margin-bottom: 22px;
}

.lux-block {
  display: flex;
  align-items: baseline;
  gap: 10px;
}

.lux-value {
  font-family: var(--font-mono);
  font-size: 64px;
  font-weight: 600;
  line-height: 1;
  color: var(--primary-light);
  text-shadow: 0 0 32px rgba(232, 163, 61, 0.35);
  transition: color 0.6s ease;
}

/* 灯灭时读数退回冷色——光从数字里也撤走 */
.lamp-hero:not(.is-lit) .lux-value {
  color: var(--text-regular);
  text-shadow: none;
}

.lux-unit {
  font-family: var(--font-mono);
  font-size: 16px;
  color: var(--text-secondary);
}

.lux-label {
  margin-top: 8px;
  font-family: var(--font-serif);
  font-size: 15px;
  letter-spacing: 0.2em;
  color: var(--text-secondary);
}

.hero-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 22px;
  font-size: 12px;
  color: var(--text-secondary);
}

.meta-device {
  font-family: var(--font-mono);
  color: var(--text-regular);
}

.meta-pill {
  padding: 2px 12px;
  border-radius: 999px;
  font-weight: 500;
}

.meta-pill.on {
  color: #f2c979;
  background: rgba(232, 163, 61, 0.14);
  box-shadow: inset 0 0 0 1px rgba(232, 163, 61, 0.35);
}

.meta-pill.off {
  color: var(--text-secondary);
  background: rgba(148, 168, 200, 0.08);
  box-shadow: inset 0 0 0 1px rgba(148, 168, 200, 0.2);
}

.meta-threshold {
  font-family: var(--font-mono);
  color: var(--text-placeholder);
}

/* 窄屏：竖排，灯在上 */
@media (max-width: 720px) {
  .hero-inner {
    flex-direction: column;
    gap: 8px;
    padding: 20px;
  }

  .lamp-stage {
    flex-basis: auto;
    width: 200px;
  }

  .hero-title {
    font-size: 30px;
  }

  .lux-value {
    font-size: 48px;
  }
}
</style>
