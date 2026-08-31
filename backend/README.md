*AI GENERATED*

# 智慧路灯后端（streetlight-backend）

智慧路灯系统的后端服务：Rust + axum 提供 REST API，轮询华为云 IoTDA 北向接口，把设备状态、光照、告警、控制指令落库到 PostgreSQL，并为 Vue 前端提供设备管理、控灯、阈值、告警、仪表盘、账号 RBAC 与维护智能问答能力。

- 监听地址：`0.0.0.0:8080`，所有业务接口以 `/api` 为前缀
- Swagger UI：`http://127.0.0.1:8080/docs`（OpenAPI JSON：`/api/openapi.json`）
- 数据库：PostgreSQL 16，启动时自动执行 `migrations/`
- 华为云：IoTDA 北向 HTTPS（AK/SK V11-HMAC-SHA256 衍生签名）

---

## 1. 总体架构

```
BearPi-HM Nano / Hi3861
   │  南向 MQTT（设备侧域名, 1883）
   ▼
华为云 IoTDA
   ▲  北向 HTTPS（应用侧域名, AK/SK 签名）
   │  · 查询设备状态 / 影子
   │  · 下发 Light_Control_Led 命令
   │  · 设置 Threshold 可写属性
streetlight-backend (axum, :8080)
   │  每 8s 轮询一次，数据落库
   ▼
PostgreSQL 16
   ▲
   │  REST /api（JWT + RBAC）
Vue3 前端 / Swagger UI / curl
```

核心数据流：

1. 固件通过 MQTT 把 `Light` 服务属性（`Luminance`、`LightStatus`）上报到 IoTDA 设备影子。
2. 后端每 8 秒向 IoTDA 北向查询每台已注册设备：先查在线状态，再查影子，把最新光照写入 `lux_record`、把灯态写入 `device.lamp`。
3. 前端调 REST API 读库；手动控灯 / 改阈值时，后端签名调用 IoTDA 北向下发指令，并把指令结果写入 `command_record` 留痕。
4. 设备离线 / 恢复由轮询任务自动生成或消解 `alarm`。

---

## 2. 目录结构

```
backend/
├── Cargo.toml                # 包定义与依赖（edition 2024）
├── Cargo.lock                # 锁定依赖版本
├── rustfmt.toml              # 格式化配置：max_width = 80
├── .env.example              # 环境变量模板（复制为 .env 后填写，.env 不入库）
├── Dockerfile                # 多阶段构建镜像
├── docker-compose.yml        # PostgreSQL + 后端 + nocodb 一键部署
├── dev.sh                    # 统一入口：db/run/up/down/update/logs/status（见 §3.2/3.3）
├── migrations/
│   ├── 0001_init.sql         # 业务表：device/lux_record/config/alarm/command_record
│   ├── 0002_rbac.sql         # RBAC：role/permission/role_permission/app_user + 种子数据
│   ├── 0003_assistant.sql    # 维护知识库 maintenance_knowledge + 种子数据
│   └── 0004_super_admin.sql  # super_admin 角色 + role:manage 权限码（防权限锁死）
└── src/
    ├── main.rs               # 入口：装配 state、迁移、CORS、启动
    ├── api.rs                # 业务 REST API：设备/光照/控灯/阈值/告警/指令/仪表盘/问答
    ├── auth.rs               # 登录、JWT 中间件、Argon2id、用户/角色/权限 API、引导管理员
    ├── iothub.rs             # IoTDA 北向客户端：V11 签名 + 设备状态/影子/命令/属性 + 轮询任务
    ├── assistant.rs          # 维护智能问答：意图识别 + 实体抽取 + 查库 + 模板回答
    ├── openapi.rs            # Swagger/OpenAPI 文档聚合
    └── tests.rs              # 纯逻辑单元测试（不依赖数据库与网络）
```

---

## 3. 快速开始

### 3.1 环境要求

| 依赖 | 用途 |
|---|---|
| Rust stable（项目当前使用 cargo 1.98+，edition 2024） | 编译运行 |
| Docker | 本地 PostgreSQL；`docker compose` 部署 |
| jq（可选） | 下面的 curl 示例解析 JSON |

### 3.2 本地开发（cargo run）

```bash
cd backend

# 1) 复制并填写环境变量（本地只填 DATABASE_URL / JWT_SECRET 即可启动，
#    HUAWEI_* 不填则 IoTDA 北向功能停用）
cp .env.example .env

# 2) 只启动 PostgreSQL（内部走 compose，与全栈部署不冲突）
./dev.sh db

# 3) 加载 .env 并启动（dev.sh run 会自动 source .env）
./dev.sh run

# 可选：观察日志级别
RUST_LOG=streetlight_backend=debug ./dev.sh run
```

> Windows 侧不经过 WSL 时：PowerShell 用 `.\start.ps1`（加载 .env 后 cargo run），Git Bash 用仓库根 `run.sh`（优先跑已编译的 .exe）。二者与 `dev.sh run` 功能等价，只是环境不同。

启动时**按角色补建引导账号**（某角色已有任意账号则跳过该角色）：

- `super_admin` 系统管理员：默认 `superadmin` / `superadmin123`（可用 `BOOTSTRAP_SUPER_ADMIN_USERNAME` / `BOOTSTRAP_SUPER_ADMIN_PASSWORD` 覆盖）
- `admin` 路灯管理员：默认 `admin` / `admin123`（可用 `BOOTSTRAP_ADMIN_USERNAME` / `BOOTSTRAP_ADMIN_PASSWORD` 覆盖）

生产环境必须在 `.env` 中覆盖全部引导账号默认值与 `JWT_SECRET`。

### 3.3 Docker Compose 一键部署

```bash
cd backend
docker compose up -d --build
# 或统一入口: ./dev.sh up
```

启动三个容器：

| 服务 | 端口 | 说明 |
|---|---|---|
| `postgres` | 5432 | 数据卷 `streetlight-pgdata`，历史数据不丢 |
| `backend` | 8080 | 读取 `.env`，`DATABASE_URL` 被 compose 覆盖为内部服务名 `postgres` |
| `nocodb` | 8081 | 可选，电子表格式看数据用；云部署可删除 |

云服务器部署：同一份 compose 文件 + 填好的 `.env` 即可；建议删除 postgres 的 `5432:5432` 与 nocodb 端口映射，只放行 8080。云上更新代码用 `./dev.sh update`（见 §10.3）。

### 3.4 快速验证

