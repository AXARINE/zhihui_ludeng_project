-- 通知系统 + 每日日报
-- 通知类型 type: alert-维修通知 / report-日报；receiver_role: 收件角色（'all'=全体）
CREATE TABLE IF NOT EXISTS notification (
    id            BIGSERIAL PRIMARY KEY,
    title         VARCHAR(128) NOT NULL,
    content       TEXT         NOT NULL DEFAULT '',
    type          VARCHAR(16)  NOT NULL DEFAULT 'alert',
    device_id     VARCHAR(64),
    receiver_role VARCHAR(32)  NOT NULL DEFAULT 'admin',
    is_read       BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_notification_role ON notification (receiver_role, is_read, created_at DESC);

-- 每日日报（每天 09:00 由后端定时任务生成，懒生成兜底）
CREATE TABLE IF NOT EXISTS daily_report (
    id          BIGSERIAL PRIMARY KEY,
    report_date DATE    NOT NULL UNIQUE,
    content     JSONB   NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 权限：发送维修通知（市政人员 / 系统管理员）
INSERT INTO permission (id, perm_code, perm_name, module, description) VALUES
  (14, 'notify:send', '发送维修通知', '告警管理', '对异常路灯发起维修通知（市政人员/系统管理员）')
ON CONFLICT (id) DO NOTHING;

INSERT INTO role_permission (role_id, permission_id)
SELECT r.id, p.id FROM role r, permission p
WHERE r.role_code IN ('super_admin', 'municipal') AND p.perm_code = 'notify:send'
ON CONFLICT DO NOTHING;

SELECT setval(pg_get_serial_sequence('permission', 'id'),
              GREATEST((SELECT COALESCE(MAX(id), 0) FROM permission), 14));
