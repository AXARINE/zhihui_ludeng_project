/**
 * 设备状态 Store
 *
 * 作用：管理设备数据，让多个页面共享同一份数据
 *
 * 为什么用 Pinia？
 * 1. 多个页面需要共享数据
 * 2. 一个页面修改数据，其他页面自动更新
 * 3. 数据集中管理，便于维护
 *
 * 使用方式：
 * import { useDeviceStore } from '@/stores/deviceStore'
 * const deviceStore = useDeviceStore()
 * deviceStore.fetchDeviceList()
 */

// 导入 Pinia
import { defineStore } from 'pinia'

// 导入接口
import {
  getDeviceList,
  controlLamp,
  getThreshold,
  setThreshold,
  getDimming,
  setDimming,
  getAlarmList,
  getLatestLux,
  getLuxHistory
} from '@/api/device'

// 导入 Mock 数据（开发阶段使用）
import {
  mockDeviceList,
  mockAlarmList,
  mockThresholdConfig,
  mockDimmingConfig,
  mockResponse
} from '@/mock/device'

// 演示设备（测试用灯，不接真实硬件）—— 全局共享，所有模块可见
import { DEMO_LAMPS } from '@/constants/demoLamps'

// 演示告警（配合演示灯展示：demo_05 离线）
const DEMO_ALARMS = [
  {
    id: -2,
    device_id: 'demo_05',
    type: 'offline',
    message: '演示灯·思贤路2号 设备离线（演示告警）',
    created_at: new Date(Date.now() - 3600 * 1000).toISOString(),
    resolved_at: null
  }
]

// ============================================
// 开发模式标志
// ============================================
// 【重要】true = 使用 Mock 数据，false = 使用真实后端接口
//
// 由 .env 文件中的 VITE_USE_MOCK 控制，不用改代码：
//   VITE_USE_MOCK=true   → 用假数据（板子不在手边时开发界面）
//   VITE_USE_MOCK=false  → 调真实后端 localhost:8080
// 改完 .env 需要重启 npm run dev 才生效
const USE_MOCK = import.meta.env.VITE_USE_MOCK !== 'false'

// ============================================
// 数据归一化（重要，踩过坑）
// ============================================
// 后端数据库存的是小写（'online' / 'on' / 'auto'），
// 但前端组件里判断的是大写（'ONLINE' / 'ON' / 'AUTO'）。
// 如果不转换，接上真实后端后所有状态判断都会失效，
// 界面上会显示成"离线、灯关着"，看起来像后端没数据。
//
// 所以在数据进 store 的入口统一转成大写，
// 数据库那边不用动，前端组件也不用动。
function normalizeDevice(d) {
  return {
    ...d,
    status: (d.status || 'offline').toUpperCase(),  // online → ONLINE
    lamp: (d.lamp || 'off').toUpperCase(),          // on → ON
    mode: (d.mode || 'auto').toUpperCase()          // auto → AUTO
  }
}

