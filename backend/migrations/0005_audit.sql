-- 1) command_record 增加 operator_id:谁下发的控灯指令(老数据为 NULL)
-- 2) 新增 audit_log:用户/角色/阈值等管理操作的审计流水

ALTER TABLE command_record
    ADD COLUMN IF NOT EXISTS operator_id BIGINT REFERENCES app_user(id);

CREATE TABLE IF NOT EXISTS audit_log (
    id         BIGSERIAL PRIMARY KEY,
    actor_id   BIGINT,                            -- 操作者 app_user.id;系统行为为 NULL
    action     TEXT NOT NULL,                     -- user.create / user.update / user.delete / role.perms_update / config.threshold
    target     TEXT NOT NULL DEFAULT '',          -- 目标标识(用户名/角色/设备)
    detail     TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_audit_log_time ON audit_log (created_at);
