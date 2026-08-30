/**
 * Mock 数据
 *
 * 作用：在前端开发阶段，模拟后端接口返回的数据
 *
 * 为什么要 Mock？
 * 1. 前后端可以并行开发
 * 2. 前端不依赖后端
 * 3. 方便测试
 *
 * 什么时候删除？
 * 等后端接口开发完成后，删除 Mock，改用真实接口
 */

// ============================================
// 设备列表（符合后端响应格式）
// ============================================
// 【注意】这里的格式需要和后端保持一致
// 后端格式：{id, name, status, lamp, mode, last_seen_at, created_at}
export const mockDeviceList = [
  {
    id: 'dev-001',           // 设备ID
    name: '路灯001',          // 设备名称
    status: 'ONLINE',        // 设备状态
    lamp: 'ON',              // 灯状态
    mode: 'AUTO',            // 工作模式
    last_seen_at: '2026-08-22T10:30:00Z',
    created_at: '2026-08-01T00:00:00Z'
  },
  {
    id: 'dev-002',
    name: '路灯002',
    status: 'OFFLINE',
    lamp: 'OFF',
    mode: 'MANUAL',
    last_seen_at: '2026-08-20T18:45:00Z',
    created_at: '2026-08-01T00:00:00Z'
  },
  {
    id: 'dev-003',
    name: '路灯003',
    status: 'ONLINE',
    lamp: 'ON',
    mode: 'AUTO',
    last_seen_at: '2026-08-22T09:15:00Z',
    created_at: '2026-08-02T00:00:00Z'
  },
  {
    id: 'dev-004',
    name: '路灯004',
    status: 'FAULT',
    lamp: 'OFF',
    mode: 'MANUAL',
    last_seen_at: '2026-08-21T14:20:00Z',
    created_at: '2026-08-02T00:00:00Z'
  }
]

// ============================================
// 光照数据（符合后端响应格式）
// ============================================
// 【注意】后端格式：{device_id, lux, timestamp}
export const mockLightData = [
  { device_id: 'dev-001', lux: 250, timestamp: '2026-08-22T10:30:00Z' },
  { device_id: 'dev-001', lux: 280, timestamp: '2026-08-22T10:35:00Z' },
  { device_id: 'dev-001', lux: 300, timestamp: '2026-08-22T10:40:00Z' },
  { device_id: 'dev-001', lux: 320, timestamp: '2026-08-22T10:45:00Z' },
  { device_id: 'dev-001', lux: 290, timestamp: '2026-08-22T10:50:00Z' }
]

// ============================================
// 告警列表（符合后端响应格式）
// ============================================
// 【注意】后端格式：{id, device_id, message, created_at, resolved_at}
export const mockAlarmList = [
  {
    id: 1,
    device_id: 'dev-001',
    message: '设备离线告警',
    created_at: '2026-08-22T10:30:00Z',
    resolved_at: null  // 未解决
  },
  {
    id: 2,
    device_id: 'dev-002',
    message: '设备故障告警',
    created_at: '2026-08-21T18:45:00Z',
    resolved_at: null  // 未解决
  },
  {
    id: 3,
    device_id: 'dev-003',
    message: '光照过低告警',
    created_at: '2026-08-20T09:15:00Z',
    resolved_at: '2026-08-20T10:00:00Z'  // 已解决
  }
]

// ============================================
// 阈值配置（符合后端响应格式）
// ============================================
// 【注意】后端格式：{device_id, threshold}
export const mockThresholdConfig = {
  device_id: 'dev-001',
  threshold: 40.0
}

// ============================================
// 地图点位（符合后端 /api/map/devices 响应格式）
// ============================================
// 【注意】后端坐标是 WGS84，状态/灯态/模式是小写
export const mockMapDeviceList = [
  {
    id: 'dev-001',
    name: '路灯001',
    location: '东川路',
    latitude: 31.024,          // WGS84 纬度
    longitude: 121.437,        // WGS84 经度
    status: 'online',
    lamp: 'on',
    mode: 'auto',
    lux: 12,
    last_seen_at: '2026-08-22T10:30:00Z'
  },
  {
    id: 'dev-002',
    name: '路灯002',
    location: '思源路',
    latitude: 31.021,
    longitude: 121.442,
    status: 'offline',
    lamp: 'off',
    mode: 'manual',
    lux: 35,
    last_seen_at: '2026-08-20T18:45:00Z'
  },
  {
    id: 'dev-003',
    name: '路灯003',
    location: '宣桥路',
    latitude: 31.028,
    longitude: 121.445,
    status: 'online',
    lamp: 'off',
    mode: 'auto',
    lux: 260,
    last_seen_at: '2026-08-22T09:15:00Z'
  },
  {
    id: 'dev-004',
    name: '路灯004（未定位）',
    location: '',
    latitude: null,
    longitude: null,
    status: 'online',
    lamp: 'on',
    mode: 'manual',
    lux: 8,
    last_seen_at: '2026-08-21T14:20:00Z'
  }
]

// ============================================
// 模拟响应（用于 Mock 接口）
// ============================================
// 【作用】模拟后端成功响应
export function mockResponse(data) {
  return new Promise((resolve) => {
    setTimeout(() => {
      resolve(data)
    }, 500)  // 延迟 500ms，模拟网络请求
  })
}

// 【作用】模拟后端失败响应
export function mockErrorResponse(message = '请求失败') {
  return new Promise((resolve, reject) => {
    setTimeout(() => {
      reject(new Error(message))
    }, 500)
  })
}
