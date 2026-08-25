-- 系统管理员角色 + 角色权限管理权限
-- 目的：
--   1) 新增 super_admin（系统管理员）：拥有全部权限，且其权限不可被修改（防止权限管理锁死）
--   2) 新增 role:manage（角色权限管理）权限码，与 user:manage（账号管理）分离：
--      只有 super_admin 能调整角色权限；admin（路灯管理员）保留账号增删但不具备授权能力
INSERT INTO role (id, role_code, role_name, description) VALUES
  (3, 'super_admin', '系统管理员', '拥有全部权限；角色权限不可被修改（防止权限管理锁死）')
ON CONFLICT (id) DO NOTHING;

INSERT INTO permission (id, perm_code, perm_name, module, description) VALUES
  (13, 'role:manage', '角色权限管理', '系统管理', '调整各角色拥有的功能权限（仅系统管理员）')
ON CONFLICT (id) DO NOTHING;

-- 系统管理员：授予全部权限（含 role:manage / user:manage）
INSERT INTO role_permission (role_id, permission_id)
SELECT 3, p.id FROM permission p
ON CONFLICT DO NOTHING;

-- 推进自增序列，避免后续显式插入冲突
SELECT setval(pg_get_serial_sequence('role', 'id'),
              GREATEST((SELECT COALESCE(MAX(id), 0) FROM role), 3));
SELECT setval(pg_get_serial_sequence('permission', 'id'),
              GREATEST((SELECT COALESCE(MAX(id), 0) FROM permission), 13));
