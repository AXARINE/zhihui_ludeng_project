# 智慧路灯 — 华为云 IoTDA 部署文档

> 本文档描述如何把智慧路灯系统部署起来:① 华为云 IoTDA 侧建实例/产品/设备/凭证,② 固件编译烧录到 BearPi-HM Nano 开发板,③ Rust 后端 + PostgreSQL 部署到本地(WSL)或云服务器,④ 全链路验收。
> 系统已全链路验收,本文档按当前代码事实编写;接口清单见 `README.md` 第 3 节,功能愿景见 `智慧路灯_基本功能清单.md`。

## 1. 部署架构

```
Hi3861 --Wi-Fi/MQTT(oc_mqtt, 1883)--> 华为云 IoTDA(标准版实例, cn-south-1)
                                          ↑ 北向 API(HTTPS, AK/SK V11 衍生签名)
Rust 后端(axum, 8080) --> PostgreSQL(Docker)
```

| 组件 | 部署位置 | 说明 |
|---|---|---|
| 设备固件 | BearPi-HM Nano 开发板 | 50ms 采样 + 本地光照联动(断网可用),每 5s 上报 `Luminance`/`LightStatus`,接收命令与阈值下发 |
| IoTDA | 华为云 | 设备接入(MQTT)+ 影子 + 在线状态 + 命令/属性转发 |
| Rust 后端 | 本地 WSL 或云服务器 | 每 8s 轮询影子入库、在线/离线监控、REST API(含账号/RBAC)、命令经北向 API 转发 |
| PostgreSQL | Docker 容器 | 光照历史、设备、告警、指令、账号等数据,迁移脚本随后端启动自动执行 |
| 前端 | 不在本仓库 | 通过后端 REST API 访问(接口文档见 Swagger UI `/docs`) |

## 2. 前置条件

### 2.1 硬件与软件

| 项 | 要求 |
|---|---|
| 开发板 | BearPi-HM Nano + E53_SC1 扩展板(BH1750 光照传感器 + 补光灯),Type-C 数据线 |
| 编译环境 | WSL2 Ubuntu + Docker(镜像 `openharmony/openharmony-docker:0.0.3`,首次 `./build.sh` 自动拉取) |
| 烧录 | Windows 侧 HiBurn(仓库自带 `tools/hiburn_windows/`),`./flash.sh` 自动处理 UNC 路径与权限问题 |
| 串口调试 | 任意串口终端,115200 8N1(推荐 `bearpi-serial.ps1`,默认 COM4) |
| 后端 | Rust stable(本地开发)或 Docker + docker compose(生产);华为云账号已实名并开通 IoTDA |
| 网络 | 2.4GHz Wi-Fi(Hi3861 不支持 5GHz);云部署时服务器安全组放行 80/443(HTTPS 反代) |

### 2.2 需要准备的凭据清单

| 凭据 | 去向 | 获取位置 |
|---|---|---|
| Wi-Fi SSID / 密码 | 固件 `app_config.h` | 自备(2.4G) |
| IoTDA 设备 ID / 设备密钥 | 固件 `app_config.h` | 控制台 → 注册设备 |
| IoTDA 实例**设备侧域名** | 固件 `e53_sc1_example.c` 顶部 `CONFIG_APP_SERVERIP` | 控制台 → 实例 → 接入信息 |
| IoTDA 实例**应用侧域名** | 后端 `.env` 的 `HUAWEI_IOTDA_ENDPOINT` | 同上 |
| 项目 ID | 后端 `.env` | 我的凭证 → 项目列表(对应区域行) |
| AK / SK | 后端 `.env` | 我的凭证 → 访问密钥(或 IAM 用户) |

## 3. 华为云 IoTDA 侧配置(控制台操作,一次性)

### 3.1 创建实例并记录接入信息

1. 设备接入 IoTDA → 开通/创建**标准版实例**(区域如 **cn-south-1**,单设备演示免费额度足够)。
2. 实例详情 → **接入信息**,记下:
   - **设备侧接入域名**,形如 `69b5bf8bcd.st1.iotda-device.cn-south-1.myhuaweicloud.com` → 填固件;
   - **应用侧接入域名**,形如 `69b5bf8bcd.st1.iotda-app.cn-south-1.myhuaweicloud.com` → 填后端 `.env`。
