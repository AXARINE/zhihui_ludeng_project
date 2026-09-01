# 智慧路灯管理系统 - 前端部分

## 项目简介

基于 Vue3 + Element Plus + ECharts 的智慧路灯 IoT 管理系统前端。

## 技术栈

- **Vue3**：核心框架
- **Element Plus**：UI 组件库
- **ECharts**：图表库
- **Pinia**：状态管理
- **Vue Router**：路由管理
- **Axios**：HTTP 请求

## 项目结构

```
frontend/
├── src/
│   ├── api/                # 接口定义
│   ├── components/         # 通用组件
│   ├── mock/               # Mock 数据
│   ├── pages/              # 页面组件
│   ├── router/             # 路由配置
│   ├── stores/             # 状态管理
│   ├── utils/              # 工具函数
│   ├── App.vue             # 根组件
│   ├── main.js             # 应用入口
│   └── style.css           # 全局样式
├── public/                 # 静态资源
├── index.html              # 网页入口
├── package.json            # 依赖配置
└── vite.config.js          # 构建配置
```

## 快速开始

### 安装依赖

```bash
cd frontend
npm install
```

### 启动开发服务器

```bash
npm run dev
```

访问 http://localhost:5173/

### 构建生产版本

```bash
npm run build
```

产物在 `dist/`。要打进发布部署包：复制 `dist/` 内容到 `../deploy/site/`，或直接打 tag 走 CI 自动出包（见仓库根 README 与部署文档 5.4）。

## 主要功能

1. **首页大屏**：设备统计、图表展示
2. **设备列表**：设备管理、状态监控
3. **设备控制**：开灯/关灯、亮度调节
4. **告警管理**：告警列表、处理告警
5. **阈值配置**：光照阈值、自动模式

## 页面说明

| 页面 | 路径 | 功能 |
|------|------|------|
| 首页大屏 | /dashboard | 设备统计、图表展示 |
| 设备列表 | /devices | 设备管理、控制 |
| 设备详情 | /device/:id | 设备详细信息 |
| 告警列表 | /alarms | 告警管理 |
| 阈值配置 | /config | 参数配置 |

## 组件说明

| 组件 | 作用 |
|------|------|
| DeviceCard | 设备卡片，展示设备信息和控制按钮 |
| LightTrendChart | 光照趋势图 |
| DeviceStatusPie | 设备状态饼图 |

## 状态管理

使用 Pinia 管理全局状态：

- **deviceStore**：管理设备数据、告警数据、阈值配置

## 接口说明

前端通过 Axios 调用后端接口，接口定义在 `src/api/device.js`。

开发阶段使用 Mock 数据，后端接口写好后切换真实接口。

## 学习文档

项目包含详细的学习文档，帮助理解代码：

1. 项目整体结构
2. Vue3 核心概念
3. Axios 和接口请求
4. Vue Router 路由
5. ECharts 图表
6. Pinia 状态管理

## 注意事项

1. 开发时使用 Mock 数据，后端接口写好后切换
2. 确保 Node.js 版本 >= 16
3. 首次运行需要安装依赖：`npm install`
