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
// 更新账号信息
// PATCH /api/users/{id}
// ============================================
export function updateUser(id, data) {
  return request({ url: `/users/${id}`, method: 'patch', data })
}

// ============================================
// 获取审计日志
// GET /api/commands
// ============================================
export function getCommands(params) {
  return request({ url: '/commands', method: 'get', params })
}

// ============================================
// 更新告警状态（标记已处理 / 恢复未处理）
// PATCH /api/alarms/{id}
// 后端用一个 PATCH 接口统一处理，通过 body 传 resolved 字段
// ============================================
export function resolveAlarm(id) {
  return request({ url: `/alarms/${id}`, method: 'patch', data: { resolved: true } })
}

export function unresolveAlarm(id) {
  return request({ url: `/alarms/${id}`, method: 'patch', data: { resolved: false } })
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

// ============================================
// 10. 更新设备信息
// PATCH /api/devices/{id}
// ============================================
export function updateDevice(deviceId, data) {
  return request({
    url: `/devices/${deviceId}`,
    method: 'patch',
    data
  })
  // data 的格式：
  // {
  //   name: '新名称',       // 可选
  //   status: 'online'      // 可选
  // }
}

// ============================================
// 11. 获取当前登录用户信息
// GET /api/auth/me
// ============================================
export function getMe() {
  return request({ url: '/auth/me', method: 'get' })
}

// ============================================
// 12. 获取权限列表
// GET /api/permissions
// ============================================
export function getPermissions() {
  return request({ url: '/permissions', method: 'get' })
}

// ============================================
// 13. 获取角色当前拥有的权限 ID 列表
// GET /api/roles/{id}/permissions
// ============================================
export function getRolePermissions(roleId) {
  return request({ url: `/roles/${roleId}/permissions`, method: 'get' })
}

// ============================================
// 14. 更新角色权限
// PUT /api/roles/{id}/permissions
// ============================================
export function updateRolePermissions(roleId, permissionIds) {
  return request({
    url: `/roles/${roleId}/permissions`,
    method: 'put',
    data: { permission_ids: permissionIds }
  })
  // permissionIds: 权限 ID 数组，例如 [1, 2, 3]
}

// ============================================
// 14. 获取仪表盘数据
// GET /api/dashboard
// ============================================
export function getDashboard() {
  return request({ url: '/dashboard', method: 'get' })
}

// ============================================
// 15. 获取光照统计数据
// GET /api/devices/{id}/lux/stats
// ============================================
export function getLuxStats(deviceId, params) {
  return request({
    url: `/devices/${deviceId}/lux/stats`,
    method: 'get',
    params
  })
}

// ============================================
// 16. 获取单设备命令日志
// GET /api/devices/{id}/commands
// ============================================
export function getDeviceCommands(deviceId, params) {
  return request({
    url: `/devices/${deviceId}/commands`,
    method: 'get',
    params
  })
}

// ============================================
// 17. 获取全局最新光照数据（所有设备）
// GET /api/lux/latest
// ============================================
export function getGlobalLuxLatest() {
  return request({ url: '/lux/latest', method: 'get' })
}

// ============================================
// 18. 健康检查
// GET /api/health
// ============================================
export function getHealth() {
  return request({ url: '/health', method: 'get' })
}
