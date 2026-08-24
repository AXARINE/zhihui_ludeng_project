-- =====================================================================
-- 智慧路灯 - 基础数据库建库脚本（第一阶段：普通数据库）
-- 数据库名：smart_street_light
-- 字符集  ：utf8mb4（支持中文）
-- 引擎    ：InnoDB
-- 适用    ：MySQL 5.7+ / 8.0
--
-- 运行方式：
--   mysql -u root -p < smart_street_light.sql
-- =====================================================================

CREATE DATABASE IF NOT EXISTS `smart_street_light`
  DEFAULT CHARACTER SET utf8mb4
  DEFAULT COLLATE utf8mb4_general_ci;

USE `smart_street_light`;

-- ---------------------------------------------------------------------
-- 1. 设备表 device
--    对应功能：路灯设备管理、设备状态监控
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `device` (
  `id`             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `device_id`      VARCHAR(64)     NOT NULL                COMMENT '设备唯一标识（对应华为云/板子设备ID）',
  `name`           VARCHAR(128)    NOT NULL DEFAULT ''     COMMENT '设备名称',
  `location`       VARCHAR(255)    NOT NULL DEFAULT ''     COMMENT '安装位置/路段',
  `online_status`  TINYINT         NOT NULL DEFAULT 0      COMMENT '在线状态：0-离线 1-在线',
  `lamp_status`    TINYINT         NOT NULL DEFAULT 0      COMMENT '灯开关状态：0-关灯 1-开灯',
  `last_heartbeat` DATETIME        NULL     DEFAULT NULL   COMMENT '最后心跳时间',
  `created_at`     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `updated_at`     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                   ON UPDATE CURRENT_TIMESTAMP             COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_device_id` (`device_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='路灯设备表';

-- ---------------------------------------------------------------------
-- 2. 光照历史数据表 luminance_data（时序表，数据量最大）
--    对应功能：实时监测、历史光照趋势折线图
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `luminance_data` (
  `id`         BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `device_id`  VARCHAR(64)     NOT NULL                COMMENT '设备标识',
  `luminance`  FLOAT           NOT NULL                COMMENT '光照强度(lux)',
  `created_at` DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '上报时间',
  PRIMARY KEY (`id`),
  KEY `idx_device_time` (`device_id`, `created_at`)    COMMENT '按设备+时间查询历史，必加索引'
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='光照历史数据(时序)';

-- ---------------------------------------------------------------------
-- 3. 光照阈值配置表 threshold_config
--    对应功能：阈值设置、路灯光照联动（自动开关）
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `threshold_config` (
  `id`             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `device_id`      VARCHAR(64)     NOT NULL                COMMENT '设备标识',
  `low_threshold`  FLOAT           NOT NULL DEFAULT 100.0  COMMENT '光照低于此值自动开灯(lux)',
  `high_threshold` FLOAT           NOT NULL DEFAULT 300.0  COMMENT '光照高于此值自动关灯(lux)',
  `updated_at`     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                   ON UPDATE CURRENT_TIMESTAMP             COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_device_id` (`device_id`)                  COMMENT '每个设备一条阈值配置'
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='光照阈值配置表';

-- ---------------------------------------------------------------------
-- 4. 告警记录表 alarm_record
--    对应功能：设备离线告警、告警日志查看
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `alarm_record` (
  `id`          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `device_id`   VARCHAR(64)     NOT NULL                COMMENT '设备标识',
  `alarm_type`  VARCHAR(32)     NOT NULL DEFAULT 'offline' COMMENT '告警类型：offline-离线',
  `message`     VARCHAR(255)    NOT NULL DEFAULT ''     COMMENT '告警内容',
  `status`      TINYINT         NOT NULL DEFAULT 0      COMMENT '处理状态：0-未处理 1-已处理',
  `created_at`  DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '告警时间',
  `resolved_at` DATETIME        NULL     DEFAULT NULL   COMMENT '处理时间',
  PRIMARY KEY (`id`),
  KEY `idx_device_time` (`device_id`, `created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='告警记录表';

-- ---------------------------------------------------------------------
-- 5. 控制指令记录表 command_record
--    对应功能：路灯手动控制、光照联动自动控制（结果反馈留痕）
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `command_record` (
  `id`          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `device_id`   VARCHAR(64)     NOT NULL                COMMENT '设备标识',
  `command_type` VARCHAR(16)    NOT NULL                COMMENT '指令类型：on-开灯 off-关灯',
  `source`      VARCHAR(16)     NOT NULL DEFAULT 'manual' COMMENT '来源：manual-手动 auto-自动联动',
  `status`      VARCHAR(16)     NOT NULL DEFAULT 'sent' COMMENT '状态：sent-已下发 success-成功 failed-失败',
  `message`     VARCHAR(255)    NOT NULL DEFAULT ''     COMMENT '结果反馈信息',
  `created_at`  DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '下发时间',
  `executed_at` DATETIME        NULL     DEFAULT NULL   COMMENT '执行/反馈时间',
  PRIMARY KEY (`id`),
  KEY `idx_device_time` (`device_id`, `created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='路灯控制指令记录表';

-- =====================================================================
-- 权限体系（RBAC）—— 对应《基本功能清单》中的两个角色
--   市政人员（municipal）：数据监测 / 数据可视化 / 设备控制 / 参数管理 / 离线告警
--   路灯管理员（admin）  ：路灯设备管理 / 告警日志查看 / 维护智能问答
-- 说明：以下 4 张表用于登录鉴权与功能级权限隔离
-- =====================================================================

-- ---------------------------------------------------------------------
-- 6. 角色表 role
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `role` (
  `id`          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `role_code`   VARCHAR(32)     NOT NULL                COMMENT '角色编码（唯一，municipal/admin）',
  `role_name`   VARCHAR(64)     NOT NULL                COMMENT '角色名称',
  `description` VARCHAR(255)    NOT NULL DEFAULT ''     COMMENT '角色描述',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_role_code` (`role_code`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='角色表';

-- ---------------------------------------------------------------------
-- 7. 权限(功能)表 permission —— 每个功能点一条
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `permission` (
  `id`          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `perm_code`   VARCHAR(64)     NOT NULL                COMMENT '权限编码（唯一）',
  `perm_name`   VARCHAR(64)     NOT NULL                COMMENT '功能名称',
  `module`      VARCHAR(32)     NOT NULL DEFAULT ''     COMMENT '所属功能模块',
  `description` VARCHAR(255)    NOT NULL DEFAULT ''     COMMENT '功能描述',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_perm_code` (`perm_code`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='权限(功能)表';

-- ---------------------------------------------------------------------
-- 8. 角色-权限关联表 role_permission
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `role_permission` (
  `id`            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `role_id`       BIGINT UNSIGNED NOT NULL                COMMENT '角色ID -> role.id',
  `permission_id` BIGINT UNSIGNED NOT NULL                COMMENT '权限ID -> permission.id',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_role_perm` (`role_id`, `permission_id`),
  CONSTRAINT `fk_rp_role` FOREIGN KEY (`role_id`)       REFERENCES `role` (`id`)       ON DELETE CASCADE,
  CONSTRAINT `fk_rp_perm` FOREIGN KEY (`permission_id`) REFERENCES `permission` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='角色-权限关联表';

-- ---------------------------------------------------------------------
-- 9. 用户表 user
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `user` (
  `id`            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `username`      VARCHAR(64)     NOT NULL                COMMENT '登录账号',
  `password_hash` VARCHAR(255)    NOT NULL                COMMENT '密码哈希（生产用 bcrypt/argon2，勿存明文）',
  `real_name`     VARCHAR(64)     NOT NULL DEFAULT ''     COMMENT '姓名',
  `role_id`       BIGINT UNSIGNED NOT NULL                COMMENT '角色ID -> role.id',
  `status`        TINYINT         NOT NULL DEFAULT 1      COMMENT '账号状态：0-禁用 1-启用',
  `created_at`    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `updated_at`    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                  ON UPDATE CURRENT_TIMESTAMP             COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_username` (`username`),
  KEY `idx_role_id` (`role_id`),
  CONSTRAINT `fk_user_role` FOREIGN KEY (`role_id`) REFERENCES `role` (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='用户表';

-- ---------------------------------------------------------------------
-- 10. 维护知识库 maintenance_knowledge
--    对应功能：维护智能问答（本地检索，按故障现象给原因+维护建议）
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS `maintenance_knowledge` (
  `id`         BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键',
  `keyword`    VARCHAR(64)     NOT NULL                COMMENT '故障现象关键词',
  `category`   VARCHAR(32)     NOT NULL DEFAULT ''     COMMENT '故障分类',
  `cause`      VARCHAR(255)    NOT NULL                COMMENT '可能原因',
  `suggestion` VARCHAR(500)    NOT NULL                COMMENT '维护建议',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_keyword` (`keyword`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='维护知识库（智能问答检索用）';

-- =====================================================================
-- 初始化数据：角色与权限配置（真实配置，勿删）
-- 说明：账号 / 设备 / 光照 / 告警 / 指令 等数据一律由后端接口实时写入，
--       建库脚本不再预置任何模拟数据。
-- =====================================================================

-- 角色
INSERT INTO `role` (`id`, `role_code`, `role_name`, `description`) VALUES
  (1, 'municipal', '市政人员',   '实时监测、数据可视化、设备控制、参数管理、离线告警'),
  (2, 'admin',     '路灯管理员', '路灯设备统一管理、告警日志查看、维护知识问答');

-- 权限（10 个功能点，与《基本功能清单》一一对应）
INSERT INTO `permission` (`id`, `perm_code`, `perm_name`, `module`, `description`) VALUES
  (1,  'luminance:monitor', '光照强度监测', '数据监测',   '实时展示当前光照强度数值'),
  (2,  'luminance:history', '历史光照趋势', '数据可视化', '折线图展示历史光照强度变化'),
  (3,  'control:linkage',   '路灯光照联动', '设备控制',   '光照低于阈值自动开灯、高于阈值自动关灯'),
  (4,  'control:manual',    '路灯手动控制', '设备控制',   '页面按钮手动远程开关路灯'),
  (5,  'config:threshold',  '阈值设置',     '参数管理',   '设置路灯开关的光照阈值参数'),
  (6,  'device:status',     '设备状态监控', '数据监测',   '查看路灯设备在线/离线状态'),
  (7,  'alarm:offline',     '设备离线告警', '告警管理',   '设备离线时告警通知'),
  (8,  'device:manage',     '路灯设备管理', '系统管理',   '添加、查看、解绑路灯设备'),
  (9,  'alarm:log',         '告警日志查看', '告警管理',   '查看历史告警记录列表'),
  (10, 'assistant:qa',      '维护智能问答', '智能体',     '对话获取维护建议（RAG，暂未实现）');

-- 角色-权限映射
INSERT INTO `role_permission` (`role_id`, `permission_id`) VALUES
  -- 市政人员：功能 1~7
  (1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6), (1, 7),
  -- 路灯管理员：功能 8~10
  (2, 8), (2, 9), (2, 10);

-- 维护知识库（智能问答检索用，按故障现象给原因+建议）
INSERT INTO `maintenance_knowledge` (`keyword`, `category`, `cause`, `suggestion`) VALUES
  ('离线',     '通信故障', '设备掉电、网络中断或网关异常',              '检查路灯供电与网络连接，确认网关在线；若持续离线需现场排查'),
  ('光照异常', '传感器故障', '光敏传感器被遮挡、老化或接线松动',        '清洁传感器表面，检查接线是否牢固；必要时校准或更换传感器'),
  ('频繁开关', '阈值配置',   '光照阈值设置不合理，导致灯在阈值附近反复开关（抖光）', '增大上下限差值（滞回区间），或开启连续多次确认防抖'),
  ('通信超时', '网络故障',   '网络信号弱或平台下发指令超时',            '检查设备信号强度与服务器连通性，重试指令，排查网关与运营商网络'),
  ('灯不亮',   '电源故障',   '供电异常、驱动或灯珠损坏',                '检查供电与驱动电源，确认 lamp_status，更换损坏灯珠或驱动板'),
  ('温度过高', '散热故障',   '散热不良或环境温度过高',                  '检查散热片与通风，必要时降低亮度或更换散热结构');

-- 验证：查看表结构
-- SHOW TABLES;
-- SELECT * FROM device;