```bash
# 健康检查（公开接口）
curl http://127.0.0.1:8080/api/health

# 登录拿 token
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .token)

# 注册设备（IoTDA 轮询只查已注册设备）
curl -s -X POST http://127.0.0.1:8080/api/devices \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"id":"your_device_id","name":"智慧路灯1号","location":"南门路段"}'

# 查设备、开灯、查告警
curl -s http://127.0.0.1:8080/api/devices -H "Authorization: Bearer $TOKEN"
curl -s -X POST http://127.0.0.1:8080/api/devices/your_device_id/lamp \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"action":"on"}'
curl -s 'http://127.0.0.1:8080/api/alarms?resolved=false' \
  -H "Authorization: Bearer $TOKEN"
```

> 也可以在 Swagger UI 里先调 `POST /api/auth/login`，点右上角 **Authorize** 填入 token 后在线调试全部接口。

---

## 4. 认证与 RBAC

### 4.1 认证流程

- 密码使用 **Argon2id** 哈希（随机盐），数据库中只存哈希串。
- 登录成功后签发 **HS256 JWT**，有效期 24 小时，`Claims` 含 `user_id / username / role_id / role_code / exp`。
- 全局中间件 `auth_middleware` 只做“你是谁”（认证）：除公开路径外，要求请求头 `Authorization: Bearer <token>`；校验通过后把 `Auth { user_id, role_id, role_code }` 塞进 request extensions。
- handler 通过 `Auth` extractor 取出身份，再调用 `auth.require(&state, "权限码")` 做“你能不能做”（授权）。权限码校验走进程内缓存 `perm_cache`（按 `role_id` 缓存权限码集合，命中免查库），`PUT /api/roles/{id}/permissions` 提交后失效对应条目，因此用 `super_admin` 在 Swagger 里改了角色权限后**立即生效，无需重启**（单实例前提；多副本部署时外部直接改库不会触发失效，需改 TTL/广播方案）。

公开路径（无需 token）：

| 路径 | 说明 |
|---|---|
| `GET /api/health` | 健康检查 |
| `POST /api/auth/login` | 登录 |
| `GET /docs`、`/docs/*` | Swagger UI |
| `GET /api/openapi.json` | OpenAPI 文档 |
| 任意 `OPTIONS` | CORS 预检 |

### 4.2 角色与权限码

种子数据在 `migrations/0002_rbac.sql` 与 `migrations/0004_super_admin.sql`：

| role_id | role_code | 角色 | 权限范围 |
|---|---|---|---|
| 1 | `municipal` | 市政人员 | 监测、可视化、控制（含联动预留）、阈值、告警查看、指令留痕 |
| 2 | `admin` | 路灯管理员 | 1~12 号业务权限；**不含 `role:manage`**，不能调整角色权限 |
| 3 | `super_admin` | 系统管理员 | 全部 13 个权限（含 `role:manage`）；其权限固定不可被修改 |

权限码与 API 的对应关系：

| 权限码 | 含义 | 用于 |
|---|---|---|
| `luminance:monitor` | 光照监测 | 最新光照、全局最新光照 |
| `luminance:history` | 历史光照 | 历史/统计接口 |
| `control:linkage` | 光照联动 | 预留；联动在固件本地执行，后端暂不消费 |
| `control:manual` | 手动控灯 | `POST /api/devices/{id}/lamp` |
| `config:threshold` | 阈值设置 | 阈值查询/修改 |
| `config:dimming` | 调光设置 | 手动亮度与照度-亮度曲线查询/修改 |
| `notify:send` | 发送维修通知 | `POST /api/notifications`（municipal / super_admin） |
| `device:status` | 设备状态 | 设备列表、仪表盘 |
| `alarm:offline` | 离线告警 | 预留；离线告警由轮询任务自动生成 |
| `device:manage` | 设备管理 | 设备增删改 |
| `alarm:log` | 告警日志 | 告警查询、处理 |
| `assistant:qa` | 维护智能问答 | `POST /api/assistant/ask`（默认 admin / super_admin） |
| `command:log` | 指令留痕 | 全局/单设备指令查询 |
| `user:manage` | 账号与角色/权限查询 | 用户增删查、角色/权限列表查询 |
| `role:manage` | 角色权限管理 | `GET/PUT /api/roles/{id}/permissions`（默认仅 super_admin） |

### 4.3 安全注意事项

- `JWT_SECRET` 必须覆盖开发默认值 `dev-secret-change-me`，建议 32 字节以上随机串；泄露后所有已签发 token 可被伪造。
- 账号活性复检（**token 隐式吊销**）：认证中间件验签后按 `user_cache`（30s TTL）校验账号仍存在且启用，因此禁用/删除/降权后已签发 token **最迟 30s 失效**（`update_user`/`delete_user` 提交后主动失效对应缓存条目，TTL 兜底"绕过 API 直接改库"的场景）；角色取自数据库而非 token claims，改角色后无需等旧 token 过期。
- 引导账号按角色分别判断：`BOOTSTRAP_SUPER_ADMIN_*` / `BOOTSTRAP_ADMIN_*` 只在对应角色**没有任何账号**时生效；生产首次启动后请删除或修改默认账号。
- 防权限锁死：`super_admin` 的权限映射被接口硬保护（403 拒绝修改）；拥有 `role:manage` 的角色修改**自己**的权限时，必须保留 `role:manage`，否则同样 403。
- CORS 当前为开发期全放开（`CorsLayer` Any，只在 `main.rs` 装配一处，业务 router 不再各自加层），上线前应收紧为前端实际域名。

---

## 5. API 说明

### 5.1 通用约定

- 请求/响应均为 `application/json`，错误响应为纯文本（见下）。
- 所有时间查询参数均为 **RFC3339** 字符串，例如 `2026-08-24T10:00:00Z`、`2026-08-24T18:00:00+08:00`（服务端统一转 UTC）。
- 历史 / 告警 / 指令类列表按时间倒序；设备、账号、角色等目录型列表按创建或 ID 正序。`limit` 默认 500、上限 5000（`clamp_limit` 强制夹到 `1..=max`）；光照历史接口固定最多返回 5000 条。
- 未配置 `HUAWEI_*` 时，依赖 IoTDA 北向的接口返回 `503 IOTDA 北向未配置`。

统一错误模型（`api::Error` → HTTP 状态）：

