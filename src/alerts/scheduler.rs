use super::{Alert, AlertDb};
use crate::alerts::fetcher::fetch_value;
use chrono::{TimeZone, Utc};
use cron::Schedule;
use matrix_sdk::{Client, ruma::OwnedRoomId};
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use std::{str::FromStr, sync::Arc, time::Duration};

/// Convert a standard 5-field cron expression to the 6-field format
/// expected by the `cron` crate (which requires a leading seconds field).
pub fn to_cron6(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {expr}")
    } else {
        expr.to_string()
    }
}

/// Validate a cron expression (accepts 5- or 6-field).
pub fn validate_schedule(expr: &str) -> Result<(), String> {
    let expr6 = to_cron6(expr);
    Schedule::from_str(&expr6).map_err(|e| format!("Ungültiger Cron-Ausdruck: {e}"))?;
    Ok(())
}

/// Returns true if the alert is due to run based on its last_checked time and schedule.
pub fn is_due(alert: &Alert) -> bool {
    let expr6 = to_cron6(&alert.schedule);
    let sched = match Schedule::from_str(&expr6) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let now = Utc::now();
    let after = match alert.last_checked {
        Some(ts) => Utc.timestamp_opt(ts, 0).single().unwrap_or(now - chrono::Duration::days(1)),
        None => now - chrono::Duration::days(1),
    };

    sched.after(&after).next().map_or(false, |next| next <= now)
}

/// Background task: checks all enabled alerts every 30 seconds.
pub async fn run_scheduler(db: Arc<AlertDb>, client: Client, template: Arc<String>) {
    tracing::info!("Alert scheduler started");
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let alerts = match db.load_enabled().await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Scheduler: failed to load alerts: {e}");
                continue;
            }
        };

        let due: Vec<_> = alerts.into_iter().filter(|a| is_due(a)).collect();
        if !due.is_empty() {
            tracing::debug!("Scheduler: {} alert(s) due", due.len());
        }

        for alert in due {
            let db = Arc::clone(&db);
            let client = client.clone();
            let template = Arc::clone(&template);
            tokio::spawn(async move {
                run_check(&alert, &db, &client, &template).await;
            });
        }
    }
}

pub async fn run_check(alert: &Alert, db: &AlertDb, client: &Client, template: &str) {
    let now = Utc::now().timestamp();
    tracing::debug!("Checking alert \"{}\" ({})", alert.name, alert.url);

    // Stamp last_checked immediately so the 30s scheduler loop cannot re-fire this
    // alert while a slow fetch (5 attempts × retry delay = up to ~50s) is in flight.
    db.update_last_checked(&alert.name, now).await.ok();

    match fetch_value(&alert.url, &alert.css, &alert.property).await {
        Err(e) => {
            tracing::warn!("Alert \"{}\": fetch failed — {e}", alert.name);
            let msg = format!("⚠️ Alert \"{}\" fehlgeschlagen\n\n{}", alert.name, e);
            send_to_room(client, &alert.room_id, &msg).await;
        }
        Ok(new_value) => {

            let old_value = match &alert.last_value {
                Some(v) => v.clone(),
                None => {
                    // First check after enable — just save the value, no notification
                    tracing::debug!("Alert \"{}\": initial value saved: {:?}", alert.name, new_value);
                    db.update_last_value(&alert.name, &new_value, now).await.ok();
                    return;
                }
            };

            if new_value != old_value {
                tracing::info!(
                    "Alert \"{}\": change detected — {:?} → {:?}",
                    alert.name, old_value, new_value
                );
                let msg = render_template(template, &alert.name, &alert.url, &old_value, &new_value);
                send_to_room(client, &alert.room_id, &msg).await;
                db.update_last_value(&alert.name, &new_value, now).await.ok();
            } else {
                tracing::debug!("Alert \"{}\": no change", alert.name);
            }
        }
    }
}

fn render_template(template: &str, name: &str, url: &str, old: &str, new: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{url}", url)
        .replace("{old}", old)
        .replace("{new}", new)
}

async fn send_to_room(client: &Client, room_id: &str, msg: &str) {
    let Ok(room_id) = room_id.parse::<OwnedRoomId>() else {
        tracing::error!("Invalid room ID: {room_id}");
        return;
    };
    if let Some(room) = client.get_room(&room_id) {
        if let Err(e) = room.send(RoomMessageEventContent::text_plain(msg)).await {
            tracing::error!("Failed to send alert notification: {e}");
        }
    } else {
        tracing::warn!("Alert target room {room_id} not found");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron6_converts_5field() {
        assert_eq!(to_cron6("0 8 * * 1-5"), "0 0 8 * * 1-5");
    }

    #[test]
    fn cron6_leaves_6field_alone() {
        assert_eq!(to_cron6("0 0 8 * * 1-5"), "0 0 8 * * 1-5");
    }

    #[test]
    fn validate_valid_schedule() {
        assert!(validate_schedule("0 8 * * *").is_ok());
        assert!(validate_schedule("*/5 * * * *").is_ok());
    }

    #[test]
    fn validate_invalid_schedule() {
        assert!(validate_schedule("not a cron").is_err());
    }

    #[test]
    fn render_template_substitutes_all() {
        let tmpl = "Alert {name} changed\n\nOld: {old}\nNew: {new}";
        let result = render_template(tmpl, "price", "https://ex.com", "10€", "12€");
        assert!(result.contains("price"));
        assert!(result.contains("10€"));
        assert!(result.contains("12€"));
    }
}
