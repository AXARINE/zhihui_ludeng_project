/**
 * 通知与日报接口
 */
import request from '@/utils/request'

// 发起维修通知（notify:send 权限：市政人员 / 系统管理员）
export function createNotification(data) {
  return request({ url: '/notifications', method: 'post', data })
}

// 当前角色可见的通知列表
export function getNotifications() {
  return request({ url: '/notifications', method: 'get' })
}

// 未读数（顶栏红点）
export function getUnreadCount() {
  return request({ url: '/notifications/unread-count', method: 'get' })
}

// 标记已读
export function markNotificationRead(id) {
  return request({ url: `/notifications/${id}/read`, method: 'post' })
}

// 当日日报（后端懒生成）
export function getTodayReport() {
  return request({ url: '/reports/today', method: 'get' })
}
