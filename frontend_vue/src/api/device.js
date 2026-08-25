/**
 * 设备相关接口
 *
 * 作用：定义所有和设备相关的接口
 * 好处：接口集中管理，修改方便
 *
 * 后端 API 地址：http://localhost:8080/api
 *
 * 使用方式：
 * import { getDeviceList, controlDevice } from '@/api/device'
 * const list = await getDeviceList()
 */

// 导入封装好的 axios
import request from '@/utils/request'

// ============================================
// 登录
// POST /api/auth/login
// ============================================
export function login(username, password) {
  return request({
    url: '/auth/login',
    method: 'post',
    data: { username, password }
  })
}

// ============================================
// 获取角色列表
// GET /api/roles
// ============================================
export function getRoles() {
  return request({ url: '/roles', method: 'get' })
}

// ============================================
// 获取账号列表
// GET /api/users
// ============================================
export function getUsers() {
  return request({ url: '/users', method: 'get' })
}

// ============================================
// 创建账号
// POST /api/users
// ============================================
export function createUser(data) {
  return request({ url: '/users', method: 'post', data })
}

// ============================================
// 删除账号
// DELETE /api/users/{id}
// ============================================
export function deleteUser(id) {
  return request({ url: `/users/${id}`, method: 'delete' })
}

// ============================================
// 获取审计日志
// GET /api/commands
// ============================================
export function getCommands(params) {
  return request({ url: '/commands', method: 'get', params })
}

// ============================================
// 告警标记已处理
// POST /api/alarms/{id}/resolve
// ============================================
export function resolveAlarm(id) {
  return request({ url: `/alarms/${id}/resolve`, method: 'post' })
}

// ============================================
// 告警恢复未处理
// POST /api/alarms/{id}/unresolve
// ============================================
export function unresolveAlarm(id) {
  return request({ url: `/alarms/${id}/unresolve`, method: 'post' })
}

// ============================================
// 智能问答
// POST /api/assistant/ask
// ============================================
export function askAssistant(question) {
  return request({ url: '/assistant/ask', method: 'post', data: { question } })
}

// ============================================
// 1. 获取设备列表
// GET /api/devices
// ============================================
export function getDeviceList() {
  return request({
    url: '/devices',
    method: 'get'
  })
}

// ============================================
// 2. 创建设备
// POST /api/devices
// ============================================
export function createDevice(data) {
  return request({
    url: '/devices',
    method: 'post',
    data
  })
  // data 的格式：
  // {
  //   id: '001',
  //   name: '智慧路灯1号'  // 可选
  // }
}

// ============================================
// 3. 删除设备
// DELETE /api/devices/{id}
// ============================================
export function deleteDevice(deviceId) {
  return request({
    url: `/devices/${deviceId}`,
    method: 'delete'
  })
}

// ============================================
// 4. 获取最新光照数据
// GET /api/devices/{id}/lux/latest
// ============================================
export function getLatestLux(deviceId) {
  return request({
    url: `/devices/${deviceId}/lux/latest`,
    method: 'get'
  })
}

// ============================================
// 5. 获取历史光照数据
// GET /api/devices/{id}/lux/history?from=&to=
// ============================================
export function getLuxHistory(deviceId, params) {
  return request({
    url: `/devices/${deviceId}/lux/history`,
    method: 'get',
    params
  })
  // params 的格式：
  // {
  //   from: '2024-01-01T00:00:00Z',  // 可选，RFC3339 格式
  //   to: '2024-01-15T23:59:59Z'      // 可选，RFC3339 格式
  // }
}

// ============================================
// 6. 控制灯（开灯/关灯/自动）
// POST /api/devices/{id}/lamp
// ============================================
export function controlLamp(deviceId, action) {
  return request({
    url: `/devices/${deviceId}/lamp`,
    method: 'post',
    data: { action }
  })
  // action 的值：
  // 'on'   - 开灯
  // 'off'  - 关灯
  // 'auto' - 自动模式
}

// ============================================
// 7. 获取阈值配置
// GET /api/devices/{id}/threshold
// ============================================
export function getThreshold(deviceId) {
  return request({
    url: `/devices/${deviceId}/threshold`,
    method: 'get'
  })
}

// ============================================
// 8. 设置阈值配置
// PUT /api/devices/{id}/threshold
// ============================================
export function setThreshold(deviceId, threshold) {
  return request({
    url: `/devices/${deviceId}/threshold`,
    method: 'put',
    data: { threshold }
  })
  // threshold: 光照阈值（数字类型）
}

// ============================================
// 9. 获取告警列表
// GET /api/alarms?device_id=&resolved=
// ============================================
export function getAlarmList(params) {
  return request({
    url: '/alarms',
    method: 'get',
    params
  })
  // params 的格式：
  // {
  //   device_id: '001',     // 可选，按设备筛选
  //   resolved: true/false  // 可选，按状态筛选
  // }
}
