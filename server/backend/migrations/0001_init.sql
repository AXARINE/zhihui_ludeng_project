CREATE TABLE IF NOT EXISTS device (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'offline',
    lamp TEXT NOT NULL DEFAULT 'off',
    mode TEXT NOT NULL DEFAULT 'auto',
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS lux_record (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    lux REAL NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_lux_record_device_time ON lux_record (device_id, created_at);

CREATE TABLE IF NOT EXISTS config (
    device_id TEXT PRIMARY KEY,
    threshold REAL NOT NULL DEFAULT 40
);

CREATE TABLE IF NOT EXISTS alarm (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ
);
