use super::{Alert, AlertDb};
use crate::alerts::{fetcher::fetch_value, scheduler::validate_schedule};
use chrono::Utc;
use matrix_sdk::Client;
use std::sync::Arc;

// ── public entry point ────────────────────────────────────────────────────────

pub async fn handle_alerts(
    body: &str,
    sender: &str,
    room_id: &str,
    db: &Arc<AlertDb>,
    client: &Client,
    template: &Arc<String>,
) -> String {
    // body starts with "alerts", so split off the subcommand
    let mut parts = body.splitn(3, ' ');
    let _cmd = parts.next(); // "alerts"
    // Lowercase only the subcommand name; `rest` preserves original case for argument values
    let sub = parts.next().unwrap_or("list").trim().to_lowercase();
    let sub = sub.as_str();
    let rest = parts.next().unwrap_or("").trim();

    match sub {
        "" | "list" => list(sender, room_id, db, false).await,
        "list-all" => list(sender, room_id, db, true).await,
        "info" => info(rest, db).await,
        "create" => create(rest, sender, room_id, db, client, template).await,
        "update" => update(rest, room_id, db).await,
        "remove" => remove(rest, db).await,
        "enable" => set_enabled(rest, true, db).await,
        "disable" => set_enabled(rest, false, db).await,
        "cleanup" => cleanup(db).await,
        other => format!(
            "Unbekannter alerts-Befehl: \"{other}\".\n\
             Verfügbare Befehle: list, list-all, info, create, update, remove, enable, disable, cleanup"
        ),
    }
}

// ── list ──────────────────────────────────────────────────────────────────────

async fn list(sender: &str, room_id: &str, db: &AlertDb, all: bool) -> String {
    let alerts = match if all { db.load_all().await } else { db.load_by_creator(sender).await } {
        Ok(a) => a,
        Err(e) => return format!("Fehler beim Laden der Alerts: {e}"),
    };

    if alerts.is_empty() {
        return if all {
            "Keine Alerts vorhanden.".to_string()
        } else {
            "Du hast keine Alerts.".to_string()
        };
    }

    let header = if all {
        format!("Alle Alerts ({}):\n", alerts.len())
    } else {
        format!("Deine Alerts ({}):\n", alerts.len())
    };

    let rows: Vec<String> = alerts
        .iter()
        .map(|a| {
            let here = if a.room_id == room_id { " [hier]" } else { "" };
            let status = if a.enabled { "✅" } else { "⏸" };
            if all {
                let you = if a.creator == sender { " [du]" } else { "" };
                format!(
                    "  {status} {:<20} {:<30} {}{}{}",
                    a.name, a.room_id, a.creator, you, here
                )
            } else {
                format!("  {status} {:<20} {}{}", a.name, a.room_id, here)
            }
        })
        .collect();

    let legend = "\n[hier] = dieser Raum  [du] = du";
    format!("{header}\n{}\n{}", rows.join("\n"), legend)
}

// ── info ──────────────────────────────────────────────────────────────────────

async fn info(name: &str, db: &AlertDb) -> String {
    if name.is_empty() {
        return "Verwendung: alerts info <name>".to_string();
    }
    match db.get(name).await {
        Err(e) => format!("Fehler: {e}"),
        Ok(None) => format!("Kein Alert mit dem Namen \"{name}\" gefunden."),
        Ok(Some(a)) => {
            let last_checked = a.last_checked
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "nie".to_string());

            let last_value = a.last_value.as_deref().unwrap_or("–");
            let status = if a.enabled { "aktiv" } else { "deaktiviert" };

            format!(
                "Alert: {}\n\
                 Status:       {}\n\
                 URL:          {}\n\
                 CSS:          {}\n\
                 Property:     {}\n\
                 Schedule:     {}\n\
                 Raum:         {}\n\
                 Erstellt von: {}\n\
                 Letzter Wert: {}\n\
                 Letzte Prüfung: {}",
                a.name, status, a.url, a.css, a.property,
                a.schedule, a.room_id, a.creator, last_value, last_checked
            )
        }
    }
}

// ── create ────────────────────────────────────────────────────────────────────