| Error 变体 | HTTP 状态 | 使用场景 |
|---|---|---|
| `BadRequest` | 400 | 参数缺失/非法、时间格式错误 |
| `Unauthorized` | 401 | 登录失败、未登录、token 无效 |
| `Forbidden` | 403 | 当前角色无对应权限码 |
| `NotFound` | 404 | 设备/账号/告警/角色不存在 |
| `Conflict` | 409 | 用户名已存在 |
| `Iothub` | 502 | IoTDA 北向调用失败（错误信息带华为云响应体） |
| `IothubUnavailable` | 503 | 未配置 HUAWEI_* 环境变量 |
| `Db` / `Internal` | 500 | 数据库或内部错误 |

### 5.2 接口总览

| 方法 | 路径 | 权限码 | 说明 | 成功码 |
|---|---|---|---|---|
| GET | `/api/health` | 公开 | 服务与数据库连通性 | 200 |
| POST | `/api/auth/login` | 公开 | 登录拿 JWT | 200 |
| GET | `/api/auth/me` | 登录即可 | 当前用户信息 | 200 |
| GET | `/api/users` | `user:manage` | 账号列表 | 200 |
| POST | `/api/users` | `user:manage` | 创建账号 | 201 |
| PATCH | `/api/users/{id}` | `user:manage` | 更新账号（用户名/密码/姓名/角色/状态） | 200 |
| DELETE | `/api/users/{id}` | `user:manage` | 删除账号（不能删自己） | 200 |
| GET | `/api/roles` | `user:manage` | 角色列表 | 200 |
| GET | `/api/permissions` | `user:manage` | 权限列表 | 200 |
| GET | `/api/roles/{id}/permissions` | `role:manage` | 角色当前权限 ID 列表 | 200 |
| PUT | `/api/roles/{id}/permissions` | `role:manage` | 全量替换角色权限映射（super_admin 角色受保护） | 204 |
| GET | `/api/devices` | `device:status` | 设备列表 | 200 |
| POST | `/api/devices` | `device:manage` | 注册设备（幂等，可带经纬度） | 201 |
| PATCH | `/api/devices/{id}` | `device:manage` | 更新设备名称/位置/经纬度 | 200 |
| DELETE | `/api/devices/{id}` | `device:manage` | 删除设备并清空关联数据 | 204 |
| GET | `/api/devices/{id}/lux/latest` | `luminance:monitor` | 单设备最新一条光照 | 200 |
| GET | `/api/devices/{id}/lux/history` | `luminance:history` | 单设备历史光照 | 200 |
| GET | `/api/devices/{id}/lux/stats` | `luminance:history` | 单设备光照统计 | 200 |
| GET | `/api/lux/latest` | `luminance:monitor` | 所有设备的最新光照 | 200 |
| GET | `/api/map/devices` | `device:status` | 地图点位（坐标+状态+最新光照） | 200 |
| POST | `/api/devices/{id}/lamp` | `control:manual` | 开灯/关灯/自动（下发 IoTDA） | 202 |
| GET | `/api/devices/{id}/threshold` | `config:threshold` | 查询阈值（未配置返回 40） | 200 |
| PUT | `/api/devices/{id}/threshold` | `config:threshold` | 更新阈值并下发 IoTDA | 204 |
| GET | `/api/devices/{id}/dimming` | `config:dimming` | 查询调光配置（默认亮度 100、无曲线） | 200 |
| PUT | `/api/devices/{id}/dimming` | `config:dimming` | 设置手动亮度/照度-亮度曲线并下发 IoTDA | 204 |
| GET | `/api/devices/{id}/commands` | `command:log` | 单设备指令留痕 | 200 |
| GET | `/api/commands` | `command:log` | 全局指令留痕 | 200 |
| GET | `/api/alarms` | `alarm:log` | 告警列表（可多条件过滤） | 200 |
| PATCH | `/api/alarms/{id}` | `alarm:log` | 标记告警已处理/恢复未处理 | 200 |
| GET | `/api/audit-logs` | `user:manage` | 审计流水（账号/角色/阈值变更，from/to/limit） | 200 |
| GET | `/api/dashboard` | `device:status` | 首页聚合统计 | 200 |
| POST | `/api/assistant/ask` | `assistant:qa` | 维护智能问答 | 200 |
| POST | `/api/iotda/callback` | 公开（免 JWT；配置 `IOTDA_WEBHOOK_TOKEN` 后需 `Authorization: Bearer` 校验） | IoTDA 数据转发 HTTP 推送入口 | 200 |

### 5.3 认证与账号

**POST /api/auth/login**

```jsonc
// 请求
{ "username": "admin", "password": "admin123" }

// 响应 200
{
  "token": "<jwt>",
  "token_type": "Bearer",
  "expires_in": 86400,
  "user": {
    "id": 1, "username": "admin", "real_name": "路灯管理员",
    "role_id": 2, "role_code": "admin", "role_name": "路灯管理员",
    "status": 1, "created_at": "...", "updated_at": "..."
  },
  "role": { "id": 2, "role_code": "admin", "role_name": "路灯管理员", "description": "" },
  "permissions": ["luminance:monitor", "...", "user:manage"]
}
```

用户名密码错误或账号被禁用统一返回 `401 用户名或密码错误`（不暴露具体原因）。

**POST /api/users**

```jsonc
// 请求
{ "username": "operator", "password": "secret6", "real_name": "值班员", "role_id": 1 }
```

校验规则：`username` 去空格后 1~64 字符；密码 8~64 字符且须同时含字母和数字；`role_id` 必须存在；用户名唯一。

**PATCH /api/users/{id}**

```jsonc
// 请求（所有字段可选，至少传一个）
{ "username": "new_name", "password": "newpass123", "real_name": "新姓名", "role_id": 2, "status": 1 }
```

校验规则：`username` 修改时需唯一且 1~64 字符；`password` 8~64 字符且须同时含字母和数字；`role_id` 必须存在；`status` 只能 0 或 1。

**GET / PUT /api/roles/{id}/permissions**

两个接口都需要 `role:manage`（默认仅 `super_admin`）。PUT 为全量替换：

```jsonc
// 请求：权限 ID 数组（注意是数字 ID，不是权限码字符串），全量替换
{ "permission_ids": [1, 2, 3, 4, 5, 6, 7, 9, 11] }
```

- 所有 `permission_ids` 必须真实存在，否则 400；更新在单个事务中完成，映射写入用 `unnest($2::bigint[])` 批量插入（避免 N 次 roundtrip）。
- 提交成功后同步失效该角色的 `perm_cache` 条目，下一请求重新加载新映射。
- `super_admin` 角色的权限映射**固定**，对其 PUT 返回 403。
- 修改“自己当前角色”时，提交的权限 ID 里必须保留 `role:manage`，否则 403（防止把自己锁在权限管理之外）。

