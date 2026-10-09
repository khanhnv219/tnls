//! SQLite persistence for tunnel profiles.
//!
//! Single `tunnels` table, `user_version = 1`. Rows map 1:1 to
//! [`TunnelConfig`](crate::tunnel::TunnelConfig); validation runs on every
//! read so corrupt rows surface as [`TunnelError::Storage`] instead of
//! silently producing invalid configs.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::tunnel::{TunnelConfig, TunnelError, TunnelInput, TunnelMode};

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS tunnels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    mode TEXT NOT NULL CHECK (mode IN ('local', 'remote', 'dynamic')),
    ssh_target TEXT NOT NULL,
    bind_address TEXT NOT NULL DEFAULT '127.0.0.1',
    local_port INTEGER NOT NULL,
    remote_host TEXT,
    remote_port INTEGER,
    identity_file TEXT,
    auto_start INTEGER NOT NULL DEFAULT 0,
    keep_alive_interval INTEGER NOT NULL DEFAULT 30,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tunnels_local_port ON tunnels(local_port);
"#;

/// Open (creating parent dirs) and migrate a database file.
pub fn open(path: &Path) -> Result<Connection, TunnelError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| TunnelError::Storage(format!("create db dir: {e}")))?;
        }
    }
    let conn =
        Connection::open(path).map_err(|e| TunnelError::Storage(format!("open database: {e}")))?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory database (tests, throwaway checks).
pub fn open_memory() -> Result<Connection, TunnelError> {
    let conn = Connection::open_in_memory()
        .map_err(|e| TunnelError::Storage(format!("open memory db: {e}")))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), TunnelError> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| TunnelError::Storage(format!("read user_version: {e}")))?;
    if version == 0 {
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| TunnelError::Storage(format!("apply schema v1: {e}")))?;
        conn.execute_batch("PRAGMA user_version = 1")
            .map_err(|e| TunnelError::Storage(format!("stamp user_version: {e}")))?;
    } else if version != 1 {
        return Err(TunnelError::Storage(format!(
            "unsupported database version: {version}"
        )));
    }
    Ok(())
}