/// Parses: alerts create <name> <url> <css> <property> <schedule...>
/// css may be quoted if it contains spaces.
async fn create(
    args: &str,
    sender: &str,
    room_id: &str,
    db: &AlertDb,
    _client: &Client,
    template: &Arc<String>,
) -> String {
    let tokens = shell_split(args);
    if tokens.len() < 5 {
        return "Verwendung: alerts create <name> <url> <css> <property> <schedule>\n\
                Beispiel: alerts create preis https://shop.de \".price\" innerText \"0 8 * * 1-5\""
            .to_string();
    }

    let name = &tokens[0];
    let url = &tokens[1];
    let css = &tokens[2];
    let property = &tokens[3];
    // schedule is everything from token[4] onward (supports unquoted 5-field cron)
    let schedule = tokens[4..].join(" ");

    // Validate
    if let Err(e) = validate_schedule(&schedule) {
        return e;
    }
    if url.parse::<reqwest::Url>().is_err() {
        return format!("Ungültige URL: \"{url}\"");
    }

    // Check name uniqueness
    match db.get(name).await {
        Ok(Some(_)) => return format!("Ein Alert mit dem Namen \"{name}\" existiert bereits."),
        Err(e) => return format!("DB-Fehler: {e}"),
        Ok(None) => {}
    }

    // Initial fetch
    match fetch_value(url, css, property).await {
        Err(e) => format!("❌ Alert konnte nicht erstellt werden – Abruf fehlgeschlagen:\n{e}"),
        Ok(value) => {
            let now = Utc::now().timestamp();
            let alert = Alert {
                name: name.clone(),
                url: url.clone(),
                css: css.clone(),
                property: property.clone(),
                schedule: schedule.clone(),
                room_id: room_id.to_string(),
                creator: sender.to_string(),
                last_value: Some(value.clone()),
                last_checked: Some(now),
                enabled: true,
                created_at: now,
            };

            match db.insert(&alert).await {
                Err(e) => format!("Fehler beim Speichern: {e}"),
                Ok(()) => {
                    tracing::info!("Alert \"{name}\" created by {sender} in {room_id} (schedule: {schedule}, url: {url})");
                    // Send initial confirmation using the template
                    let confirmation = template
                        .replace("{name}", name)
                        .replace("{url}", url)
                        .replace("{old}", "–")
                        .replace("{new}", &value)
                        .replace("{css}", css)
                        .replace("{property}", property);
                    format!(
                        "✅ Alert \"{name}\" erstellt.\n\nAktueller Wert: {value}\n\n{confirmation}"
                    )
                }
            }
        }
    }
}

// ── update ────────────────────────────────────────────────────────────────────

/// Parses: alerts update <name> <field> [value...]
/// For `room` with no value, uses the current room_id.
async fn update(args: &str, current_room: &str, db: &AlertDb) -> String {
    let tokens = shell_split(args);
    if tokens.len() < 2 {
        return "Verwendung: alerts update <name> <feld> [wert]\n\
                Felder: url, css, property, schedule, room, name"
            .to_string();
    }

    let alert_name = &tokens[0];
    let field = tokens[1].as_str();

    // Verify alert exists
    match db.get(alert_name).await {
        Ok(None) => return format!("Kein Alert \"{alert_name}\" gefunden."),
        Err(e) => return format!("DB-Fehler: {e}"),
        Ok(Some(_)) => {}
    }

    match field {
        "room" => {
            let new_room = if tokens.len() >= 3 { tokens[2].as_str() } else { current_room };
            match db.update_field(alert_name, "room_id", new_room).await {
                Ok(_) => format!("✅ Raum für \"{alert_name}\" aktualisiert."),
                Err(e) => format!("Fehler: {e}"),
            }
        }
        "name" => {
            if tokens.len() < 3 {
                return "Verwendung: alerts update <name> name <neuer_name>".to_string();
            }
            let new_name = &tokens[2];
            match db.rename(alert_name, new_name).await {
                Ok(true) => format!("✅ Alert umbenannt in \"{new_name}\"."),
                Ok(false) => format!("Kein Alert \"{alert_name}\" gefunden."),
                Err(e) => format!("Fehler: {e}"),
            }
        }
        "schedule" => {
            if tokens.len() < 3 {
                return "Verwendung: alerts update <name> schedule <cron>".to_string();
            }
            let new_schedule = tokens[2..].join(" ");
            if let Err(e) = validate_schedule(&new_schedule) {
                return e;
            }
            match db.update_field(alert_name, "schedule", &new_schedule).await {
                Ok(_) => format!("✅ Schedule für \"{alert_name}\" aktualisiert."),
                Err(e) => format!("Fehler: {e}"),
            }
        }
        other => {
            if tokens.len() < 3 {
                return format!("Verwendung: alerts update <name> {other} <wert>");
            }
            let value = &tokens[2];
            let db_field = match other {
                "url" => "url",
                "css" => "css",
                "property" => "property",
                _ => return format!("Unbekanntes Feld \"{other}\". Erlaubt: url, css, property, schedule, room, name"),
            };
            match db.update_field(alert_name, db_field, value).await {
                Ok(_) => format!("✅ {other} für \"{alert_name}\" aktualisiert."),
                Err(e) => format!("Fehler: {e}"),
            }
        }
    }
}

