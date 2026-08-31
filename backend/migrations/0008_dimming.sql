-- 调光配置:config 表新增两列 + config:dimming 权限点
--   brightness:手动亮度百分比 0~100(默认 100 全亮),对应产品模型 Light 服务可写属性 Brightness
--   dim_curve:auto 模式照度→亮度曲线锚点串 `lux:pct,lux:pct,...`(≤4 点、lux 严格递增;
--             空串 = 不启用曲线),对应可写属性 DimCurve
ALTER TABLE config
    ADD COLUMN IF NOT EXISTS brightness INTEGER NOT NULL DEFAULT 100,
    ADD COLUMN IF NOT EXISTS dim_curve  TEXT    NOT NULL DEFAULT '';

INSERT INTO permission (id, perm_code, perm_name, module, description) VALUES
  (15, 'config:dimming', '调光设置', '参数管理', '设置路灯手动亮度与照度-亮度曲线')
ON CONFLICT (id) DO NOTHING;

-- 市政人员与路灯管理员:授予调光设置权限(与 config:threshold 同模块同范围)
INSERT INTO role_permission (role_id, permission_id)
SELECT r.id, p.id FROM role r, permission p
WHERE r.role_code IN ('municipal', 'admin') AND p.perm_code = 'config:dimming'
ON CONFLICT DO NOTHING;

-- 系统管理员:授予全部权限(含新增权限点;权限固定不可改,见 0004)
INSERT INTO role_permission (role_id, permission_id)
SELECT 3, p.id FROM permission p
ON CONFLICT DO NOTHING;

-- 推进自增序列,避免后续显式插入冲突
SELECT setval(pg_get_serial_sequence('permission', 'id'),
              GREATEST((SELECT COALESCE(MAX(id), 0) FROM permission), 15));