3. ⚠️ 标准版/企业版实例**没有区域共享域名**(`iotda.{region}.myhuaweicloud.com` 不存在),必须用实例级域名。

### 3.2 创建产品与模型

1. 产品 → 创建产品:协议 **MQTT**、数据格式 **JSON**、设备类型自定(如 StreetLight)。
2. 产品详情 → 模型定义,服务 ID 为 **`Light`**:

| 类型 | 标识 | 数据类型 | 读写 | 说明 |
|---|---|---|---|---|
| 属性 | `Luminance` | int | 只读 | 光照值,设备每 5s 上报 |
| 属性 | `LightStatus` | string | 只读 | 灯态(ON/OFF) |
| 属性 | `Threshold` | int | **可读可写** | 开关灯阈值(0~10000);⚠️ 不勾"可写"会导致下发报 IOTDA.000029 |
| 命令 | `Light_Control_Led` | 参数 `Led`:string,枚举 ON/OFF/AUTO | — | 控灯/恢复自动 |

### 3.3 注册设备

设备 → 所有设备 → 注册设备;记下**设备 ID** 与**设备密钥**(填 `app_config.h`)。

### 3.4 北向 API 凭证(IAM AK/SK)

1. 统一身份认证 IAM → 创建 IAM 用户(勾选编程访问)或直接在"我的凭证 → 访问密钥"创建 AK/SK。
2. 给该用户所属**用户组**绑定 IoTDA 权限策略(如自定义策略 `{"Action": ["IoTDA:*:*"]}`,作用范围为对应区域级项目);⚠️ 授权有数分钟传播延迟,授权后立刻调用可能仍报 401。
3. 记录**项目 ID**(我的凭证 → 项目列表,选实例所在区域行)。
4. ⚠️ **签名算法**:标准版/企业版实例的北向 API 必须使用 **V11-HMAC-SHA256 衍生签名**(后端已实现,`HUAWEI_IOTDA_REGION` 留空会自动从 endpoint 域名推断);若误用旧版 SDK-HMAC-SHA256,接口返回 401 IOTDA.000002,与 IAM 权限无关。

## 4. 设备端固件部署

### 4.1 获取代码

```bash
git clone --recursive https://github.com/AXARINE/zhihui_ludeng_project.git
cd zhihui_ludeng_project
# 未加 --recursive 的仓库补拉源码树:git submodule update --init
```

### 4.2 配置凭据

1. 复制模板:`cp C3_e53_sc1_pls/include/app_config.example.h C3_e53_sc1_pls/include/app_config.h`,填入 Wi-Fi SSID/密码与设备 ID/密钥(该文件被 `.gitignore` 忽略,**不会进 git**;`build.sh` 会自动把它写入源码树)。
2. 编辑 `C3_e53_sc1_pls/e53_sc1_example.c` 顶部,把 `CONFIG_APP_SERVERIP` 改为**你的实例设备侧域名**。
   - 端口保持 `1883`:⚠️ **不要改 8883 MQTTS**——本工程 iot_link 内置 mbedtls 在 Hi3861 上运行 TLS 不稳定(证书解析阶段内核异常、订阅最长 90s 超时、断开清理 panic,设备重启循环、命令下发超时 IOTDA.014111)。根 CA 保留在 `include/iotda_ca.h` 备用,问题解决前勿启用。

### 4.3 编译与烧录(WSL 内)

```bash
./build.sh      # Docker 一键编译:自动同步样例进源码树、启用 BUILD.gn、产出 out/.../Hi3861_wifiiot_app_allinone.bin
./flash.sh 4    # 烧录(把 4 换成板子的 COM 号);HiBurn 窗口弹出后按一下开发板 RESET
```

- 烧录成功标志:`FLASH OK (HiBurn exit 0)`;之后**再按一次 RESET** 新固件才运行。
- 烧录失败排查:HiBurn 退出码 17 = 板子未连接或窗口被关;COM 参数必须数字格式(脚本已内置)。

### 4.4 验证固件运行

- 串口(115200):确认 `oc_mqtt_profile_connect succed`,随后每 5s 出现属性上报日志;
- 无串口时(如充电器供电):手捂光敏传感器,补光灯亮 = 固件在跑且 auto 联动正常;
- IoTDA 控制台 → 设备详情:状态"在线",属性 `Luminance`/`LightStatus` 每 5s 刷新。

