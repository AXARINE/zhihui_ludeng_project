-- 智慧路灯 PostgreSQL 建库脚本(后端启动时由 sqlx::migrate! 自动执行)
-- 设备未正式部署,本文件允许原地修改;一旦上线,后续变更必须新建 0002_*.sql 递增迁移

CREATE TABLE IF NOT EXISTS device (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',        -- 安装位置/路段
    status TEXT NOT NULL DEFAULT 'offline',   -- online/offline,以 IoTDA 设备状态为准
    lamp TEXT NOT NULL DEFAULT 'off',         -- 灯态(影子 LightStatus)
    mode TEXT NOT NULL DEFAULT 'auto',        -- auto/manual
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS lux_record (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    lux REAL NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_lux_record_device_time ON lux_record (device_id, created_at);

CREATE TABLE IF NOT EXISTS config (
    device_id TEXT PRIMARY KEY,
    threshold REAL NOT NULL DEFAULT 40
);

CREATE TABLE IF NOT EXISTS alarm (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    type TEXT NOT NULL,
    message TEXT NOT NULL DEFAULT '',         -- 告警内容
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ                   -- 非空即已消解
);

-- 控制指令留痕:谁给哪盏灯下过什么指令、北向是否接受
-- 注意:固件侧执行结果不回传北向,status 只能到 sent/failed,没有 executed_at
CREATE TABLE IF NOT EXISTS command_record (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    action TEXT NOT NULL,                     -- on/off/auto
    source TEXT NOT NULL DEFAULT 'manual',    -- manual(目前只有手动;auto 联动是固件本地行为,不经过后端)
    status TEXT NOT NULL DEFAULT 'sent',      -- sent=北向已接受 failed=北向拒绝/异常
    message TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_command_record_device_time ON command_record (device_id, created_at);
