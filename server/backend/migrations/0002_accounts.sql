-- 账号管理 + RBAC + 审计日志 + 智能问答

-- 角色表
CREATE TABLE IF NOT EXISTS role (
    id BIGSERIAL PRIMARY KEY,
    role_code VARCHAR(32) NOT NULL UNIQUE,
    role_name VARCHAR(64) NOT NULL,
    description VARCHAR(255) NOT NULL DEFAULT ''
);

-- 权限表
CREATE TABLE IF NOT EXISTS permission (
    id BIGSERIAL PRIMARY KEY,
    perm_code VARCHAR(64) NOT NULL UNIQUE,
    perm_name VARCHAR(64) NOT NULL,
    module VARCHAR(32) NOT NULL DEFAULT '',
    description VARCHAR(255) NOT NULL DEFAULT ''
);

-- 角色-权限关联表
CREATE TABLE IF NOT EXISTS role_permission (
    id BIGSERIAL PRIMARY KEY,
    role_id BIGINT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    permission_id BIGINT NOT NULL REFERENCES permission(id) ON DELETE CASCADE,
    UNIQUE(role_id, permission_id)
);

-- 用户表
CREATE TABLE IF NOT EXISTS users (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    real_name VARCHAR(64) NOT NULL DEFAULT '',
    role_id BIGINT NOT NULL REFERENCES role(id),
    status SMALLINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_user_role ON users(role_id);

-- 控制指令审计表
CREATE TABLE IF NOT EXISTS command_record (
    id BIGSERIAL PRIMARY KEY,
    device_id VARCHAR(64) NOT NULL,
    command_type VARCHAR(16) NOT NULL,
    source VARCHAR(16) NOT NULL DEFAULT 'manual',
    status VARCHAR(16) NOT NULL DEFAULT 'sent',
    message VARCHAR(255) NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    executed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_command_device_time ON command_record(device_id, created_at);

-- 路灯维护知识库
CREATE TABLE IF NOT EXISTS maintenance_knowledge (
    id BIGSERIAL PRIMARY KEY,
    keyword VARCHAR(64) NOT NULL UNIQUE,
    category VARCHAR(32) NOT NULL DEFAULT '',
    cause VARCHAR(255) NOT NULL,
    suggestion VARCHAR(500) NOT NULL
);

-- 种子数据：角色
INSERT INTO role (role_code, role_name, description) VALUES
    ('municipal', '市政人员', '查看光照、控制路灯、设置阈值、查看状态和离线告警'),
    ('admin', '路灯管理员', '设备管理、告警日志、智能问答')
ON CONFLICT (role_code) DO NOTHING;

-- 种子数据：权限
INSERT INTO permission (perm_code, perm_name, module, description) VALUES
    ('luminance:monitor', '实时光照监测', '数据监测', '查看当前光照强度'),
    ('luminance:history', '历史光照查询', '数据可视化', '查看历史光照趋势'),
    ('control:linkage', '光照联动控制', '设备控制', '自动开关灯'),
    ('control:manual', '手动远程控制', '设备控制', '手动开关灯'),
    ('config:threshold', '阈值设置', '参数管理', '设置光照阈值'),
    ('device:status', '设备状态监控', '数据监测', '查看在线离线状态'),
    ('alarm:offline', '离线告警接收', '告警管理', '接收离线通知'),
    ('device:manage', '设备管理', '系统管理', '添加删除设备'),
    ('alarm:log', '告警日志查看', '告警管理', '查看历史告警'),
    ('assistant:qa', '维护智能问答', '智能体', '路灯维护问答')
ON CONFLICT (perm_code) DO NOTHING;

-- 种子数据：角色-权限关联
INSERT INTO role_permission (role_id, permission_id)
SELECT r.id, p.id FROM role r, permission p
WHERE r.role_code = 'municipal' AND p.perm_code IN (
    'luminance:monitor', 'luminance:history', 'control:linkage',
    'control:manual', 'config:threshold', 'device:status', 'alarm:offline'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

INSERT INTO role_permission (role_id, permission_id)
SELECT r.id, p.id FROM role r, permission p
WHERE r.role_code = 'admin' AND p.perm_code IN (
    'device:manage', 'alarm:log', 'assistant:qa'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- 种子数据：维护知识库
INSERT INTO maintenance_knowledge (keyword, category, cause, suggestion) VALUES
    ('灯不亮', '照明故障', '灯泡损坏、线路断路、电源故障', '检查灯泡是否烧坏，检查线路连接，测试电源电压'),
    ('灯闪烁', '照明故障', '电压不稳、接触不良、驱动器故障', '检查电源电压稳定性，紧固接线端子，更换驱动器'),
    ('传感器异常', '传感器故障', 'BH1750损坏、I2C通信故障、线路松动', '检查传感器焊接，重新插拔I2C连接线，更换传感器'),
    ('设备离线', '通信故障', 'WiFi断开、MQTT连接中断、电源掉电', '检查WiFi信号强度，重启设备，检查电源适配器'),
    ('光照不准', '传感器故障', '传感器被遮挡、校准偏移、环境光干扰', '清洁传感器表面，重新校准，避免强光直射')
ON CONFLICT (keyword) DO NOTHING;
