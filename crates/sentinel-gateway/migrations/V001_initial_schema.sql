-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SENTINEL Database Schema V001
-- SQLite WAL mode for crash-safe writes on edge hardware
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- Enable WAL mode for concurrent read/write and crash safety
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Registered sensor nodes
CREATE TABLE IF NOT EXISTS devices (
    device_id     INTEGER PRIMARY KEY,
    device_name   TEXT NOT NULL DEFAULT 'Unnamed Freezer',
    location      TEXT NOT NULL DEFAULT '',
    installed_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at  TEXT,
    firmware_ver  INTEGER NOT NULL DEFAULT 10000,
    is_active     INTEGER NOT NULL DEFAULT 1
);

-- Raw telemetry readings (append-only time series)
CREATE TABLE IF NOT EXISTS readings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id       INTEGER NOT NULL REFERENCES devices(device_id),
    received_at     TEXT NOT NULL DEFAULT (datetime('now')),
    timestamp_secs  INTEGER NOT NULL,
    temp_centi_c    INTEGER NOT NULL,
    door_open       INTEGER NOT NULL DEFAULT 0,
    door_open_secs  INTEGER NOT NULL DEFAULT 0,
    health_score    INTEGER NOT NULL,
    rssi_dbm        INTEGER NOT NULL DEFAULT -50
);

-- Index for time-range queries on the dashboard
CREATE INDEX IF NOT EXISTS idx_readings_device_time
    ON readings(device_id, received_at DESC);

-- Alert history (for audit trail and compliance reporting)
CREATE TABLE IF NOT EXISTS alerts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id     INTEGER NOT NULL REFERENCES devices(device_id),
    severity      TEXT NOT NULL CHECK (severity IN ('WARNING', 'CRITICAL')),
    message       TEXT NOT NULL,
    temp_centi_c  INTEGER NOT NULL,
    health_score  INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    acknowledged  INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_alerts_device
    ON alerts(device_id, created_at DESC);