### 5.4 设备管理

**POST /api/devices**

```jsonc
// 请求
{
  "id": "your_device_id",
  "name": "智慧路灯1号",
  "location": "南门路段",
  "latitude": 23.1291,     // 可选,WGS84 纬度
  "longitude": 113.2644    // 可选,WGS84 经度;必须与 latitude 成对提供
}
```

- `id` 为必填，去空格后 1~64 字符，必须与 IoTDA 设备 ID 一致。
- 重复注册（同 ID）不报错，返回 201（`ON CONFLICT DO NOTHING`，幂等）。
- 只有注册进 `device` 表的设备才会被轮询任务查询、接收推送。
- 开启 `IOTDA_AUTO_SYNC_DEVICES` 后,华为云新增设备会自动注册入库(只增不删不改,间隔默认 30 分钟),并给路灯管理员发通知;手动注册对已在库设备仍幂等无害。
- 经纬度可选；只传其中一个返回 400，范围越界（纬度 ±90、经度 ±180）返回 400。

**PATCH /api/devices/{id}**：`name`、`location`、`latitude+longitude`（成对）至少提供一项，动态 SQL 只更新传入字段。

**DELETE /api/devices/{id}**：在**单个事务**内删除 `device / config / lux_record / alarm / command_record` 五张表里的关联数据（任一步失败整体回滚，不留孤儿数据）；事务提交成功返回 204（不校验设备是否存在）。

设备对象结构：

```jsonc
{
  "id": "your_device_id",
  "name": "智慧路灯1号",
  "location": "南门路段",
  "latitude": 23.1291,       // WGS84 纬度,null = 未定位
  "longitude": 113.2644,     // WGS84 经度,null = 未定位
  "status": "online",       // online / offline，由轮询任务维护
  "lamp": "on",             // on / off，来自影子 LightStatus
  "mode": "auto",           // auto / manual（当前为信息字段，见“已知限制”）
  "last_seen_at": "2026-08-25T10:00:00Z",
  "created_at": "2026-08-25T09:00:00Z"
}
```

### 5.5 地图点位

**GET /api/map/devices**

一次返回全部设备的点位信息，供前端地图打点（弹窗数据齐全，无需逐设备再查）：

```jsonc
[
  {
    "id": "your_device_id",
    "name": "智慧路灯1号",
    "location": "南门路段",
    "latitude": 23.1291,       // null = 未定位,前端跳过该点
    "longitude": 113.2644,
    "status": "online",       // online / offline,建议映射点位颜色
    "lamp": "on",             // on / off
    "mode": "auto",
    "lux": 352,               // 最新一条光照,从未上报则 null
    "last_seen_at": "2026-08-25T10:00:00Z"
  }
]
```

- 坐标为 **WGS84**（GPS 原始坐标系）；前端用高德/腾讯底图（GCJ-02）时需自行转换，OpenStreetMap/天地图可直接使用。
- 权限 `device:status`（地图本质是状态可视化，与设备列表同权限）。

### 5.6 光照数据

**GET /api/devices/{id}/lux/latest**

返回最新一条 `LuxRecord`；没有数据时返回 `null`。

**GET /api/devices/{id}/lux/history?from=&to=&before=&limit=**

- `from` / `to` 可选，RFC3339；闭区间过滤。
- **分页（keyset 游标）**：`limit` 默认 1000（上限 5000）；结果按 `created_at DESC, id DESC`。
  翻页时把上一页最后一条的 `created_at` 原样传给 `before`（严格小于），游标翻页走 `(device_id, created_at)` 索引，性能与数据总量无关。不传 `before` 即取最新一页。
- 数据量大时不要再依赖“一次拿全量”（旧行为默认 LIMIT 5000 已移除，见 `perf/` 压测报告 F4）。

**GET /api/devices/{id}/lux/stats?from=&to=**

返回 `count / min / max / avg / latest` 五项；`avg` 保留 1 位小数；无数据时 `min/max/avg/latest` 为 `null`。

**GET /api/lux/latest**

返回所有设备各一条最新光照（`LEFT JOIN LATERAL` 保证无光照记录的设备也出现在列表，`lux` 为 `null`）：

```jsonc
[{ "device_id": "dev001", "id": 123, "lux": 400, "created_at": "..." }]
```

`LuxRecord` 结构：`{ "id": 123, "device_id": "dev001", "lux": 400, "created_at": "..." }`

### 5.7 控灯与阈值

**POST /api/devices/{id}/lamp**

```jsonc
// 请求：action 只接受小写 on / off / auto（serde rename_all=lowercase）
{ "action": "on" }
```

处理流程：

1. 校验 `control:manual` 权限；
2. 调 IoTDA `POST /v5/iot/{project_id}/devices/{id}/commands`，下发 `Light_Control_Led`（参数 `Led` 用大写 `ON/OFF/AUTO`）；
3. 无论成功失败都写入 `command_record` 留痕：北向接受记 `sent`，失败记 `failed` 并带错误消息；
4. 北向接受返回 202（固件执行结果不回传，所以指令状态只有 `sent/failed`，没有“已执行”）。

**GET /api/devices/{id}/threshold**：数据库没有配置记录时返回默认 `40`。

**PUT /api/devices/{id}/threshold**

```jsonc
{ "threshold": 300 }   // 合法范围 0~10000
```

先 upsert 到本地 `config` 表，再调 IoTDA 北向 `PUT .../properties` 下发 `Threshold`。注意：如果本地库已写入但北向下发失败，接口返回 502，本地值会保留（当前实现如此，调试时留意两端一致性）。

**GET /api/devices/{id}/dimming**：查询调光配置；数据库没有配置记录时返回默认 `{ "brightness": 100, "dim_curve": "" }`。

**PUT /api/devices/{id}/dimming**

```jsonc
// 两字段至少给一个,都是可选的
{ "brightness": 30,                  // 手动亮度 0~100;设备收到后进入 manual 模式,0 = 手动关灯
  "dim_curve": "0:100,150:60,300:0"  // auto 模式照度-亮度曲线:≤4 个 lux:pct 锚点,
                                     // lux 严格递增 0~100000,分段线性插值;空串 = 停用曲线,
                                     // 回退固件原有阈值开关逻辑
}
```

与阈值同序:先落库 `config`(只更新出现的列),再经 IoTDA 北向 `PUT .../properties` 下发 `Brightness`/`DimCurve` 属性,最后写审计 `config.dimming`。**前提:产品模型 `Light` 服务已在 IoTDA 控制台添加 `Brightness`(int 0~100)与 `DimCurve`(string 长度 64)两个"可读可写"属性**,否则下发报 IOTDA.000029。

