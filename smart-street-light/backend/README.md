# 智慧路灯后端服务

FastAPI 后端，负责**账号管理**（市政人员 / 路灯管理员增删）与**数据采集**（光照 / 心跳 / 告警上报入库）。

## 环境

- Python 3.12 + MySQL 8.0.40（本地 `smart_street_light` 库，root/123456）

## 安装依赖

```bash
python -m pip install -r requirements.txt
```

## 启动

```bash
uvicorn main:app --host 127.0.0.1 --port 8000
```

| 地址 | 用途 |
|------|------|
| http://127.0.0.1:8000/ | 管理页面（账号增删 + 数据查看） |
| http://127.0.0.1:8000/docs | 交互式接口文档（可直接测试） |

## 接口一览

### 账号管理
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/users` | 账号列表 |
| POST | `/api/users` | 新增账号（body: `username/password/real_name/role_id`） |
| DELETE | `/api/users/{id}` | 删除账号 |
| GET | `/api/roles` | 角色列表（role_id：1 市政人员 / 2 路灯管理员） |

### 设备管理
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/devices` | 设备列表 |
| POST | `/api/devices` | 新增设备（body: `device_id/name/location`） |
| DELETE | `/api/devices/{id}` | 删除/解绑设备（同时清除其光照/阈值/告警/指令数据） |

### 数据采集（硬件/平台上报）
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/data/luminance` | 上报光照（body: `device_id/luminance`） |
| POST | `/api/data/heartbeat` | 上报心跳/状态（body: `device_id/online_status`） |
| POST | `/api/data/alarm` | 上报告警（body: `device_id/alarm_type/message`） |

> 上报时若 `device_id` 不存在，会自动注册到 `device` 表。

### 设备控制（阈值 + 联动 + 手动）
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/thresholds` | 设置光照阈值（body: `device_id/low_threshold/high_threshold`，按设备 upsert） |
| POST | `/api/control/auto` | 手动触发一次联动判断（body: `device_id`，用最近 N 次采样跑算法） |
| POST | `/api/control/manual` | 手动开关灯（body: `device_id/action`，`action` 为 `on`/`off`） |

> 光照联动算法：滞回控制（双阈值）+ 连续 3 次确认防抖。关灯时连续 3 次低于下限自动开灯，开灯时连续 3 次高于上限自动关灯，阈值附近抖动不动作。开关动作会同步更新 `device.lamp_status` 并写入 `command_record`（`source=auto/manual`）。

### 告警管理
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/alarms/{id}/resolve` | 处理告警（`status` 0→1，写 `resolved_at`） |

### 智能问答（本地检索增强，无需外部大模型）
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/assistant/ask` | 维护智能问答（body: `question`，返回 `answer`） |

> 支持问：告警 / 光照数据 / 设备状态 / 阈值 / 控制记录 / 维护建议（结合 `maintenance_knowledge` 知识库）。意图识别 + 实体抽取后真查数据库，再套话术回答。

### 查询
`GET /api/devices`、`GET /api/alarms`、`GET /api/luminance?device_id=&limit=`、`GET /api/commands?device_id=&limit=`、`GET /api/health`

## 上报示例

```bash
# 光照（lux）
curl -X POST http://127.0.0.1:8000/api/data/luminance \
  -H "Content-Type: application/json" \
  -d '{"device_id":"lamp_001","luminance":245.5}'

# 心跳（在线）
curl -X POST http://127.0.0.1:8000/api/data/heartbeat \
  -H "Content-Type: application/json" \
  -d '{"device_id":"lamp_001","online_status":true}'

# 告警
curl -X POST http://127.0.0.1:8000/api/data/alarm \
  -H "Content-Type: application/json" \
  -d '{"device_id":"lamp_001","alarm_type":"offline","message":"设备离线"}'

# 新增账号
curl -X POST http://127.0.0.1:8000/api/users \
  -H "Content-Type: application/json" \
  -d '{"username":"municipal","password":"123456","real_name":"张工","role_id":1}'
```