fn parse_rfc3339(value: String) -> Result<DateTime<Utc>, TunnelError> {
    value
        .parse::<DateTime<Utc>>()
        .map_err(|e| TunnelError::Storage(format!("bad timestamp {value:?}: {e}")))
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<TunnelConfig> {
    let mode_str: String = row.get(2)?;
    let created_str: String = row.get(11)?;
    let updated_str: String = row.get(12)?;
    Ok(TunnelConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        mode: TunnelMode::parse(&mode_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        ssh_target: row.get(3)?,
        bind_address: row.get(4)?,
        local_port: row.get::<_, i64>(5)? as u16,
        remote_host: row.get(6)?,
        remote_port: row.get::<_, Option<i64>>(7)?.map(|p| p as u16),
        identity_file: row.get(8)?,
        auto_start: row.get::<_, i64>(9)? != 0,
        keep_alive_interval: row.get::<_, i64>(10)? as u64,
        created_at: parse_rfc3339(created_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e))
        })?,
        updated_at: parse_rfc3339(updated_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(12, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}

/// Insert a new tunnel from validated input; returns the stored config.
pub fn create(conn: &Connection, input: TunnelInput) -> Result<TunnelConfig, TunnelError> {
    let config = input.into_config()?;
    conn.execute(
        "INSERT INTO tunnels (id, name, mode, ssh_target, bind_address, local_port,
            remote_host, remote_port, identity_file, auto_start, keep_alive_interval,
            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            config.id,
            config.name,
            config.mode.as_str(),
            config.ssh_target,
            config.bind_address,
            config.local_port as i64,
            config.remote_host,
            config.remote_port.map(|p| p as i64),
            config.identity_file,
            i64::from(config.auto_start),
            config.keep_alive_interval as i64,
            config.created_at.to_rfc3339(),
            config.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| TunnelError::Storage(format!("insert tunnel: {e}")))?;
    Ok(config)
}

/// Fetch one tunnel by id.
pub fn get(conn: &Connection, id: &str) -> Result<TunnelConfig, TunnelError> {
    let mut stmt = conn
        .prepare("SELECT * FROM tunnels WHERE id = ?1")
        .map_err(|e| TunnelError::Storage(format!("prepare get: {e}")))?;
    let config = stmt
        .query_row([id], row_to_config)
        .optional()
        .map_err(|e| TunnelError::Storage(format!("get tunnel: {e}")))?
        .ok_or_else(|| TunnelError::NotFound(id.to_owned()))?;
    config.validate()?;
    Ok(config)
}

/// List all tunnels ordered by name.
pub fn list(conn: &Connection) -> Result<Vec<TunnelConfig>, TunnelError> {
    let mut stmt = conn
        .prepare("SELECT * FROM tunnels ORDER BY name ASC")
        .map_err(|e| TunnelError::Storage(format!("prepare list: {e}")))?;
    let rows = stmt
        .query_map([], row_to_config)
        .map_err(|e| TunnelError::Storage(format!("list tunnels: {e}")))?;
    let mut out = Vec::new();
    for row in rows {
        let config = row.map_err(|e| TunnelError::Storage(format!("decode row: {e}")))?;
        config.validate()?;
        out.push(config);
    }
    Ok(out)
}

/// Replace the stored profile, preserving id/created_at.
pub fn update(
    conn: &Connection,
    id: &str,
    input: TunnelInput,
) -> Result<TunnelConfig, TunnelError> {
    let mut current = get(conn, id)?;
    current.apply_update(input)?;
    let changed = conn
        .execute(
            "UPDATE tunnels SET name=?1, mode=?2, ssh_target=?3, bind_address=?4,
                local_port=?5, remote_host=?6, remote_port=?7, identity_file=?8,
                auto_start=?9, keep_alive_interval=?10, updated_at=?11
             WHERE id=?12",
            params![
                current.name,
                current.mode.as_str(),
                current.ssh_target,
                current.bind_address,
                current.local_port as i64,
                current.remote_host,
                current.remote_port.map(|p| p as i64),
                current.identity_file,
                i64::from(current.auto_start),
                current.keep_alive_interval as i64,
                current.updated_at.to_rfc3339(),
                current.id,
            ],
        )
        .map_err(|e| TunnelError::Storage(format!("update tunnel: {e}")))?;
    if changed == 0 {
        return Err(TunnelError::NotFound(id.to_owned()));
    }
    Ok(current)
}

/// Delete one tunnel; errors when the id does not exist.
pub fn delete(conn: &Connection, id: &str) -> Result<(), TunnelError> {
    let changed = conn
        .execute("DELETE FROM tunnels WHERE id = ?1", [id])
        .map_err(|e| TunnelError::Storage(format!("delete tunnel: {e}")))?;
    if changed == 0 {
        return Err(TunnelError::NotFound(id.to_owned()));
    }
    Ok(())
}

/// Profiles flagged for auto-start, ordered by name.
pub fn list_auto_start(conn: &Connection) -> Result<Vec<TunnelConfig>, TunnelError> {
    let mut stmt = conn
        .prepare("SELECT * FROM tunnels WHERE auto_start = 1 ORDER BY name ASC")
        .map_err(|e| TunnelError::Storage(format!("prepare auto-start list: {e}")))?;
    let rows = stmt
        .query_map([], row_to_config)
        .map_err(|e| TunnelError::Storage(format!("list auto-start: {e}")))?;
    let mut out = Vec::new();
    for row in rows {
        let config = row.map_err(|e| TunnelError::Storage(format!("decode row: {e}")))?;
        config.validate()?;
        out.push(config);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnel::TunnelMode;

    fn local_input(name: &str) -> TunnelInput {
        TunnelInput {
            name: name.to_owned(),
            mode: TunnelMode::Local,
            ssh_target: "dev-bastion".to_owned(),
            bind_address: None,
            local_port: 5433,
            remote_host: Some("postgres.internal".to_owned()),
            remote_port: Some(5432),
            identity_file: None,
            auto_start: None,
            keep_alive_interval: None,
        }
    }

    #[test]
    fn crud_roundtrip() {
        let conn = open_memory().unwrap();
        let created = create(&conn, local_input("db")).unwrap();
        assert_eq!(created.bind_address, "127.0.0.1");

        let fetched = get(&conn, &created.id).unwrap();
        assert_eq!(fetched, created);

        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 1);

        let mut upd = local_input("db-v2");
        upd.local_port = 5434;
        let updated = update(&conn, &created.id, upd).unwrap();
        assert_eq!(updated.name, "db-v2");
        assert_eq!(updated.local_port, 5434);
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.created_at, created.created_at);

        delete(&conn, &created.id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert_eq!(
            get(&conn, &created.id).unwrap_err(),
            TunnelError::NotFound(created.id.clone())
        );
    }

    #[test]
    fn missing_ids_error() {
        let conn = open_memory().unwrap();
        assert!(matches!(
            get(&conn, "nope").unwrap_err(),
            TunnelError::NotFound(_)
        ));
        assert!(matches!(
            update(&conn, "nope", local_input("x")).unwrap_err(),
            TunnelError::NotFound(_)
        ));
        assert!(matches!(
            delete(&conn, "nope").unwrap_err(),
            TunnelError::NotFound(_)
        ));
    }

    #[test]
    fn rejects_invalid_input() {
        let conn = open_memory().unwrap();
        let mut bad = local_input("bad");
        bad.local_port = 0;
        assert!(create(&conn, bad).is_err());
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn auto_start_filter() {
        let conn = open_memory().unwrap();
        let mut on = local_input("auto");
        on.auto_start = Some(true);
        create(&conn, on).unwrap();
        create(&conn, local_input("manual")).unwrap();

        let flagged = list_auto_start(&conn).unwrap();
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].name, "auto");
    }

    #[test]
    fn migrate_idempotent_and_versioned() {
        let conn = open_memory().unwrap();
        migrate(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn file_open_creates_parent_dirs() {
        let dir = std::env::temp_dir().join(format!("tnls-test-{}", uuid::Uuid::new_v4()));
        let db = dir.join("nested").join("tunnels.db");
        let conn = open(&db).unwrap();
        create(&conn, local_input("file-db")).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 1);
        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