### 5.8 指令留痕

- `GET /api/devices/{id}/commands?from=&to=&limit=`
- `GET /api/commands?device_id=&from=&to=&limit=`

```jsonc
{
  "id": 1,
  "device_id": "dev001",
  "action": "on",        // on / off / auto
  "source": "manual",    // 目前只有 manual；auto 联动在固件本地执行，不经后端
  "status": "sent",      // sent = 北向已接受；failed = 北向拒绝/异常
  "message": "",
  "created_at": "..."
}
```

### 5.9 告警

**GET /api/alarms?device_id=&resolved=&from=&to=&type=&limit=**

- `resolved=true`：只查已处理（`resolved_at IS NOT NULL`）；`false`：只查未处理。
- `type` 精确匹配，目前系统自动产生的主要是 `offline`（设备离线）。

**PATCH /api/alarms/{id}**

```jsonc
{ "resolved": true }   // true：置为已处理（resolved_at = now，重复调用保持首次时间）
                       // false：恢复为未处理（resolved_at = NULL）
```

告警结构：`{ "id": 1, "device_id": "dev001", "type": "offline", "message": "设备离线", "created_at": "...", "resolved_at": null }`

### 5.10 仪表盘

**GET /api/dashboard**

一次返回四组聚合（`AVG` 均四舍五入 1 位小数）；四类查询互不依赖，用 `tokio::try_join!` 并发执行（连接池上限 5，恰好够用）：

```jsonc
{
  "devices":  { "total": 10, "online": 8, "lamp_on": 5 },
  "alarms":   { "open": 2, "last_24h": 3 },
  "lux_24h":  { "reports_24h": 1200, "avg_lux_24h": 356.8 },
  "commands_24h": { "manual_24h": 7, "auto_24h": 0 }
}
```

### 5.11 维护智能问答

**POST /api/assistant/ask**

```jsonc
// 请求
{ "question": "最近7天有哪些告警？" }
// 响应
{ "answer": "最近7天，全部设备共 3 条告警，未处理 1 条：\n· dev001 offline（未处理）08-25 10:00 设备离线\n维护建议：..." }
```

实现原理见 7.5 节。这是本地检索式问答，不调用外部大模型，可离线运行。

---

## 6. 数据库与迁移

### 6.1 表结构

| 表 | 用途 | 关键字段 |
|---|---|---|
| `device` | 已注册设备与实时状态 | `id`(PK), `status`, `lamp`, `mode`, `last_seen_at`, `latitude/longitude`(WGS84,可空,0006) |
| `lux_record` | 光照历史 | 自增 id，`device_id + created_at` 索引 |
| `config` | 每设备阈值与调光配置 | `device_id`(PK), `threshold` 默认 40；`brightness` 默认 100、`dim_curve` 默认空(0008) |
| `alarm` | 告警 | `type`, `message`, `resolved_at`（非空=已处理） |
| `command_record` | 控制指令留痕 | `action/source/status/message`，`device_id + created_at` 索引 |
| `role` | 角色 | `municipal` / `admin` / `super_admin` |
| `permission` | 权限点 | 15 个 `perm_code` |
| `role_permission` | 角色-权限映射 | `(role_id, permission_id)` 唯一，级联删除 |
| `app_user` | 登录账号 | `password_hash`（Argon2id），`status` 0/1 |
| `maintenance_knowledge` | 问答知识库 | `keyword/cause/suggestion` 种子数据 |

> PostgreSQL 中 `user` 是保留字，所以账号表命名为 `app_user`。
> 业务表之间未建外键约束；删除设备时靠 `DELETE /api/devices/{id}` 显式清理关联表。新增设备子表时必须同步在该 handler 的静态 SQL 数组里补 DELETE。

### 6.2 迁移规则

- 使用 `sqlx::migrate!("./migrations")`，后端每次启动自动执行未应用的迁移。
- 文件名必须递增：`NNNN_描述.sql`。
- 已应用到数据库的迁移文件**不允许修改**；只能新增迁移。0001 文件内注释允许“设备未正式部署前原地修改”，一旦上线立即冻结。
- 种子数据（角色/权限/知识库）写在迁移 SQL 里，保证全新环境开箱即用。
- 已有库升级到 0004 时，`admin` 不会自动获得新增的 `role:manage`（0002 的授权先于该权限插入执行，设计如此）；需要授权时用 `superadmin` 登录后走 PUT 接口。

---

## 7. 核心原理

### 7.1 启动装配（main.rs）

```
初始化 tracing（默认 info，可用 RUST_LOG 覆盖）
→ 连接 PostgreSQL（默认本地 streetlight/streetlight，连接池上限 5）
→ 自动执行 migrations
→ bootstrap_admin：按角色补建引导账号（super_admin / admin 各缺账号才创建）
→ 初始化角色权限缓存 perm_cache（空表，首次 require 时惰性加载）
→ 读取 JWT_SECRET（未设置则警告并用开发默认值）
→ IothubClient::from_env()：HUAWEI_* 齐全才启用，否则北向功能停用
→ 若启用：spawn 8 秒轮询任务
→ 组装 Router = api::router + auth::router + SwaggerUi(/docs)
→ 全局 auth_middleware → CORS 层（main.rs 统一装配，业务 router 内不再各自加层）
→ 监听 0.0.0.0:8080
```

`AppState` 是 `#[derive(Clone)]` 的普通结构体（`PgPool` / `Arc` 内部共享，按值克隆开销可忽略），包含 `db: PgPool`、`iothub: Option<Arc<IothubClient>>`、`jwt_secret: Arc<str>`、`perm_cache: PermCache`（`Arc<RwLock<HashMap<i64, Arc<HashSet<String>>>>>`，角色权限缓存）；各模块 `router(state: AppState)` 直接按值接收 `with_state`，不再套一层 `Arc<AppState>`。

### 7.2 一次请求的生命周期

```
HTTP 请求
→ CORS 层
→ auth_middleware
    · 公开路径？直接放行
    · 否则解析 Authorization: Bearer <token>
    · HS256 验签 + exp 校验 → 把 Auth 塞进 extensions
→ handler extractor：State<AppState> + Auth + Path/Query/Json
→ 参数反序列化（axum 自动 400）
→ auth.require(&state, "权限码")：先查 perm_cache，未命中查 role_permission 并回填缓存，无权限 403
→ 业务逻辑：QueryBuilder 查库 / reqwest 调 IoTDA
→ 所有可恢复错误以 `?` 上抛为 api::Error
→ Error::into_response() 映射为 (HTTP 状态, 中文错误消息)
```

