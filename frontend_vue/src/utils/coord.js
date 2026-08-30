/**
 * 坐标系转换工具 — WGS84 → GCJ-02
 *
 * 背景（踩坑说明）：
 * - 后端设备坐标统一存 WGS84（GPS 原始坐标系，注册设备时传入）
 * - 高德/腾讯底图是 GCJ-02（"火星坐标"，国家测绘加密偏移）
 * - 如果直接把 WGS84 坐标打在 GCJ-02 底图上，点位会整体偏移
 *   100~700 米（往西南方向飘），看起来路灯"漂"到马路边
 * - 所以前端渲染前必须做一次 WGS84 → GCJ-02 转换
 *
 * 算法是业界通行的公开近似逆推公式（误差 ≈ 1~2 米，打点足够）。
 * 中国境外没有偏移，直接原样返回。
 */

const PI = Math.PI
const A = 6378245.0                // 长半轴（克拉索夫斯基椭球）
const EE = 0.00669342162296594323  // 偏心率平方

/**
 * 判断坐标是否在中国境外（境外无 GCJ-02 偏移）
 */
function outOfChina(lng, lat) {
  return lng < 72.004 || lng > 137.8347 || lat < 0.8293 || lat > 55.8271
}

function transformLat(x, y) {
  let ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * Math.sqrt(Math.abs(x))
  ret += (20.0 * Math.sin(6.0 * x * PI) + 20.0 * Math.sin(2.0 * x * PI)) * 2.0 / 3.0
  ret += (20.0 * Math.sin(y * PI) + 40.0 * Math.sin(y / 3.0 * PI)) * 2.0 / 3.0
  ret += (160.0 * Math.sin(y / 12.0 * PI) + 320 * Math.sin(y * PI / 30.0)) * 2.0 / 3.0
  return ret
}

function transformLng(x, y) {
  let ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * Math.sqrt(Math.abs(x))
  ret += (20.0 * Math.sin(6.0 * x * PI) + 20.0 * Math.sin(2.0 * x * PI)) * 2.0 / 3.0
  ret += (20.0 * Math.sin(x * PI) + 40.0 * Math.sin(x / 3.0 * PI)) * 2.0 / 3.0
  ret += (150.0 * Math.sin(x / 12.0 * PI) + 300.0 * Math.sin(x / 30.0 * PI)) * 2.0 / 3.0
  return ret
}

/**
 * WGS84 坐标转 GCJ-02 坐标
 * @param {number} lng - WGS84 经度
 * @param {number} lat - WGS84 纬度
 * @returns {{lng: number, lat: number}} GCJ-02 坐标
 */
export function wgs84ToGcj02(lng, lat) {
  if (outOfChina(lng, lat)) return { lng, lat }

  let dLat = transformLat(lng - 105.0, lat - 35.0)
  let dLng = transformLng(lng - 105.0, lat - 35.0)
  const radLat = (lat / 180.0) * PI
  let magic = Math.sin(radLat)
  magic = 1 - EE * magic * magic
  const sqrtMagic = Math.sqrt(magic)
  dLat = (dLat * 180.0) / (((A * (1 - EE)) / (magic * sqrtMagic)) * PI)
  dLng = (dLng * 180.0) / (A / sqrtMagic * Math.cos(radLat) * PI)
  return { lng: lng + dLng, lat: lat + dLat }
}

/**
 * GCJ-02 坐标近似转回 WGS84（一次迭代逼近，误差 ≈ 1~2 米）
 *
 * 用途：高德/腾讯坐标拾取器给出的都是 GCJ-02，粘进"设备编辑"对话框时
 * 先转成 WGS84 再入库（后端约定统一存 WGS84）。
 *
 * @param {number} lng - GCJ-02 经度
 * @param {number} lat - GCJ-02 纬度
 * @returns {{lng: number, lat: number}} WGS84 坐标
 */
export function gcj02ToWgs84(lng, lat) {
  if (outOfChina(lng, lat)) return { lng, lat }
  const gcj = wgs84ToGcj02(lng, lat)
  return { lng: lng * 2 - gcj.lng, lat: lat * 2 - gcj.lat }
}
