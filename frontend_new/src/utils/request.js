/**
 * Axios 封装文件
 *
 * 作用：统一处理前端和后端的 HTTP 请求
 *
 * 为什么要封装？
 * 1. 统一配置（baseURL、超时时间）
 * 2. 统一处理错误
 * 3. 统一添加 token（如果需要登录）
 */

// 导入 axios 库（发请求的工具）
import axios from 'axios'

// ============================================
// 1. 创建 axios 实例（就像创建一个专门发请求的"机器人"）
// ============================================
const service = axios.create({
  // baseURL：后端接口的基础地址
  // 从 .env 文件的 VITE_API_BASE 读，读不到就用默认值 '/api'
  //
  // 注意：这里写相对路径 '/api'（不是 http://localhost:8080/api），
  // 配合 vite.config.js 里的代理，请求走前端自己的 5173 端口，
  // 由 Vite 转发给后端，这样就不会被浏览器跨域拦截。
  // 好处：以后后端换地址（比如部署到服务器）只改 .env，不用动代码
  baseURL: import.meta.env.VITE_API_BASE || '/api',

  // timeout：请求超时时间（毫秒）
  // 如果请求超过 10 秒还没响应，就报错
  timeout: 10000
})

// ============================================
// 2. 请求拦截器（发请求之前做什么）
// ============================================
service.interceptors.request.use(
  // config：请求的配置信息
  (config) => {
    // 这里可以做一些"发请求之前"的事情
    // 比如：添加 token（登录凭证）

    // 自动添加 JWT token
    const token = localStorage.getItem('token')
    if (token) {
      config.headers['Authorization'] = `Bearer ${token}`
    }

    console.log('发送请求：', config.url)
    return config
  },
  // 错误处理
  (error) => {
    console.log('请求出错：', error)
    return Promise.reject(error)
  }
)

// ============================================
// 3. 响应拦截器（收到响应之后做什么）
// ============================================
service.interceptors.response.use(
  // response：后端返回的响应数据
  (response) => {
    console.log('收到响应：', response.data)

    // ============================================
    // 【重要】这里原来写错了，是个很隐蔽的坑
    // ============================================
    // 原来的代码要求后端返回 { code: 200, data: [...] } 这种包装格式，
    // 不满足 code === 200 就当失败。
    //
    // 但我们这个 Rust 后端（axum 框架）不做包装，它直接返回裸数据：
    //   GET /api/devices  →  [{ "id": "xxx", "name": "路灯1" }]
    //   而不是            →  { "code": 200, "data": [...] }
    //
    // 所以 res.code 是 undefined，条件永远不成立，
    // 每个请求都会被判定为失败，界面上一个设备都不显示。
    //
    // 正确做法：HTTP 状态码 2xx 就是成功（axios 已经帮我们判断过了，
    // 非 2xx 会直接进下面的错误分支），这里直接把数据返回。
    return response.data
  },
  // 错误处理（网络错误、超时等）
  (error) => {
    console.log('响应出错：', error.message)

    // 根据错误状态码做不同处理
    if (error.response) {
      // 服务器返回了错误状态码
      switch (error.response.status) {
        case 401:
          console.log('未授权，请登录')
          localStorage.removeItem('token')
          localStorage.removeItem('user')
          window.location.href = '/login'
          break
        case 404:
          console.log('接口不存在')
          break
        case 500:
          console.log('服务器内部错误')
          break
        default:
          console.log('其他错误：', error.response.status)
      }
    } else if (error.code === 'ECONNABORTED') {
      console.log('请求超时')
    } else {
      console.log('网络错误')
    }

    return Promise.reject(error)
  }
)

// ============================================
// 4. 导出这个"发请求的机器人"
// ============================================
// 其他文件导入后就可以用 service.get() 或 service.post() 发请求了
export default service