### 7.3 IoTDA 北向客户端（iothub.rs）

**配置发现**：四个变量 `HUAWEI_AK / HUAWEI_SK / HUAWEI_PROJECT_ID / HUAWEI_IOTDA_ENDPOINT` 全部非空才启用；region 优先读 `HUAWEI_IOTDA_REGION`，否则从 endpoint 域名推断（`xxx.<region>.myhuaweicloud.com` 中取 region 段）。

**签名**：标准版/企业版实例必须使用 **V11-HMAC-SHA256 衍生签名**（旧 SDK-HMAC-SHA256 会 401）：

```
info   = yyyymmdd/{region}/iotdm
PRK    = HMAC-SHA256(key = AK, data = SK)
T1     = HMAC-SHA256(key = PRK, data = info || 0x01)
签名密钥 = hex(T1)          # 注意：hex 字符串作为 HMAC key
string_to_sign = "V11-HMAC-SHA256\n{sdk_date}\n{info}\n{sha256(canonical_request)}"
```

两个易错点（`tests.rs` 用 KAT 锁死防回归）：规范 URI 必须以 `/` 结尾；canonical headers 块与 `SignedHeaders` 之间多一个空行。

签名凭据统一打包在 `Credentials { ak, sk, region, host }`，`sign_derived(&creds, method, uri, sdk_date, body)` 一次取齐四个字段；`sk` 用 `SecretKey` 新类型包装（`Debug`/`Display` 固定输出 `***` 打码），防止日志或错误消息意外泄露设备密钥。

**北向调用**：

| 后端方法 | IoTDA 接口 | 用途 |
|---|---|---|
| `device_status` | GET `/v5/iot/{project}/devices/{id}` | 在线状态 |
| `shadow` | GET `/v5/iot/{project}/devices/{id}/shadow` | 取 `Light` 服务 reported 属性 |
| `control_led` | POST `/v5/iot/{project}/devices/{id}/commands` | 下发 `Light_Control_Led` |
| `set_threshold` | PUT `/v5/iot/{project}/devices/{id}/properties` | 下发 `Threshold` |
| `set_dimming` | PUT `/v5/iot/{project}/devices/{id}/properties` | 下发 `Brightness`/`DimCurve`(只放出现的键) |

- HTTP 客户端超时 35s：IoTDA 命令同步等待设备响应，超时太短会误判失败。
- 非 2xx 时把状态码和华为云响应体一起放进错误（`BAD_GATEWAY`），方便排查 IOTDA 错误码。
- 容器镜像内置 ca-certificates 与 `gai.conf` IPv4 优先级（IoTDA 域名 AAAA 记录在前，容器无 IPv6 出口）。

### 7.4 8 秒轮询任务

```
每 8s：
  0. 本地失联检测：心跳 last_seen_at（= 最后一条数据上报的 IoTDA 平台事件时间）
     超过 90s 未前进的在线设备直接标记离线并产生离线告警（去重），
     不等 IoTDA 的 MQTT 超时判定（60-120s，太慢）
  SELECT id FROM device
  → 对每台设备并发执行 poll_device（`for_each_concurrent(8)`，并发上限 8，避免设备多时同时打出几十个 HTTPS 请求）：
      1. device_status：在线？
      2. UPDATE device.status；状态发生变化时：
         在线 → 自动消解该设备未处理的 offline 告警（受心跳新鲜度门控，见下）
         离线 → 插入一条 offline 告警
      3. 离线 → 直接返回（不读影子）
      4. 在线 → 读影子 Light 服务：
         Luminance  → INSERT lux_record
         LightStatus → UPDATE device.lamp（转小写）
         event_time → 心跳：单调前进 last_seen_at（乱序/迟到的旧事件不回拨）
```

关键设计：

- **只在设备 ONLINE 时读影子入库**。设备离线后影子仍保留最后一次上报值，直接入库会持续写入假光照数据；断连后影子的 event_time 是冻结的，因此心跳也不会被旧影子误刷。
- **在线状态由两条信号共同决定**：IoTDA 状态 API 报 ONLINE 且 90s 内有数据上报（心跳新鲜）才翻回在线。设备断连后 MQTT 宽限期（60-120s）内 API 仍报 ONLINE，门控挡住"翻回在线→再超时"的抖动与告警风暴。
- 心跳由数据上报驱动（webhook 属性推送与轮询影子共用 `apply_shadow_props`），不由状态观测驱动——否则本地失联检测会被同一个慢信号持续刷新而失效。

### 7.5 维护智能问答（assistant.rs）

无外部大模型，本地检索 + 模板生成：

```
question
→ 意图识别：关键词词典命中长度加权打分（query_alarm / query_luminance /
   query_threshold / query_device / query_command / advice / fallback）
→ 实体抽取：优先匹配 DB 中的 device_id / name 子串；
   否则正则匹配“灯N号 / N号灯 / 灯N”
→ 时间窗：解析“最近N天/小时/分钟/周”，默认按意图取 7 天或 1 天
→ 真查业务表（alarm / lux_record / config / device / command_record）
→ 有告警/提问关键词时查 maintenance_knowledge 附维护建议
→ 模板拼装中文回答（最多展示 5 条告警、10 条指令）
```

实现细节：查询行用 `sqlx::FromRow` 结构体按列名映射（`AlarmRow`/`DeviceRow`/`CommandRow`/`LuxAggRow`/`ThresholdRow`/`KnowledgeRow`）；时间窗与设备号正则用 `LazyLock<Regex>` 静态编译一次、进程内复用；知识库检索由 `find_advice` 统一承担（告警文本与提问共用）。

### 7.6 OpenAPI 文档

- 规范要求所有对外 handler 都用 `#[utoipa::path]` 标注路径、参数、请求体、响应与安全方案，并登记进 `openapi.rs` 的 `paths(...)`。
- `src/openapi.rs` 汇总 `paths(...)` 并补充 `bearer_auth` 安全方案，Swagger UI 的 Authorize 可用。
- 新增接口必须同步：route + `#[utoipa::path]` + `openapi.rs` 的 paths 注册，三者缺一会造成“接口可用但文档看不到”。

---

## 8. 开发规范

### 8.1 新增一个 API 的检查清单

以新增“设备信息详情”为例，完整动作如下：

