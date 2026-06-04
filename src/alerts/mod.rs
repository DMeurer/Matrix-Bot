pub mod commands;
pub mod fetcher;
pub mod scheduler;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Alert {
    pub name: String,
    pub url: String,
    pub css: String,
    pub property: String,
    pub schedule: String,
    pub room_id: String,
    pub creator: String,
    pub last_value: Option<String>,
    pub last_checked: Option<i64>,
    pub enabled: bool,
    pub created_at: i64,
}

pub struct AlertDb {
    conn: Mutex<Connection>,
}

impl AlertDb {
    pub fn open(path: &Path) -> Result<Arc<Self>> {
        let conn = Connection::open(path)
            .context("Failed to open alerts database")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS alerts (
                name         TEXT PRIMARY KEY,
                url          TEXT NOT NULL,
                css          TEXT NOT NULL,
                property     TEXT NOT NULL,
                schedule     TEXT NOT NULL,
                room_id      TEXT NOT NULL,
                creator      TEXT NOT NULL,
                last_value   TEXT,
                last_checked INTEGER,
                enabled      INTEGER NOT NULL DEFAULT 1,
                created_at   INTEGER NOT NULL
            );",
        )
        .context("Failed to create alerts table")?;

        Ok(Arc::new(Self {
            conn: Mutex::new(conn),
        }))
    }

    pub async fn insert(&self, alert: &Alert) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO alerts
             (name, url, css, property, schedule, room_id, creator,
              last_value, last_checked, enabled, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                alert.name, alert.url, alert.css, alert.property, alert.schedule,
                alert.room_id, alert.creator, alert.last_value, alert.last_checked,
                alert.enabled as i64, alert.created_at,
            ],
        )
        .context("Failed to insert alert")?;
        Ok(())
    }

    pub async fn get(&self, name: &str) -> Result<Option<Alert>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT name,url,css,property,schedule,room_id,creator,
                    last_value,last_checked,enabled,created_at
             FROM alerts WHERE name = ?1",
        )?;
        let mut rows = stmt.query(params![name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_alert(row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn load_all(&self) -> Result<Vec<Alert>> {
        let conn = self.conn.lock().await;
        load_alerts_with(&conn, "SELECT name,url,css,property,schedule,room_id,creator,last_value,last_checked,enabled,created_at FROM alerts ORDER BY name")
    }

    pub async fn load_enabled(&self) -> Result<Vec<Alert>> {
        let conn = self.conn.lock().await;
        load_alerts_with(&conn, "SELECT name,url,css,property,schedule,room_id,creator,last_value,last_checked,enabled,created_at FROM alerts WHERE enabled=1 ORDER BY name")
    }

    pub async fn load_by_creator(&self, creator: &str) -> Result<Vec<Alert>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT name,url,css,property,schedule,room_id,creator,last_value,last_checked,enabled,created_at
             FROM alerts WHERE creator = ?1 ORDER BY name",
        )?;
        collect_alerts(stmt.query(params![creator])?)
    }

    pub async fn update_field(&self, name: &str, field: &str, value: &str) -> Result<bool> {
        // Only allow safe known column names
        let col = match field {
            "url" => "url",
            "css" => "css",
            "property" => "property",
            "schedule" => "schedule",
            "room_id" => "room_id",
            _ => return Err(anyhow::anyhow!("Unknown field: {field}")),
        };
        let conn = self.conn.lock().await;
        let n = conn.execute(
            &format!("UPDATE alerts SET {col} = ?1 WHERE name = ?2"),
            params![value, name],
        )?;
        Ok(n > 0)
    }

    pub async fn rename(&self, old_name: &str, new_name: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let n = conn.execute(
            "UPDATE alerts SET name = ?1 WHERE name = ?2",
            params![new_name, old_name],
        )?;
        Ok(n > 0)
    }

    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<bool> {
        let conn = self.conn.lock().await;
        let n = conn.execute(
            "UPDATE alerts SET enabled = ?1 WHERE name = ?2",
            params![enabled as i64, name],
        )?;
        Ok(n > 0)
    }

    pub async fn update_last_value(&self, name: &str, value: &str, checked_at: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE alerts SET last_value = ?1, last_checked = ?2 WHERE name = ?3",
            params![value, checked_at, name],
        )?;
        Ok(())
    }

    pub async fn update_last_checked(&self, name: &str, checked_at: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE alerts SET last_checked = ?1 WHERE name = ?2",
            params![checked_at, name],
        )?;
        Ok(())
    }

    pub async fn delete(&self, name: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let n = conn.execute("DELETE FROM alerts WHERE name = ?1", params![name])?;
        Ok(n > 0)
    }

    /// Disable all alerts created by the given user (called when a user is disallowed).
    pub async fn disable_by_creator(&self, creator: &str) -> Result<usize> {
        let conn = self.conn.lock().await;
        let n = conn.execute(
            "UPDATE alerts SET enabled = 0 WHERE creator = ?1",
            params![creator],
        )?;
        Ok(n)
    }

    /// Delete all disabled alerts (the cleanup command).
    pub async fn delete_disabled(&self) -> Result<usize> {
        let conn = self.conn.lock().await;
        let n = conn.execute("DELETE FROM alerts WHERE enabled = 0", [])?;
        Ok(n)
    }
}

fn row_to_alert(row: &rusqlite::Row<'_>) -> Result<Alert> {
    Ok(Alert {
        name:         row.get(0)?,
        url:          row.get(1)?,
        css:          row.get(2)?,
        property:     row.get(3)?,
        schedule:     row.get(4)?,
        room_id:      row.get(5)?,
        creator:      row.get(6)?,
        last_value:   row.get(7)?,
        last_checked: row.get(8)?,
        enabled:      row.get::<_, i64>(9)? != 0,
        created_at:   row.get(10)?,
    })
}

fn load_alerts_with(conn: &Connection, sql: &str) -> Result<Vec<Alert>> {
    let mut stmt = conn.prepare(sql)?;
    collect_alerts(stmt.query([])?)
}

fn collect_alerts(mut rows: rusqlite::Rows<'_>) -> Result<Vec<Alert>> {
    let mut alerts = Vec::new();
    while let Some(row) = rows.next()? {
        alerts.push(row_to_alert(row)?);
    }
    Ok(alerts)
}
