# 智慧路灯管理系统 · 前端

基于 **BearPi-HM Nano + 华为云 IoTDA** 的智慧路灯 Web 管理端（纯静态单页，零构建依赖）。

## 快速开始

1. **启动后端**（Rust 版，监听 `8080`，含 JWT 登录）：
   ```bash
   # Windows Git Bash（需先启动 Docker Desktop）
   cd ../backend
   set -a; source .env; set +a
   cargo run
   ```
   或使用仓库根目录的一键脚本：`./run.sh`

2. **打开页面**：直接用浏览器打开本目录的 `index.html`（`file://` 即可，后端已放开 CORS）。

3.**登录**：默认管理员账号 `admin / admin123`（首次启动后端自动创建，可用环境变量 `BOOTSTRAP_ADMIN_USERNAME` / `BOOTSTRAP_ADMIN_PASSWORD` 覆盖）。
**登录**：系统管理员账号 `superadmin / superadmin123`（拥有全部权限，包括账号管理权限，可用环境变量 `BOOTSTRAP_ADMIN_USERNAME` / `BOOTSTRAP_ADMIN_PASSWORD` 覆盖）。

## 功能模块

| 模块 | 说明 |
| --- | --- |
| 📊 仪表盘 | 实时光照/灯态/模式/设备状态/上报时间 5 指标卡 + 光照趋势图（1h/6h/24h）+ 近 7 天告警趋势柱状图 + 设备在线概览环形图 + 历史光照数据（按小时 · 最近 24 小时，可展开） |
| 💡 设备管理 | 设备下拉切换、手动控灯（开/关/自动）、光照联动阈值下发、设备列表（在线状态/灯态/模式） |
| 🚨 告警管理 | 告警记录列表（未处理/已消解状态标识） |
| 👤 账号管理 | 账号增删（市政人员 / 路灯管理员两种角色，需 `user:manage` 权限，默认仅 admin） |
| 🔐 权限管理 | 按模块勾选角色拥有的功能权限，保存即生效（权限在服务端强制校验） |
| 🤖 智能问答 | 维护智能问答：问告警/光照/阈值/设备/指令/维护建议（本地知识库检索，无需外部大模型） |

## 角色与权限（RBAC）

后端基于 JWT + `role_permission` 表做服务端强制校验，前端仅按当前角色展示/隐藏入口：

| 功能 | 路灯管理员（admin） | 市政人员（municipal） |
| --- | --- | --- |
| 光照监测 / 历史趋势 / 设备控制 / 阈值 / 告警 / 指令留痕 | ✅ | ✅ |
| 设备管理（增删设备） | ✅ | ❌ |
| 账号管理、权限管理 | ✅ | ❌ |
| 维护智能问答 | ✅ | ❌ |

权限映射可在"🔐 权限管理"页面在线调整（需 `user:manage`，即 admin 专属）。

## 技术说明

- **技术栈**：原生 HTML/CSS/JS + [ECharts 5](https://cdn.bootcdn.net/ajax/libs/echarts/5.5.0/echarts.min.js)（CDN 引入，需联网加载图表库）
- **认证**：JWT —— 登录后 token 存 `localStorage`（`sl_token`），所有请求自动带 `Authorization: Bearer <token>`，401 自动跳回登录；用户信息存 `sl_user`
- **数据刷新**：设备列表/在线概览 15s、实时指标 10s、趋势图 20s、告警/告警趋势 30s（后端为 REST 轮询，无 WebSocket）
- **历史光照**：展示层按小时聚合（每小时平均），原始 5 秒数据仍实时存储

## 对接的后端接口

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| POST | `/api/auth/login` | 登录获取 token |
| GET | `/api/devices` | 设备列表 |
| GET/POST | `/api/devices/{id}/lux/latest` `/lux/history` | 实时/历史光照 |
| POST | `/api/devices/{id}/lamp` | 控灯（on/off/auto） |
| GET/PUT | `/api/devices/{id}/threshold` | 阈值查询/下发 |
| GET | `/api/alarms` | 告警列表 |
| GET/POST/DELETE | `/api/users` `/api/roles` | 账号与角色管理 |
| GET/PUT | `/api/roles/{id}/permissions` | 角色权限查询 / 保存 |
| GET | `/api/permissions` | 全部权限清单 |
| POST | `/api/assistant/ask` | 维护智能问答 |

## 注意事项

- **CORS**：后端 `api.rs` 已配置 `CorsLayer::permissive()`，否则浏览器跨域访问会被拦截。
- **密钥安全**：`.env`（华为云 AK/SK 等）已被仓库 `.gitignore` 忽略，不会提交；前端不持有任何云端密钥。
- **权限**：账号/权限管理需 `user:manage` 权限，默认仅 admin 角色拥有；权限调整即时生效。
