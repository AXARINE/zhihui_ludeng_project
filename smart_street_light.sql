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
-- 示例数据（可删除，仅用于验证结构是否正常）
-- =====================================================================
INSERT INTO `device` (`device_id`, `name`, `location`, `online_status`)
VALUES ('demo_street_lamp_001', '1号路灯', '人民路南段', 0);

INSERT INTO `threshold_config` (`device_id`, `low_threshold`, `high_threshold`)
VALUES ('demo_street_lamp_001', 100.0, 300.0);

INSERT INTO `luminance_data` (`device_id`, `luminance`)
VALUES ('demo_street_lamp_001', 245.5),
       ('demo_street_lamp_001', 132.0),
       ('demo_street_lamp_001', 89.2);

-- 验证：查看表结构
-- SHOW TABLES;
-- SELECT * FROM device;
