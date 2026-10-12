use rusqlite::{params, Connection};
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use sentinel_core::{
    alert::AlertEvent,
    telemetry::{DeviceId, FirmwareVersion, FreezerReading, HealthScore, TempCentiC},
};

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    LockPoisoned,
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(err) => write!(f, "database engine fault: {err}"),
            Self::LockPoisoned => write!(f, "internal database lock poisoned by panicked thread"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(err) => Some(err),
            Self::LockPoisoned => None,
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA cache_size = -2000;",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[inline(always)]
    fn lock(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.conn.lock().map_err(|_| DbError::LockPoisoned)
    }

    pub fn run_migrations(&self) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute_batch(include_str!("../migrations/V001_initial_schema.sql"))?;
        tracing::info!("Database schema migrations applied successfully");
        Ok(())
    }

    pub fn upsert_device(&self, device_id: u32) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO devices (device_id, last_seen_at) VALUES (?1, datetime('now'))
             ON CONFLICT(device_id) DO UPDATE SET last_seen_at = datetime('now')",
            params![device_id],
        )?;
        Ok(())
    }

    pub fn insert_reading(&self, reading: &FreezerReading) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO readings (
                device_id, timestamp_secs, temp_centi_c, door_open,
                door_open_secs, health_score, rssi_dbm
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                reading.device_id.as_u32(),
                reading.timestamp_secs as i64,
                reading.temp.as_centi(),
                reading.door_open as i32,
                reading.door_open_secs as i32,
                reading.health_score.as_u16() as i32,
                reading.rssi_dbm as i32,
            ],
        )?;
        Ok(())
    }

    pub fn insert_alert(&self, alert: &AlertEvent) -> Result<(), DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO alerts (
                device_id, severity, message, temp_centi_c, health_score
              ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                alert.device_id,
                alert.severity.to_string(),
                alert.message,
                alert.temp_centi_c,
                alert.health_score,
            ],
        )?;
        Ok(())
    }

    pub fn get_latest_readings(
        &self,
        device_id: u32,
        limit: u32,
    ) -> Result<Vec<FreezerReading>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, timestamp_secs, temp_centi_c, door_open, door_open_secs, health_score, rssi_dbm
             FROM readings
             WHERE device_id = ?1
             ORDER BY received_at DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![device_id, limit], |row| {
            let dev_raw: u32 = row.get(0)?;
            let ts: u64 = row.get(1)?;
            let tc: i16 = row.get(2)?;
            let door: i32 = row.get(3)?;
            let ds: u16 = row.get(4)?;
            let hs: u16 = row.get(5)?;
            let rs: i8 = row.get(6)?;

            Ok(FreezerReading {
                device_id: DeviceId::new(dev_raw),
                timestamp_secs: ts,
                temp: TempCentiC::from_centi(tc),
                door_open: door != 0,
                door_open_secs: ds,
                health_score: HealthScore::new_saturating(hs),
                rssi_dbm: rs,
                firmware_version: FirmwareVersion::new(1, 0, 0),
            })
        })?;

        let mut readings = Vec::with_capacity(limit as usize);
        for reading in rows {
            readings.push(reading?);
        }

        Ok(readings)
    }

    pub fn get_devices(&self) -> Result<Vec<DeviceInfo>, DbError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT device_id, device_name, location, last_seen_at, is_active FROM devices",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(DeviceInfo {
                device_id: row.get(0)?,
                device_name: row.get(1)?,
                location: row.get(2)?,
                last_seen_at: row.get(3)?,
                is_active: row.get::<_, i32>(4)? != 0,
            })
        })?;

        let mut devices = Vec::new();
        for device in rows {
            devices.push(device?);
        }

        Ok(devices)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub device_id: u32,
    pub device_name: String,
    pub location: String,
    pub last_seen_at: Option<String>,
    pub is_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::alert::{AlertCause, Severity};

    #[test]
    fn test_open_in_memory_and_schema_tables() {
        let db = Database::open_in_memory().expect("failed to open in-memory DB");
        db.run_migrations().expect("failed to apply migrations");

        let conn = db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&String::from("devices")));
        assert!(tables.contains(&String::from("readings")));
        assert!(tables.contains(&String::from("alerts")));
    }

    #[test]
    fn test_upsert_device_idempotency() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        let dev_id: u32 = 0x00A4_F3B2;
        db.upsert_device(dev_id).expect("initial upsert failed");
        let devices = db.get_devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, dev_id);

        db.upsert_device(dev_id).expect("conflict upsert failed");
        let devices_after = db.get_devices().unwrap();
        assert_eq!(devices_after.len(), 1);
    }

    #[test]
    fn test_get_devices_fields() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        assert!(db.get_devices().unwrap().is_empty());

        let dev_id: u32 = 0x0011_2233;
        db.upsert_device(dev_id).unwrap();

        let devices = db.get_devices().unwrap();
        assert_eq!(devices.len(), 1);
        let dev = &devices[0];
        assert_eq!(dev.device_id, dev_id);
        assert_eq!(dev.device_name, "Unnamed Freezer");
        assert_eq!(dev.location, "");
        assert!(dev.is_active);
        assert!(dev.last_seen_at.is_some());
    }

    #[test]
    fn test_insert_and_get_readings() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        let dev_id: u32 = 0xCAFE_BABE;
        db.upsert_device(dev_id).unwrap();

        let reading = FreezerReading {
            device_id: DeviceId::new(dev_id),
            timestamp_secs: 1_710_000_000,
            temp: TempCentiC::from_centi(-1850),
            door_open: false,
            door_open_secs: 0,
            health_score: HealthScore::new(950),
            rssi_dbm: -65,
            firmware_version: FirmwareVersion::new(1, 0, 0),
        };

        db.insert_reading(&reading).expect("failed to insert reading");

        let fetched = db.get_latest_readings(dev_id, 10).unwrap();
        assert_eq!(fetched.len(), 1);
        let r = &fetched[0];
        assert_eq!(r.device_id.as_u32(), dev_id);
        assert_eq!(r.temp.as_centi(), -1850);
        assert_eq!(r.health_score.as_u16(), 950);
        assert_eq!(r.rssi_dbm, -65);
        assert!(!r.door_open);
        assert_eq!(r.door_open_secs, 0);
        assert_eq!(r.timestamp_secs, 1_710_000_000);
    }

    #[test]
    fn test_get_latest_readings_limit_and_order() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        let dev_id: u32 = 0x1122_3344;
        db.upsert_device(dev_id).unwrap();

        for i in 1..=5 {
            let reading = FreezerReading {
                device_id: DeviceId::new(dev_id),
                timestamp_secs: 1_710_000_000 + i,
                temp: TempCentiC::from_centi(-1800 - i as i16 * 10),
                door_open: false,
                door_open_secs: 0,
                health_score: HealthScore::new(900 + i as u16 * 10),
                rssi_dbm: -60,
                firmware_version: FirmwareVersion::new(1, 0, 0),
            };
            db.insert_reading(&reading).unwrap();
        }

        let fetched = db.get_latest_readings(dev_id, 3).unwrap();
        assert_eq!(fetched.len(), 3);
    }

    #[test]
    fn test_get_latest_readings_device_isolation() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        let dev_a: u32 = 0xAAAA_AAAA;
        let dev_b: u32 = 0xBBBB_BBBB;
        db.upsert_device(dev_a).unwrap();
        db.upsert_device(dev_b).unwrap();

        let reading_a = FreezerReading {
            device_id: DeviceId::new(dev_a),
            timestamp_secs: 1_710_000_000,
            temp: TempCentiC::from_centi(-1800),
            door_open: false,
            door_open_secs: 0,
            health_score: HealthScore::new(900),
            rssi_dbm: -70,
            firmware_version: FirmwareVersion::new(1, 0, 0),
        };
        db.insert_reading(&reading_a).unwrap();

        let readings_a = db.get_latest_readings(dev_a, 10).unwrap();
        assert_eq!(readings_a.len(), 1);
        assert_eq!(readings_a[0].device_id.as_u32(), dev_a);

        let readings_b = db.get_latest_readings(dev_b, 10).unwrap();
        assert!(readings_b.is_empty());
    }

    #[test]
    fn test_foreign_key_prevents_orphan_reading() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        let orphan_id: u32 = 0x9999_9999;
        let reading = FreezerReading {
            device_id: DeviceId::new(orphan_id),
            timestamp_secs: 1_710_000_000,
            temp: TempCentiC::from_centi(-1800),
            door_open: false,
            door_open_secs: 0,
            health_score: HealthScore::new(900),
            rssi_dbm: -70,
            firmware_version: FirmwareVersion::new(1, 0, 0),
        };

        let result = db.insert_reading(&reading);
        assert!(result.is_err(), "Expected foreign key constraint violation");
    }

    #[test]
    fn test_insert_alert_and_persistence() {
        let db = Database::open_in_memory().unwrap();
        db.run_migrations().unwrap();

        let dev_id: u32 = 0x00A4_F3B2;
        db.upsert_device(dev_id).unwrap();

        let alert = AlertEvent {
            timestamp_secs: 1_710_000_000,
            message: "Critical temperature threshold breached",
            device_id: dev_id,
            temp_centi_c: -1825,
            door_open_secs: 15,
            health_score: 850,
            severity: Severity::Critical,
            cause: AlertCause::TemperatureBreach,
            door_open: false,
        };

        db.insert_alert(&alert).expect("failed to insert alert");

        let conn = db.lock().unwrap();
        let (count, sev, msg, temp, score): (i64, String, String, i16, u16) = conn
            .query_row(
                "SELECT count(*), severity, message, temp_centi_c, health_score FROM alerts WHERE device_id = ?1",
                params![dev_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(sev, "CRITICAL");
        assert_eq!(msg, "Critical temperature threshold breached");
        assert_eq!(temp, -1825);
        assert_eq!(score, 850);
    }

    #[test]
    fn test_db_error_display_and_source() {
        let poisoned = DbError::LockPoisoned;
        assert!(poisoned.to_string().contains("internal database lock poisoned"));
        assert!(std::error::Error::source(&poisoned).is_none());

        let sqlite_err = DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows);
        assert!(sqlite_err.to_string().contains("database engine fault"));
        assert!(std::error::Error::source(&sqlite_err).is_some());
    }

    #[test]
    fn test_file_based_database_wal_pragmas() {
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("sentinel_db_test_{}.db", std::process::id()));

        let db = Database::open(&temp_path).expect("failed to open file DB");
        db.run_migrations().expect("failed to run migrations");

        let conn = db.lock().unwrap();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let foreign_keys: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);

        drop(conn);
        drop(db);
        let _ = std::fs::remove_file(&temp_path);
        let _ = std::fs::remove_file(format!("{}-wal", temp_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", temp_path.display()));
    }
}