1. **定义模型**：请求体 `Deserialize`，响应体 `Serialize + ToSchema`；字段名保持 JSON 小写下划线风格。
2. **写 handler**：签名使用 `State<AppState>`、`Auth`（受保护接口）、`Path/Query/Json` 提取器；返回 `Result<..., api::Error>`，全链路 `?` 传播，不自己 `match` 错误。
3. **挂路由**：在所属模块 `router()` 中注册 method + path；路径必须带 `/api` 前缀。
4. **做授权**：受保护接口在 handler 第一行 `auth.require(&s, "权限码").await?`。新权限码需要写迁移插入 `permission` 并给角色授权。
5. **写 OpenAPI**：加 `#[utoipa::path]`，并登记进 `openapi.rs` 的 `paths(...)`。
6. **校验参数**：长度/范围/时间格式在 handler 开头显式校验，错误用 `Error::BadRequest` + 中文消息。
7. **补测试**：可脱离 DB/网络验证的纯逻辑（解析、serde、状态码、签名、意图分类等）必须补到 `src/tests.rs`。

### 8.2 SQL 与 sqlx 约定

- 本项目**不使用** `query!` 编译期宏，SQL 均为运行时执行；构建不需要数据库，但 SQL 拼写错误只能靠运行/测试发现。
- 表名、列名、固定 SQL 片段一律硬编码字符串；**用户输入只允许走 bind 参数**，禁止 `format!` 拼接用户值。
- 动态条件使用 `sqlx::QueryBuilder`：条件片段 `push(" AND ...")` + 值 `push_bind(...)`，排序/ LIMIT 数值用 `push()`（数值不是用户字符串）。
- sqlx 0.9 起 `query()` 只接受 `SqlSafeStr`，动态 `String` 会被编译期拒收；若确需动态列/表名，用 `push` 的片段只能是代码内白名单常量。
- DB 封闭取值列（`device.status/lamp/mode`、`command_record.action/source/status`）已用 `api.rs` 的 `text_enum!` 宏枚举化（`DeviceStatus`/`LampState`/`ControlMode`/`CommandSource`/`CommandStatus`）：对外 serde 小写（与库中字符串逐字节一致），sqlx `try_from` 解码，库中出现未知值时兜底 `Unknown`，不让查询整体失败；新增取值只改宏定义一处。
- `delete_device` 的设备级联清理在**单个事务**内按静态 SQL 白名单顺序执行；新增设备子表必须补进该白名单。

### 8.3 错误处理

- 业务可恢复错误统一用 `api::Error`（`thiserror`），不要在各 handler 里返回裸 `(status, text)`。
- HTTP 映射集中在 `Error::into_response()`；新增错误变体时同步补 `tests.rs::error_maps_to_http_status` 断言。
- 错误消息用中文、带上下文（哪个参数、哪个设备），例如 `设备 {id} 不存在`、`bad from: 需为 RFC3339 时间...`。
- 不可恢复的编程错误才允许 panic/expect；HMAC key 构造、正则常量这类“编译期保证合法”的 expect 需要在注释里说明理由。

### 8.4 时间与分页工具

- 时间字符串用 `api::parse_ts(param, raw)`：RFC3339 + 统一转 UTC + 中文报错；多个接口共用的 from/to 区间解析走 `api::parse_time_range(from, to)`。
- `lux_record` 的 WHERE 拼接（device_id + 可选 from/to）由 `api::push_lux_filters` 统一追加，避免各接口重复拼条件。
- 列表 limit 用 `api::clamp_limit(limit, default, max)`：默认 500、上限 5000、最小 1，禁止无限量查询。

### 8.5 代码风格与质量门禁

```bash
cd backend
cargo fmt --check        # rustfmt 配置：max_width = 80
cargo clippy --all-targets   # Cargo.toml 已启用 all/cargo/nursery/pedantic/perf 全量 warn
cargo test               # 16 个纯逻辑测试，不依赖数据库/网络，可直接跑
cargo build
```

- Clippy 全部警告应清零；个别允许的例外必须写 `#[allow(clippy::...)]` 并注释理由（目前仅 `text_enum!` 里 `infallible_try_from` 一处）。
- 公共纯函数标 `#[must_use]` 防止误丢弃结果。
- 多列查询行用 `sqlx::FromRow` 结构体按列名映射（见 `assistant.rs` 的 `AlarmRow` 等），不用裸 tuple + “列顺序”注释。
- Argon2id 哈希/校验必须经 `hash_password_async` / `verify_password_async`（内部 `spawn_blocking`，CPU 密集操作不进 async handler 阻塞 worker）。
- `dashboard` 与 `lux_stats` 这类多条独立查询用 `tokio::try_join!` 并发执行。
- 提交前至少通过：`cargo fmt --check`、`cargo clippy --all-targets`、`cargo test`、`cargo build`。
- 注释/文档：模块头 `//!` 说明职责；踩坑点写“为什么”（如影子只在 ONLINE 时入库、URI 补 `/`），而不是重复代码。

### 8.6 配置与密钥

