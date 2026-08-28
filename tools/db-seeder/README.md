# 智慧路灯 · 测试数据工具(db-seeder)

Python/PySide6 GUI,**直连本地 PostgreSQL**,不经过后端 API 与鉴权,用于手动/批量灌测试数据。

## 前提

- 数据库已启动:`cd backend && ./dev.sh db`(或 `docker compose up -d postgres`)
- 已安装 [uv](https://docs.astral.sh/uv/)(`curl -LsSf https://astral.sh/uv/install.sh | sh`)

## 启动

```bash
cd tools/db-seeder
uv sync            # 首次:建 .venv、装 psycopg + PySide6
uv run seed_gui.py
```

WSL 下窗口经 WSLg 显示(自动走 wayland 平台,缩放/中文由 Qt + fontconfig 处理;
已设 `QT_QPA_PLATFORM` 时尊重用户设置)。

## 为什么不用 tkinter

uv 的独立 CPython 自带 Tk 是**无 Xft/fontconfig 的精简构建**,只能看到 20 个 X 核心
位图字体(无中文),且不认 TTF——中文界面必然豆腐块。PySide6 走 fontconfig,
中文自动回退,HiDPI 缩放也由 Qt 处理。

## 功能

- **连接区**:预填 compose 默认连接 `127.0.0.1:5432 streetlight/streetlight`;`从 backend/.env 导入` 只解析其中的 `DATABASE_URL`
- **自动检测**:连接后列出 `public` 下全部表(`_sqlx_migrations` 除外);选表后自动读取列名/类型/可空/默认值/主键
- **手动插入**:按列动态生成表单,text 列自动带出现有取值下拉,有默认值的列可勾选"用默认值",单行 INSERT
- **批量插入**:每列独立生成策略(默认/固定值/随机整数/随机选择/时间序列/自增/随机设备ID/NULL),分块 executemany + 单事务,带进度条、可取消(取消即回滚)
- **场景预设**:
  - 光照历史曲线:为设备回填 N 天昼夜正弦光照数据,设备置 online 并刷新心跳
  - 设备上下线:按后端 `apply_online_status` 语义翻转状态,离线产生告警、上线自动消解

## 注意

- 测试设备不在华为云 IoTDA 云端,后端轮询日志出现北向 404 属预期;`IOTHUB_DRY_RUN=true` 可让后端北向调用本地短路
- 直接写库绕过后端业务校验,勿对生产库使用
- 代码结构:`dbcore.py`(数据库核心,无 GUI 依赖)`seed_gui.py`(PySide6 界面)
