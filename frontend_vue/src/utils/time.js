/**
 * 时间格式化工具
 *
 * 后端返回 UTC 时间（如 2026-08-26T03:50:48Z），
 * 前端统一转换为北京时间（UTC+8）显示。
 */

/**
 * 将 UTC 时间字符串转换为北京时间格式化字符串
 * @param {string} utcStr - UTC 时间字符串（如 "2026-08-26T03:50:48Z" 或 "2026-08-26T03:50:48.123456Z"）
 * @param {string} format - 格式：'full' 完整日期时间，'date' 只要日期，'time' 只要时间
 * @returns {string} 北京时间字符串
 */
export function formatBeijingTime(utcStr, format = 'full') {
  if (!utcStr) return ''

  // 解析 UTC 时间
  const date = new Date(utcStr)
  if (isNaN(date.getTime())) return utcStr

  // 转换为北京时间（UTC+8）
  const beijingTime = new Date(date.getTime() + 8 * 60 * 60 * 1000)

  const year = beijingTime.getUTCFullYear()
  const month = String(beijingTime.getUTCMonth() + 1).padStart(2, '0')
  const day = String(beijingTime.getUTCDate()).padStart(2, '0')
  const hours = String(beijingTime.getUTCHours()).padStart(2, '0')
  const minutes = String(beijingTime.getUTCMinutes()).padStart(2, '0')
  const seconds = String(beijingTime.getUTCSeconds()).padStart(2, '0')

  switch (format) {
    case 'date':
      return `${year}-${month}-${day}`
    case 'time':
      return `${hours}:${minutes}:${seconds}`
    case 'full':
    default:
      return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`
  }
}

/**
 * 计算距离现在多久
 * @param {string} utcStr - UTC 时间字符串
 * @returns {string} 如 "3分钟前"、"2小时前"
 */
export function timeAgo(utcStr) {
  if (!utcStr) return '从未'

  const date = new Date(utcStr)
  if (isNaN(date.getTime())) return utcStr

  const now = new Date()
  const diffMs = now - date
  const diffSec = Math.floor(diffMs / 1000)
  const diffMin = Math.floor(diffSec / 60)
  const diffHour = Math.floor(diffMin / 60)
  const diffDay = Math.floor(diffHour / 24)

  if (diffSec < 60) return '刚刚'
  if (diffMin < 60) return `${diffMin}分钟前`
  if (diffHour < 24) return `${diffHour}小时前`
  if (diffDay < 30) return `${diffDay}天前`
  return formatBeijingTime(utcStr)
}
