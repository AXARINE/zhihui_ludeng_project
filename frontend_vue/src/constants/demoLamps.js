// 演示点位（测试用灯）：不接真实硬件，方便功能演示
// 坐标分布在重庆大学虎溪校区周边，部分按路段直线排列
// demo:true 的设备控灯为本地模拟，不调后端
// 使用方式：地图页 / 设备列表页 在真实数据后合并 [...DEMO_LAMPS]
export const DEMO_LAMPS = [
  // 大学城南路：东西向直线排列（同一纬度，经度等距，3 盏）
  { id:'demo_01', name:'演示灯·大学城南路1号', location:'大学城南路', latitude:29.6048, longitude:106.3065, status:'online',  lamp:'on',  mode:'auto',   lux:36,  last_seen_at:new Date().toISOString(), demo:true },
  { id:'demo_02', name:'演示灯·大学城南路2号', location:'大学城南路', latitude:29.6048, longitude:106.3085, status:'online',  lamp:'off', mode:'auto',   lux:210, last_seen_at:new Date().toISOString(), demo:true },
  { id:'demo_03', name:'演示灯·大学城南路3号', location:'大学城南路', latitude:29.6048, longitude:106.3105, status:'online',  lamp:'on',  mode:'manual', lux:15,  last_seen_at:new Date().toISOString(), demo:true },
  // 思贤路：南北向直线排列（同一经度，纬度等距，2 盏；一盏离线用于演示异常告警）
  { id:'demo_04', name:'演示灯·思贤路1号', location:'思贤路', latitude:29.6002, longitude:106.3118, status:'online',  lamp:'off', mode:'auto', lux:160, last_seen_at:new Date().toISOString(), demo:true },
  { id:'demo_05', name:'演示灯·思贤路2号', location:'思贤路', latitude:29.5982, longitude:106.3118, status:'offline', lamp:'off', mode:'auto', lux:0,   last_seen_at:new Date().toISOString(), demo:true },
]