## 5. 后端部署

### 5.1 配置 `.env`

```bash
cd backend
cp .env.example .env
vim .env
```

| 变量 | 必填 | 说明 |
|---|---|---|
| `HUAWEI_AK` / `HUAWEI_SK` | ✅ | 北向 API 访问密钥 |
| `HUAWEI_PROJECT_ID` | ✅ | 实例所在区域的项目 ID |
| `HUAWEI_IOTDA_ENDPOINT` | ✅ | 实例**应用侧**域名(见 3.1) |
| `HUAWEI_IOTDA_REGION` | 可选 | 如 `cn-south-1`;留空自动从 endpoint 推断 |
| `DATABASE_URL` | ✅ | 本地直连模式用 `127.0.0.1`(compose 会覆盖为内部服务名) |
| `JWT_SECRET` | ✅ | 生产必改:`openssl rand -hex 32` |
| `BOOTSTRAP_ADMIN_USERNAME` / `BOOTSTRAP_ADMIN_PASSWORD` | 建议 | 首次启动且账号表为空时创建管理员;默认 `admin/admin123` 仅开发用 |

### 5.2 本地开发模式(WSL)

```bash
cd backend
./dev.sh db                        # 只起 PostgreSQL 容器(streetlight-postgres)
./dev.sh run                       # 加载 .env 并启动;监听 8080,首次启动自动建表 + 创建引导管理员
```

浏览器打开 `http://127.0.0.1:8080/docs`(Swagger UI):`POST /api/auth/login` 拿 token → 右上角 Authorize 填 `Bearer <token>` → 在线调试全部接口。

### 5.3 生产模式(docker compose,云服务器)

同一份 `docker-compose.yml` + `.env` 复制到服务器即可(PostgreSQL + 后端两个容器,均 `unless-stopped` 自重启):

```bash
# 1) 装 Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER   # 重新登录后生效
docker compose version

# 2) 拉代码 + 配置 .env(同 5.1)
git clone https://github.com/AXARINE/zhihui_ludeng_project.git
cd zhihui_ludeng_project/backend
cp .env.example .env && vim .env

# 3) 裁剪 compose(生产)
#    - 删除 nocodb 整个服务(仅本地看数据用);
#    - 删除 postgres 的 ports: "5432:5432" 映射,数据库绝不对外;
#    - 后端保留 8080:8080,后续由反向代理收敛到 443。

# 4) 启动
docker compose up -d --build
docker compose ps
docker logs -f streetlight-backend
```

之后更新后端代码:`cd backend && ./dev.sh update`(git pull --ff-only → 重建 backend 容器 → 等待健康检查,数据库等其他服务不动)。

看到 `database migrated` 与 `http listening on 0.0.0.0:8080` 即成功;首次启动自动建表并创建引导管理员。

### 5.4 HTTPS 反向代理(推荐 Caddy)

```bash
sudo apt install -y caddy
```

`/etc/caddy/Caddyfile`:

```
streetlight.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

```bash
sudo systemctl reload caddy
```

之后访问 `https://streetlight.example.com/docs`;云服务器安全组只放行 443。

### 5.5 上线加固

- `JWT_SECRET` 必须替换为随机值;
- `BOOTSTRAP_ADMIN_PASSWORD` 首次启动前设成强密码;上线后建议用 API 创建正式账号并删除默认 admin;
- 前端与后端不同域名时,把 `backend/src/main.rs` 的 CORS `allow_origin(Any)` 改为前端域名(同域走 Caddy 则无需处理);
- 安全组只放行 80/443,**不放行 5432/8080**。

### 5.6 注册设备(启动后第一步)

后端只轮询已注册的设备:调 `POST /api/devices`(可带 `name`、`location`;ID 与固件 `CONFIG_APP_DEVICEID` 一致)后,该设备才会被纳入影子轮询、在线监控与告警。

## 6. 部署验收清单