// ============================================
// 定义设备 Store
// ============================================
export const useDeviceStore = defineStore('device', {
  // ============================================
  // 1. State：存储数据（类似于组件的 data）
  // ============================================
  state: () => ({
    // 设备列表
    deviceList: [],

    // 演示灯运行时状态（全局唯一真源：MapPage / 首页大屏 / 设备列表共享同一批对象，
    // 任何页面控灯后其他页面自动同步）
    demoDevices: DEMO_LAMPS.map(normalizeDevice),

    // 告警列表
    alarmList: [],

    // 阈值配置
    thresholdConfig: {
      threshold: 40.0
    },

    // 调光配置（手动亮度 + 照度-亮度曲线）
    dimmingConfig: {
      brightness: 100,
      dim_curve: ''
    },

    // 演示灯的调光配置（键 = 设备 ID；演示灯不调后端，本地保存）
    demoDimming: {},

    // 加载状态
    loading: false,

    // 当前操作的设备ID
    currentDeviceId: null,

    // 轮询定时器
    _pollTimer: null,

    // 上次发送命令的时间戳（用于防止轮询覆盖乐观更新）
    _lastCommandTime: 0
  }),

  // ============================================
  // 2. Getters：计算属性（类似于组件的 computed）
  // ============================================
  getters: {
    // 设备总数
    deviceTotal: (state) => state.deviceList.length,

    // 在线设备数
    onlineCount: (state) => {
      return state.deviceList.filter(d => d.status === 'ONLINE').length
    },

    // 离线设备数
    offlineCount: (state) => {
      return state.deviceList.filter(d => d.status === 'OFFLINE').length
    },

    // 故障设备数
    faultCount: (state) => {
      return state.deviceList.filter(d => d.status === 'FAULT').length
    },

    // 开灯数量
    lampOnCount: (state) => {
      return state.deviceList.filter(d => d.lamp === 'ON').length
    },

    // 关灯数量
    lampOffCount: (state) => {
      return state.deviceList.filter(d => d.lamp === 'OFF').length
    },

    // 自动模式设备数
    autoModeCount: (state) => {
      return state.deviceList.filter(d => d.mode === 'AUTO').length
    },

    // 手动模式设备数
    manualModeCount: (state) => {
      return state.deviceList.filter(d => d.mode === 'MANUAL').length
    },

    // 待处理告警数
    pendingAlarmCount: (state) => {
      return state.alarmList.filter(a => a.resolved_at === null).length
    },

    // 根据ID获取设备
    getDeviceById: (state) => {
      return (deviceId) => state.deviceList.find(d => d.id === deviceId)
    }
  },

  // ============================================
  // 3. Actions：方法（类似于组件的 methods）
  // ============================================
  actions: {
    // ============================================
    // 获取设备列表
    // ============================================
    async fetchDeviceList() {
      // 如果刚发了命令（8秒内），跳过本次轮询，避免旧数据覆盖乐观更新
      if (this._lastCommandTime && Date.now() - this._lastCommandTime < 8000) {
        console.log('命令保护期内，跳过轮询')
        return
      }

      this.loading = true
      try {
        let res

        if (USE_MOCK) {
          // 【开发阶段】使用 Mock 数据
          console.log('使用 Mock 数据')
          res = await mockResponse(mockDeviceList)
        } else {
          // 【生产阶段】调用后端接口
          res = await getDeviceList()
        }

        // 【关键】统一转大写后再存进 store；合并演示灯（引用同一批对象，全局同步）
        this.deviceList = [...(res || []).map(normalizeDevice), ...this.demoDevices]
        console.log('设备列表加载成功：', this.deviceList)
      } catch (error) {
        console.log('设备列表加载失败：', error)
      } finally {
        this.loading = false
      }
    },

    // ============================================
    // 获取告警列表
    // ============================================
    async fetchAlarmList(params = {}) {
      this.loading = true
      try {
        let res

        if (USE_MOCK) {
          // 【开发阶段】使用 Mock 数据
          console.log('使用 Mock 告警数据')
          res = await mockResponse(mockAlarmList)
        } else {
          // 【生产阶段】调用后端接口
          res = await getAlarmList(params)
        }

        // 合并演示告警（配合演示灯，展示在告警列表）
        this.alarmList = [...DEMO_ALARMS, ...(res || [])]
        console.log('告警列表加载成功：', this.alarmList)
      } catch (error) {
        console.log('告警列表加载失败：', error)
      } finally {
        this.loading = false
      }
    },

    // ============================================
    // 获取阈值配置
    // ============================================
    async fetchThresholdConfig(deviceId) {
      this.loading = true
      try {
        let res

        if (USE_MOCK) {
          // 【开发阶段】使用 Mock 数据
          console.log('使用 Mock 阈值配置')
          res = await mockResponse(mockThresholdConfig)
        } else {
          // 【生产阶段】调用后端接口
          res = await getThreshold(deviceId)
        }

        this.thresholdConfig = res
        console.log('阈值配置加载成功：', this.thresholdConfig)
      } catch (error) {
        console.log('阈值配置加载失败：', error)
      } finally {
        this.loading = false
      }
    },

    // ============================================
    // 控制设备（开灯/关灯/自动）
    // ============================================
    async controlDevice(deviceId, action) {
      try {
        console.log('控制设备：', deviceId, action)

        // 演示灯：本地模拟控制（改全局 demoDevices，所有页面同步）
        const demoDev = this.demoDevices.find(d => d.id === deviceId)
        if (demoDev) {
          if (action === 'on') { demoDev.lamp = 'ON'; demoDev.mode = 'MANUAL' }
          else if (action === 'off') { demoDev.lamp = 'OFF'; demoDev.mode = 'MANUAL' }
          else if (action === 'auto') { demoDev.mode = 'AUTO' }
          return { success: true, message: '演示灯控制成功（本地模拟）' }
        }

        if (USE_MOCK) {
          // 【开发阶段】模拟控制成功
          await mockResponse({ success: true })
        } else {
          // 【生产阶段】调用后端接口
          await controlLamp(deviceId, action)
        }

        // 记录命令发送时间，防止轮询用旧数据覆盖乐观更新
        this._lastCommandTime = Date.now()

        // 【重要】使用 $patch 确保响应式更新
        this.$patch((state) => {
          const device = state.deviceList.find(d => d.id === deviceId)
          if (device) {
            if (action === 'on') {
              device.lamp = 'ON'
              device.mode = 'MANUAL'
            } else if (action === 'off') {
              device.lamp = 'OFF'
              device.mode = 'MANUAL'
            } else if (action === 'auto') {
              device.mode = 'AUTO'
            }
          }
        })

        console.log('设备状态更新完成')

        // 等板子报回真实状态再刷新（板子每5秒上报，命令执行需约6-8秒）
        if (!USE_MOCK) {
          setTimeout(() => {
            this._lastCommandTime = 0
            this.fetchDeviceList()
          }, 8000)
        }

        return { success: true, message: '控制成功' }
      } catch (error) {
        console.log('控制失败：', error)
        return { success: false, message: '控制失败' }
      }
    },

    // ============================================
    // 保存阈值配置
    // ============================================
    async saveThresholdConfig(deviceId, threshold) {
      try {
        console.log('保存阈值配置：', deviceId, threshold)

        if (USE_MOCK) {
          // 【开发阶段】模拟保存成功
          await mockResponse({ success: true })
        } else {
          // 【生产阶段】调用后端接口
          await setThreshold(deviceId, threshold)
        }

        // 更新本地数据
        this.thresholdConfig.threshold = threshold

        return { success: true, message: '保存成功' }
      } catch (error) {
        console.log('保存失败：', error)
        return { success: false, message: '保存失败' }
      }
    },

    // ============================================
    // 获取调光配置（手动亮度 + 照度-亮度曲线）
    // ============================================
    async fetchDimmingConfig(deviceId) {
      this.loading = true
      try {
        let res

        if (USE_MOCK) {
          // 【开发阶段】使用 Mock 数据
          console.log('使用 Mock 调光配置')
          res = await mockResponse(mockDimmingConfig)
        } else {
          // 【生产阶段】调用后端接口
          res = await getDimming(deviceId)
        }

        this.dimmingConfig = {
          brightness: res?.brightness ?? 100,
          dim_curve: res?.dim_curve ?? ''
        }
        console.log('调光配置加载成功：', this.dimmingConfig)
      } catch (error) {
        console.log('调光配置加载失败：', error)
      } finally {
        this.loading = false
      }
    },

    // ============================================
    // 保存调光配置（亮度 / 曲线，至少一项）
    // ============================================
    async saveDimmingConfig(deviceId, data) {
      try {
        console.log('保存调光配置：', deviceId, data)

        // 演示灯：本地模拟（不调后端，避免 404），按设备分别记录
        const demoDev = this.demoDevices.find(d => d.id === deviceId)
        if (demoDev) {
          const cur = this.demoDimming[deviceId] || {}
          if (data.brightness != null) cur.brightness = data.brightness
          if (data.dim_curve != null) cur.dim_curve = data.dim_curve
          this.demoDimming[deviceId] = cur
          this.dimmingConfig = { brightness: cur.brightness ?? 100, dim_curve: cur.dim_curve ?? '' }
          return { success: true, message: '演示灯调光配置已保存（本地模拟）' }
        }

        if (USE_MOCK) {
          // 【开发阶段】模拟保存成功
          await mockResponse({ success: true })
        } else {
          // 【生产阶段】调用后端接口
          await setDimming(deviceId, data)
        }

        // 更新本地数据
        if (data.brightness != null) this.dimmingConfig.brightness = data.brightness
        if (data.dim_curve != null) this.dimmingConfig.dim_curve = data.dim_curve

        return { success: true, message: '保存成功' }
      } catch (error) {
        console.log('保存调光配置失败：', error)
        const detail = error?.response?.data
        return { success: false, message: typeof detail === 'string' ? detail : '保存失败' }
      }
    },

    // ============================================
    // 获取最新光照数据
    // ============================================
    async fetchLatestLux(deviceId) {
      try {
        if (USE_MOCK) {
          // 【开发阶段】返回模拟数据
          return await mockResponse({
            device_id: deviceId,
            lux: Math.floor(Math.random() * 100) + 200,
            timestamp: new Date().toISOString()
          })
        } else {
          // 【生产阶段】调用后端接口
          return await getLatestLux(deviceId)
        }
      } catch (error) {
        console.log('获取光照数据失败：', error)
        return null
      }
    },

    // ============================================
    // 获取历史光照数据
    // ============================================
    async fetchLuxHistory(deviceId, params = {}) {
      try {
        if (USE_MOCK) {
          // 【开发阶段】返回模拟数据
          const mockHistory = Array.from({ length: 10 }, (_, i) => ({
            device_id: deviceId,
            lux: Math.floor(Math.random() * 100) + 200,
            timestamp: new Date(Date.now() - i * 3600000).toISOString()
          }))
          return await mockResponse(mockHistory)
        } else {
          // 【生产阶段】调用后端接口
          return await getLuxHistory(deviceId, params)
        }
      } catch (error) {
        console.log('获取历史数据失败：', error)
        return []
      }
    },

    // ============================================
    // 设置当前操作的设备ID
    // ============================================
    setCurrentDeviceId(deviceId) {
      this.currentDeviceId = deviceId
    },

    // ============================================
    // 启动自动轮询（每5秒刷新设备状态）
    // ============================================
    startPolling() {
      if (this._pollTimer) return // 已经在轮询了
      console.log('启动设备状态轮询（每5秒）')
      this.fetchDeviceList() // 立即拉一次
      this._pollTimer = setInterval(() => {
        this.fetchDeviceList()
      }, 5000)
    },

    // ============================================
    // 停止轮询
    // ============================================
    stopPolling() {
      if (this._pollTimer) {
        console.log('停止设备状态轮询')
        clearInterval(this._pollTimer)
        this._pollTimer = null
      }
    }
  }
})
