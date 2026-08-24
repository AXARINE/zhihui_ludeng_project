-- 账号 / RBAC 权限体系(对应 Python 版 role / permission / role_permission / user)
-- 注意:user 是 PostgreSQL 保留关键字,业务表命名为 app_user
-- 权限码与路由的映射见 src/api.rs / src/auth.rs 中的 require("...") 调用

CREATE TABLE IF NOT EXISTS role (
    id          BIGSERIAL PRIMARY KEY,
    role_code   TEXT NOT NULL UNIQUE,          -- municipal / admin
    role_name   TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS permission (
    id          BIGSERIAL PRIMARY KEY,
    perm_code   TEXT NOT NULL UNIQUE,          -- 如 device:manage
    perm_name   TEXT NOT NULL,
    module      TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS role_permission (
    id            BIGSERIAL PRIMARY KEY,
    role_id       BIGINT NOT NULL REFERENCES role(id)       ON DELETE CASCADE,
    permission_id BIGINT NOT NULL REFERENCES permission(id) ON DELETE CASCADE,
    UNIQUE (role_id, permission_id)
);

CREATE TABLE IF NOT EXISTS app_user (
    id            BIGSERIAL PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,               -- Argon2id
    real_name     TEXT NOT NULL DEFAULT '',
    role_id       BIGINT NOT NULL REFERENCES role(id),
    status        SMALLINT NOT NULL DEFAULT 1, -- 0 禁用 / 1 启用
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------- 种子数据 ----------
INSERT INTO role (id, role_code, role_name, description) VALUES
  (1, 'municipal', '市政人员',   '实时监测、数据可视化、设备控制、参数管理、离线告警'),
  (2, 'admin',     '路灯管理员', '路灯设备管理、告警日志查看、账号与权限管理、维护知识问答');

INSERT INTO permission (id, perm_code, perm_name, module, description) VALUES
  (1,  'luminance:monitor', '光照强度监测', '数据监测',   '实时展示当前光照强度数值'),
  (2,  'luminance:history', '历史光照趋势', '数据可视化', '折线图展示历史光照强度变化'),
  (3,  'control:linkage',   '路灯光照联动', '设备控制',   '光照低于阈值自动开灯、高于阈值自动关灯'),
  (4,  'control:manual',    '路灯手动控制', '设备控制',   '页面按钮手动远程开关路灯'),
  (5,  'config:threshold',  '阈值设置',     '参数管理',   '设置路灯开关的光照阈值参数'),
  (6,  'device:status',     '设备状态监控', '数据监测',   '查看路灯设备在线/离线状态'),
  (7,  'alarm:offline',     '设备离线告警', '告警管理',   '设备离线时告警通知'),
  (8,  'device:manage',     '路灯设备管理', '系统管理',   '添加、查看、解绑路灯设备'),
  (9,  'alarm:log',         '告警日志查看', '告警管理',   '查看历史告警记录列表'),
  (10, 'assistant:qa',      '维护智能问答', '智能体',     '对话获取维护建议'),
  (11, 'command:log',       '控制指令留痕', '系统管理',   '查看控制指令下发记录'),
  (12, 'user:manage',       '账号与权限管理', '系统管理', '管理登录账号、角色与权限映射');

-- 路灯管理员拥有全部权限
INSERT INTO role_permission (role_id, permission_id)
SELECT r.id, p.id FROM role r, permission p WHERE r.role_code = 'admin';

-- 市政人员:监测 + 可视化 + 控制 + 参数 + 告警 + 指令留痕(不能管设备/账号/知识问答)
INSERT INTO role_permission (role_id, permission_id)
SELECT r.id, p.id
FROM role r, permission p
WHERE r.role_code = 'municipal'
  AND p.perm_code IN (
    'luminance:monitor', 'luminance:history', 'control:linkage', 'control:manual',
    'config:threshold', 'device:status', 'alarm:offline', 'alarm:log', 'command:log'
  );