| 检查项 | 方法 | 预期 |
|---|---|---|
| 设备在线 | IoTDA 控制台 → 设备状态 | 在线 |
| 数据入库 | Swagger UI → login → Authorize → `GET /api/dashboard` | `reports_24h` 持续增长 |
| 实时光照 | `GET /api/devices/{id}/lux/latest` | 返回当前照度 |
| 历史/统计 | `GET /api/devices/{id}/lux/history?from=&to=`、`/lux/stats` | 条数持续增长、统计正确 |
| 远程控灯 | `POST /api/devices/{id}/lamp` `{"action":"on"}` | 补光灯亮(≤1 个轮询周期内状态回显) |
| 恢复自动 | `{"action":"auto"}` | 回到本地光照联动 |
| 阈值下发 | `PUT /api/devices/{id}/threshold` `{"threshold": 300}` | 设备按新阈值动作 |
| 离线告警 | 拔设备电 → 重新上电 | 产生 offline 告警;恢复后自动消解 |
| 权限隔离 | 市政账号(role_id:1)执行管理操作 | 返回 403 |

> 注意:控灯/阈值是透传 IoTDA 北向的,设备离线时北向拒绝(返回 502 带原因);指令记录中 `sent` 仅表示北向已受理,不代表灯已动作。

## 7. 日常运维

### 7.1 更新

```bash
# 后端(云服务器)
cd zhihui_ludeng_project && git pull
cd backend && docker compose up -d --build   # 新 migration 自动执行

# 固件(本地 WSL)
./build.sh && ./flash.sh 4
```

### 7.2 日志与数据

| 项 | 方法 |
|---|---|
| 后端日志 | `docker logs -f streetlight-backend` |
| 数据库日志 | `docker logs -f streetlight-postgres` |
| 设备日志 | 串口 115200(`pwsh -File bearpi-serial.ps1`);重播启动日志按 RESET |
| 数据持久化 | 数据卷 `streetlight-pgdata` 按名复用,`docker compose down`(不带 `-v`)不丢数据 |
| 备份 | `docker exec streetlight-postgres pg_dump -U streetlight streetlight > backup.sql` |

### 7.3 常见问题

| 症状 | 原因与处置 |
|---|---|
| 北向 API 401 IOTDA.000002 | 签名算法不对(标准版必须 V11 衍生签名)或 IAM 权限未生效(传播延迟数分钟) |
| 阈值下发报 IOTDA.000029 | 产品模型里 `Threshold` 未勾"可读可写" |
| 命令下发超时 IOTDA.014111、设备反复离线 | 误开了 8883 MQTTS(见 4.2 安全说明),改回 1883 |
| 设备连不上云 | 域名填错(标准版无区域共享域名)、仅 2.4G Wi-Fi 可用、设备 ID/密钥不匹配 |
| `./build.sh` 报缺少 app_config.h | 按 `app_config.example.h` 模板创建并填写 |
| HiBurn 退出码 17 / 52 | 板子未连接、窗口被关或 UNC 路径问题;flash.sh 已自动处理,确认 COM 号正确 |
| 串口无输出 | 串口被 HiBurn 独占,关闭后再看;充电器供电时按 RESET |

## 8. 安全说明与限制

- **设备侧 1883 明文 MQTT**:不要启用 8883 MQTTS(Hi3861 iot_link/mbedtls TLS 稳定性问题,见 4.2);设备密钥文件由华为云下载,妥善保管。
- **凭据不进 git**:Wi-Fi 密码、设备密钥、AK/SK 只存在于本地 `app_config.h` 与 `backend/.env`(均被 `.gitignore` 忽略);`.dockerignore` 排除 `.env`,凭据不进镜像。
- **轮询延迟**:影子入库与状态回显存在 5~10s 延迟(后端每 8s 轮询),演示可接受;如需实时可后续升级数据流转(需公网可达的接收端)。
- **北向限流**:轮询间隔固定 8s,勿过快调用,避免触发限流。
- **计费**:标准版实例单设备演示在免费额度内;演示结束后及时删除不用的产品/设备。

## 9. 迭代流程(部署后的开发)

- 固件:改 `C3_e53_sc1_pls/`(权威副本,勿直接改 submodule 树)→ `./build.sh` → `./flash.sh 4` → RESET ×2 → 串口验证;
- 后端:改 `backend/src/` → `cargo build` → curl 验证 REST API;新接口必须补 `#[utoipa::path]` 注解并登记进 `openapi.rs`;
- 数据库 schema:上线前可直接改 `migrations/0001_init.sql` 并清卷重建;**上线后必须新建递增迁移**。