// ── remove / enable / disable / cleanup ──────────────────────────────────────

async fn remove(name: &str, db: &AlertDb) -> String {
    if name.is_empty() {
        return "Verwendung: alerts remove <name>".to_string();
    }
    match db.delete(name).await {
        Ok(true) => {
            tracing::info!("Alert \"{name}\" removed");
            format!("✅ Alert \"{name}\" gelöscht.")
        }
        Ok(false) => format!("Kein Alert \"{name}\" gefunden."),
        Err(e) => format!("Fehler: {e}"),
    }
}

async fn set_enabled(name: &str, enabled: bool, db: &AlertDb) -> String {
    if name.is_empty() {
        let cmd = if enabled { "enable" } else { "disable" };
        return format!("Verwendung: alerts {cmd} <name>");
    }
    let verb = if enabled { "aktiviert" } else { "deaktiviert" };
    match db.set_enabled(name, enabled).await {
        Ok(true) => {
            tracing::info!("Alert \"{name}\" {verb}");
            format!("✅ Alert \"{name}\" {verb}.")
        }
        Ok(false) => format!("Kein Alert \"{name}\" gefunden."),
        Err(e) => format!("Fehler: {e}"),
    }
}

async fn cleanup(db: &AlertDb) -> String {
    match db.delete_disabled().await {
        Ok(0) => "Keine deaktivierten Alerts gefunden.".to_string(),
        Ok(n) => {
            tracing::info!("Cleanup: removed {n} disabled alert(s)");
            format!("✅ {n} deaktivierte(r) Alert(s) gelöscht.")
        }
        Err(e) => format!("Fehler: {e}"),
    }
}

// ── shell-like tokenizer ──────────────────────────────────────────────────────

/// Splits `s` on whitespace, respecting single- and double-quoted strings.
/// `"hello world"` and `'hello world'` each count as one token.
pub fn shell_split(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in s.chars() {
        match in_quote {
            Some(q) if ch == q => in_quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => in_quote = Some(ch),
            None if ch == ' ' || ch == '\t' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_split_basic() {
        assert_eq!(shell_split("a b c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn shell_split_quoted() {
        assert_eq!(
            shell_split(r#"price https://x.de ".product .price" innerText "0 8 * * *""#),
            vec!["price", "https://x.de", ".product .price", "innerText", "0 8 * * *"]
        );
    }

    #[test]
    fn shell_split_single_quotes() {
        assert_eq!(
            shell_split("a 'hello world' b"),
            vec!["a", "hello world", "b"]
        );
    }

    #[test]
    fn shell_split_unquoted_cron() {
        // schedule without quotes — tokens[4..] joined with space
        let tokens = shell_split("name https://x.de .price innerText 0 8 * * 1-5");
        assert_eq!(&tokens[4..], &["0", "8", "*", "*", "1-5"]);
        assert_eq!(tokens[4..].join(" "), "0 8 * * 1-5");
    }
}
