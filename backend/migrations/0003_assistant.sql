-- 维护智能问答：知识库表 + 种子数据（对应功能清单第 10 项，权限码 assistant:qa 已在 0002 定义）
CREATE TABLE IF NOT EXISTS maintenance_knowledge (
    id         BIGSERIAL PRIMARY KEY,
    keyword    VARCHAR(64) NOT NULL UNIQUE,
    category   VARCHAR(32) NOT NULL DEFAULT '',
    cause      VARCHAR(255) NOT NULL,
    suggestion VARCHAR(500) NOT NULL
);

INSERT INTO maintenance_knowledge (keyword, category, cause, suggestion) VALUES
  ('离线',     '通信故障', '设备掉电、网络中断或网关异常',              '检查路灯供电与网络连接，确认网关在线；若持续离线需现场排查'),
  ('光照异常', '传感器故障', '光敏传感器被遮挡、老化或接线松动',        '清洁传感器表面，检查接线是否牢固；必要时校准或更换传感器'),
  ('频繁开关', '阈值配置',   '光照阈值设置不合理，导致灯在阈值附近反复开关（抖光）', '增大阈值滞回区间，或开启连续多次确认防抖'),
  ('通信超时', '网络故障',   '网络信号弱或平台下发指令超时',            '检查设备信号强度与服务器连通性，重试指令，排查网关与运营商网络'),
  ('灯不亮',   '电源故障',   '供电异常、驱动或灯珠损坏',                '检查供电与驱动电源，确认 lamp 状态，更换损坏灯珠或驱动板'),
  ('温度过高', '散热故障',   '散热不良或环境温度过高',                  '检查散热片与通风，必要时降低亮度或更换散热结构');
