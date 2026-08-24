# 智慧路灯系统

基于 **BearPi-HM Nano + E53_SC1** 的智慧路灯物联网项目。本仓库包含：

- 数据库设计（MySQL 8.0，含 RBAC 权限）
- FastAPI 后端（账号管理 + 数据采集 + 设备管理）
- 管理页面（中文 Web 界面）
- 一键启动脚本

## 目录结构

```
├── smart_street_light.sql          # 建库脚本（含 CREATE DATABASE，直接导入即可）
├── 启动智慧路灯.bat                 # 一键启动（自动检测 Python）
├── 04_智慧路灯_基本功能清单.md        # 功能清单（参考）
├── smart_street_light_运行状态.html  # 运行状态快照
└── backend/
    ├── main.py                     # FastAPI 后端
    ├── requirements.txt            # 依赖
    ├── test_api.py                 # 接口冒烟测试
    ├── README.md                   # 后端接口说明
    └── static/index.html           # 管理页面
```

## 环境要求

- Python 3.11+（安装时勾选 **Add Python to PATH**）
- MySQL 8.0（5.7+ 亦可）

## 快速开始

### 1. 导入数据库

确保 MySQL 已启动，然后执行（会提示输入 root 密码）：

```bash
mysql -u root -p < smart_street_light.sql
```

脚本会自动创建 `smart_street_light` 库并建好 9 张表（含角色/权限种子数据）。

### 2. 安装依赖

```bash
cd backend
pip install -r requirements.txt
```

### 3. 启动

```bash
cd backend
uvicorn main:app --host 127.0.0.1 --port 8000
```

或者直接双击根目录的 **`启动智慧路灯.bat`**（会自动检测 Python、启动服务并打开浏览器）。

### 4. 打开页面

| 地址 | 用途 |
|------|------|
| http://127.0.0.1:8000/ | 管理页面（账号增删 + 设备增删 + 数据查看） |
| http://127.0.0.1:8000/docs | 交互式接口文档 |

## 数据库密码不同怎么办？

后端默认连 `root / 123456`。如果你的 MySQL 密码不同，**无需改代码**，启动前设置环境变量覆盖即可：

**cmd：**
```cmd
set DB_PASSWORD=你的密码
```

**PowerShell：**
```powershell
$env:DB_PASSWORD="你的密码"
```

支持覆盖的变量：`DB_HOST`、`DB_PORT`、`DB_USER`、`DB_PASSWORD`、`DB_NAME`。

## 接口概览

| 模块 | 接口 |
|------|------|
| 账号管理 | `GET/POST /api/users`、`DELETE /api/users/{id}`、`GET /api/roles` |
| 设备管理 | `GET/POST /api/devices`、`DELETE /api/devices/{id}` |
| 数据采集 | `POST /api/data/luminance`、`/heartbeat`、`/alarm`（上报时自动注册未知设备） |
| 查询 | `GET /api/alarms`、`GET /api/luminance`、`GET /api/health` |

详见 `backend/README.md`。
