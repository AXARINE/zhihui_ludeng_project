-- 设备经纬度(WGS84 坐标系,GPS 原始格式):地图点位用
-- 可空 = 存量设备/尚未定位;前端渲染高德/腾讯底图时自行转 GCJ-02
-- 用 DOUBLE PRECISION 而非 NUMERIC:sqlx 直接映射 f64,免 bigdecimal 依赖;
-- 小数点后 6 位约 0.1m 精度,对路灯点位绰绰有余
ALTER TABLE device
    ADD COLUMN IF NOT EXISTS latitude  DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION;

-- 范围约束幂等添加(ADD CONSTRAINT 无 IF NOT EXISTS 语法)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'device_latitude_range') THEN
        ALTER TABLE device ADD CONSTRAINT device_latitude_range
            CHECK (latitude IS NULL OR latitude BETWEEN -90 AND 90);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'device_longitude_range') THEN
        ALTER TABLE device ADD CONSTRAINT device_longitude_range
            CHECK (longitude IS NULL OR longitude BETWEEN -180 AND 180);
    END IF;
END $$;