| 环境变量 | 默认值 | 说明 |
|---|---|---|
| `DATABASE_URL` | `postgres://streetlight:streetlight@127.0.0.1:5432/streetlight` | 本地默认与 compose 保持一致 |
| `JWT_SECRET` | `dev-secret-change-me` | 生产必改 |
| `HUAWEI_AK` / `HUAWEI_SK` | 空 | 华为云访问密钥，缺任一则停用北向；SK 读入后以 `SecretKey` 包装（打印/日志自动打码） |
| `HUAWEI_PROJECT_ID` | 空 | 华为云项目 ID |
| `HUAWEI_IOTDA_ENDPOINT` | 空 | IoTDA 实例**应用侧**域名，如 `xxx.st1.iotda-app.cn-south-1.myhuaweicloud.com` |
| `HUAWEI_IOTDA_REGION` | 从 endpoint 推断 | 标准版/企业版 V11 衍生签名所需区域 |
| `IOTHUB_DRY_RUN` | `false` | 压测/联调：true 时北向调用本地短路（不发真实华为云请求） |
| `DATABASE_POOL_SIZE` | `20` | 数据库连接池上限（压测 A/B：5→20 读接口 +77~90%、控灯 +269%） |
| `ARGON2_MAX_CONCURRENCY` | `32` | Argon2 校验并发闸；每次校验约 19MiB 工作内存，登录风暴下无闸会打爆 RSS（perf 报告 F2） |
| `LOGIN_RATE_LIMIT_PER_MIN` | `30` | 登录限流：每 IP 每分钟最大尝试次数；0 = 不限流 |
| `ARGON2_M_COST_KIB` / `ARGON2_T` / `ARGON2_P` | `19456` / `2` / `1` | Argon2id 参数（OWASP 推荐档）；已存密码哈希不受影响（校验按 hash 串内嵌参数执行） |
| `BOOTSTRAP_SUPER_ADMIN_USERNAME` | `superadmin` | 仅当 `super_admin` 角色无账号时生效 |
| `BOOTSTRAP_SUPER_ADMIN_PASSWORD` | `superadmin123` | 仅当 `super_admin` 角色无账号时生效，生产必改 |
| `BOOTSTRAP_ADMIN_USERNAME` | `admin` | 仅当 `admin` 角色无账号时生效 |
| `BOOTSTRAP_ADMIN_PASSWORD` | `admin123` | 仅当 `admin` 角色无账号时生效，生产必改 |
| `IOTDA_POLL_INTERVAL_SECS` | `8` | 影子轮询间隔秒数；启用数据转发推送后建议 60（推送为主、轮询兜底校准） |
| `IOTDA_WEBHOOK_TOKEN` | 空 | 数据转发回调共享 token；配置后 `/api/iotda/callback` 要求 `Authorization: Bearer`（常数时间比较），留空=不鉴权（仅本地开发，启动有 warn，公网必须配置） |
| `IOTDA_AUTO_SYNC_DEVICES` | `false` | 设备自动同步开关：true 时按 `IOTDA_SYNC_INTERVAL_SECS` 把华为云设备列表同步进本地 `device` 表（只增不删不改，新设备自动注册），新增/漂移设备给路灯管理员发通知（未读去重） |
| `IOTDA_SYNC_INTERVAL_SECS` | `1800` | 设备同步间隔秒数（默认 30 分钟；与轮询间隔解耦） |
| `IOTDA_SYNC_PRODUCT_ID` | 空 | 只同步该产品下的设备；留空=项目全部（项目只有路灯一种产品时可不填） |
| `ALLOWED_ORIGINS` | 空 | CORS 白名单（逗号分隔）；留空=开发模式全放开（`Any`） |
| `RUST_LOG` | `info` | tracing env-filter |

- 后端**不会自动读取 `.env` 文件**（没有 dotenv 加载代码）：本地 `cargo run` 先 `set -a && . ./.env && set +a`；Docker Compose 由 `env_file` 注入。
- `.env` 已被 `.gitignore` 和 `.dockerignore` 排除，真实凭据严禁提交或写进镜像。
- `.env.example` 是环境变量的唯一权威清单；仓库根 README 中旧的 `HUAWEI_IOTHUB_*` 命名已废弃，以本文件与 `.env.example` 为准。

### 8.7 迁移纪律

- 只增不改：已上环境的迁移禁止修改；上线后的 schema 变更一律新增 `NNNN_*.sql`。
- 每个迁移文件开头写清楚目的与注意事项。
- 业务侧不要手写 `CREATE TABLE` 热补丁；一切 schema 变更走迁移。

---

## 9. 测试

`cargo test` 直接运行，共 16 个测试，不依赖数据库和网络，覆盖：

| 模块 | 覆盖点 |
|---|---|
| `iothub` | SHA-256/HMAC RFC 向量；V11 衍生签名 GET/PUT KAT、确定性、格式 |
| `api` | LampAction 小写 serde；错误状态码映射；RFC3339 时间解析；limit 夹取 |
| `auth` | Argon2id 哈希往返与随机盐；公开路径白名单 |
| `assistant` | 意图关键词加权、时间窗解析、时间格式化 |
| `iothub` 模型 | 在线状态/影子属性的 serde 约定 |

新增纯逻辑时优先写单元测试；涉及 DB/IoTDA 的链路用 `curl + Swagger` 手工回归，或伪造场景（如拔电触发离线告警）验证。

---

## 10. 部署说明

### 10.1 Docker 构建要点（Dockerfile）

- 多阶段：`rust:1-bookworm` 编译（crates 走 rsproxy.cn 稀疏镜像加速）→ `debian:bookworm-slim` 运行。
- 运行镜像安装 ca-certificates（北向 HTTPS 必需）并配置 `gai.conf` 提高 IPv4 优先级。
- `.dockerignore` 排除 `target/` 与 `.env`。

### 10.2 compose 要点

- `postgres` 带 healthcheck，`backend` 等 `service_healthy` 后启动，`restart: unless-stopped`。
- 数据卷 `pgdata` 用固定名称 `streetlight-pgdata` 复用历史数据。
- 云部署建议删除 postgres/nocodb 的端口映射，仅保留 8080。

### 10.3 云上更新后端（dev.sh update）

```bash
cd backend
./dev.sh update
```

流程：`git pull --ff-only` → 构建 backend 镜像 → 只重建 backend 容器（数据库等其他服务不动）→ 轮询 `/api/health` 直到通过（超时默认 60s，可用 `BACKEND_HEALTH_URL` / `BACKEND_HEALTH_TIMEOUT` 覆盖）→ 打印状态与最近日志。健康检查失败时退出码非 0。

---

## 11. 已知限制与常见问题

- **指令无执行回执**：固件不回传命令执行结果，`command_record.status` 只有 `sent / failed`。
- **阈值“半成功”**：`PUT /threshold` 先写库再下发；下发失败时本地已更新，接口返回 502。
- **token 吊销有 ≤30s 延迟**：删号/禁用后旧 token 最迟 30s 失效（账号活性复检，详见 4.3），期间仍可访问。
- **`mode` 字段不实时**：后端没有根据控灯/联动结果回写 `device.mode`，目前仅作信息展示。
- **北向 401**：优先检查是否用了旧版 SDK-HMAC-SHA256；标准版/企业版必须 V11 衍生签名，且 `HUAWEI_IOTDA_REGION` 或 endpoint 域名中的 region 必须正确。
- **端口被占**：`8080` 冲突通常是代理软件全局模式，改规则模式或换端口。
- **`cargo run` 读不到 .env**：后端不自动加载 `.env`，直接 `cargo run` 前必须 source（见 3.2）；用 `./dev.sh run` 则自动加载。

---

## 12. 相关文档

- 项目总览与固件说明：仓库根 `README.md`、`AGENTS.md`
- 需求：`智慧路灯_基本功能清单.md`
- 华为云配置：`华为云IoTDA部署文档.md`
- 前端 API 封装：`../frontend_vue/src/api/device.js`
