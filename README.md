# 智慧路灯 IoT 管理系统

基于 BearPi-HM Nano（RISC-V）+ 华为云 IoTDA 的智慧路灯系统，支持光照监测、自动/手动控制、告警管理、智能问答。

## 系统架构

```
┌─────────────┐     MQTT      ┌──────────────┐     HTTPS      ┌──────────────┐
│  BH1750传感器 │ ───────────→ │  华为云 IoTDA  │ ←──────────── │  Rust 后端    │
│  BearPi开发板 │ ←─────────── │  （设备接入）   │ ────────────→ │  (axum 8080)  │
└─────────────┘    控制指令    └──────────────┘    数据查询     └──────┬───────┘
                                                                      │
                                                              ┌───────┴───────┐
                                                              │  Vue3 前端     │
                                                              │  (Vite 5173)  │
                                                              └───────────────┘
```

## 功能清单

| 功能 | 说明 |
|------|------|
| 光照监测 | BH1750 传感器实时采集，5点中值滤波降噪 |
| 自动控制 | 光照 < 260 lux 开灯，> 340 lux 关灯（迟滞防抖） |
| 手动控制 | 前端远程开关灯，通过 IoTDA 下发指令 |
| 阈值配置 | 前端可调光照阈值 |
| 设备管理 | 添加/删除设备，查看在线状态 |
| 告警管理 | 设备离线告警，支持标记已处理/恢复 |
| 审计日志 | 记录每次控制操作（自动/手动） |
| 登录认证 | JWT + Argon2id 密码哈希 |
| 智能问答 | 基于知识库的维护助手（意图识别 + 数据库查询） |
| 历史趋势 | ECharts 光照折线图（1h/24h/7d/30d） |

## 快速开始

### 环境要求

| 工具 | 版本 | 用途 |
|------|------|------|
| Git | 任意 | 克隆仓库 |
| Node.js | v18+ | 前端运行 |
| Rust | 最新 | 后端编译 |
| Docker | 最新 | PostgreSQL 数据库 |
| WSL2 | 可选 | 固件编译 |

### 1. 克隆仓库

```bash
git clone https://github.com/shic0love117-alt/zhihui_ludeng_project.git
cd zhihui_ludeng_project
```

### 2. 启动数据库

```bash
docker run -d --name streetlight-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=streetlight \
  -p 5432:5432 \
  postgres:16
```

### 3. 启动后端

```bash
cd server/backend
cargo run --release
```

首次编译约 5-10 分钟，看到 `listening on 0.0.0.0:8080` 即成功。

数据库表会自动创建，管理员账号自动初始化：
- 用户名：`admin`
- 密码：`admin123`

### 4. 启动前端

```bash
cd frontend
npm install
npm run dev
```

打开浏览器访问 http://localhost:5173 ，用 admin / admin123 登录。

### 5. 固件编译（可选）

需要 WSL2 + Docker：

```bash
# Windows PowerShell
wsl

# WSL 中
cd /mnt/e/路灯项目/AXARINE_repo
bash build.sh
```

编译产物在 `out/` 目录，通过 HiBurn 烧录到 BearPi 开发板。

## 项目结构

```
├── AXARINE_repo/                # 固件代码（OpenHarmony）
│   ├── C3_e53_sc1_pls/
│   │   └── e53_sc1_example.c   # 主程序（传感器读取 + MQTT 上报）
│   └── build.sh                 # Docker 编译脚本
│
├── server/backend/              # Rust 后端（axum + sqlx）
│   ├── src/
│   │   ├── main.rs             # 入口（CORS、迁移、自动建表）
│   │   ├── api.rs              # REST API（设备、告警、问答）
│   │   ├── auth.rs             # JWT 认证
│   │   ├── iothub.rs           # 华为云 IoTDA 北向接口
│   │   └── poll.rs             # 设备状态轮询
│   ├── migrations/              # 数据库迁移
│   └── Cargo.toml
│
└── frontend/                    # Vue3 前端
    ├── src/
    │   ├── pages/               # 页面组件
    │   │   ├── Dashboard.vue    # 首页大屏
    │   │   ├── DeviceList.vue   # 设备列表
    │   │   ├── AlarmList.vue    # 告警列表
    │   │   ├── AssistantQA.vue  # 智能问答
    │   │   ├── Login.vue        # 登录页
    │   │   └── ...
    │   ├── api/device.js        # API 接口
    │   └── store/               # Pinia 状态管理
    └── vite.config.js           # Vite 配置（代理）
```

## 环境变量

后端 `.env` 文件（`server/backend/.env`）：

```env
DATABASE_URL=postgres://postgres:postgres@localhost:5432/streetlight
HUAWEI_IOTHUB_ENDPOINT=https://xxx.iotda-app.cn-south-1.myhuaweicloud.com
HUAWEI_IOTHUB_INSTANCE_ID=your_instance_id
HUAWEI_IOTHUB_DEVICE_ID=your_device_id
HUAWEI_IOTHUB_ACCESS_KEY=your_access_key
HUAWEI_IOTHUB_ACCESS_SECRET=your_access_secret
JWT_SECRET=your_jwt_secret_key
```

前端 `.env` 文件（`frontend/.env`）：

```env
VITE_USE_MOCK=false
VITE_DEVICE_ID=your_device_id
```

## 技术栈

| 层级 | 技术 |
|------|------|
| 硬件 | BearPi-HM Nano (Hi3861, RISC-V) + E53_SC1 (BH1750) |
| 云平台 | 华为云 IoTDA（标准实例，南向 MQTT + 北向 HTTPS） |
| 后端 | Rust (axum + sqlx + reqwest) |
| 数据库 | PostgreSQL 16 |
| 前端 | Vue3 + Vite + Element Plus + Pinia + ECharts |
| 认证 | JWT + Argon2id |

## 常见问题

**Q: 后端启动报 "Address already in use"**
A: 端口被占用，关掉代理软件的全局模式，改用规则模式。

**Q: 前端登录失败**
A: 确认后端已启动（8080 端口），检查浏览器控制台报错。

**Q: WSL 中 cargo 命令找不到**
A: 执行 `source ~/.cargo/env` 或重新打开 WSL 终端。

**Q: 固件编译报 symlink 错误**
A: build.sh 已包含自动修复，确保用 WSL2 运行。
