use chrono::{Datelike, Local, Weekday};
use std::sync::Arc;

use crate::access::AccessControl;
use crate::alerts::AlertDb;

/// Parse the `mensa` argument into a `(week, days)` pair.
///
/// The argument is a number `N` where the tens digit is the week offset
/// (0 = current, 1 = next, 2 = in two weeks) and the ones digit is the day
/// (0 = whole week Mo–Sa, 1–6 = Mo–Sa). No argument shows today.
///
/// Examples: `0` → this week, all · `3` → this week, Wed · `10` → next week, all
/// · `12` → next week, Tue · `20` → in two weeks, all.
pub fn parse_mensa_arg(arg: Option<&str>) -> Result<(usize, Vec<usize>), String> {
    let invalid = |s: &str| {
        format!(
            "Ungültiges Argument: \"{s}\". Format: <Woche><Tag>. \
             Woche 0 (diese), 1 (nächste), 2 (übernächste); \
             Tag 0 (alle) oder 1–6 (Mo–Sa). \
             Beispiele: 0 (diese Woche), 3 (Mi), 10 (nächste Woche), 12 (nächste Woche Di)."
        )
    };

    match arg {
        None | Some("") => {
            let day = match Local::now().weekday() {
                Weekday::Mon => 1,
                Weekday::Tue => 2,
                Weekday::Wed => 3,
                Weekday::Thu => 4,
                Weekday::Fri => 5,
                Weekday::Sat => 6,
                Weekday::Sun => 1,
            };
            Ok((0, vec![day]))
        }
        Some(s) => {
            let n: usize = s.parse().map_err(|_| invalid(s))?;
            let week = n / 10;
            let day = n % 10;
            if week > 2 {
                return Err(invalid(s));
            }
            let days = match day {
                0 => vec![1, 2, 3, 4, 5, 6],
                1..=6 => vec![day],
                _ => return Err(invalid(s)),
            };
            Ok((week, days))
        }
    }
}

/// `show_restricted` should be true when the caller is an allowed user,
/// which unlocks the `allow`/`disallow`/`alerts` entries in the command list.
pub fn handle_help(body: &str, show_restricted: bool) -> String {
    // Lowercase the argument so `help Mensa`, `help ALERTS CREATE` etc. all work
    let arg_owned = body
        .splitn(2, ' ')
        .nth(1)
        .map(|s| s.trim().to_lowercase());
    let arg = arg_owned.as_deref().filter(|s| !s.is_empty());

    match arg {
        None => {
            let public_cmds = "mensa <tag>         – Zeigt den Speiseplan der Mensa Furtwangen\n\
                               help <befehl>       – Zeigt Hilfe zu einem Befehl";
            let restricted_cmds =
                "alerts <befehl>     – Website-Änderungen überwachen\n\
                 allow <@nutzer>     – Nutzer zur Erlaubtenliste hinzufügen\n\
                 disallow <@nutzer>  – Nutzer aus der Erlaubtenliste entfernen";

            if show_restricted {
                format!(
                    "Verfügbare Befehle:\n\n\
                     {public_cmds}\n\
                     {restricted_cmds}\n\n\
                     Tipp: `help <befehl>` für mehr Details"
                )
            } else {
                format!(
                    "Verfügbare Befehle:\n\n\
                     {public_cmds}\n\n\
                     Tipp: `help <befehl>` für mehr Details"
                )
            }
        }
        Some("mensa") => {
            "mensa <auswahl>\n\n\
             Zeigt den Speiseplan der Mensa Furtwangen (HFU).\n\n\
             Die Auswahl ist eine Zahl <Woche><Tag>:\n\
             Zehnerstelle = Woche  – 0 diese, 1 nächste, 2 übernächste\n\
             Einerstelle  = Tag    – 0 ganze Woche, 1–6 Mo–Sa\n\n\
             Parameter:\n\
             (kein)  – Heutiger Tag (diese Woche)\n\
             0       – Ganze Woche (Mo–Sa)\n\
             1–6     – Mo–Sa (diese Woche)\n\
             10      – Ganze nächste Woche\n\
             11–16   – Mo–Sa (nächste Woche)\n\
             20      – Ganze übernächste Woche\n\
             21–26   – Mo–Sa (übernächste Woche)\n\n\
             Beispiele:\n\
             mensa     → Heute\n\
             mensa 0   → Ganze Woche\n\
             mensa 3   → Mittwoch (diese Woche)\n\
             mensa 10  → Ganze nächste Woche\n\
             mensa 12  → Dienstag (nächste Woche)\n\
             mensa 20  → Ganze übernächste Woche"
                .to_string()
        }
        Some("allow") => {
            "allow <@nutzer:server>\n\n\
             Fügt einen Matrix-Nutzer zur Erlaubtenliste hinzu.\n\
             Nur verfügbar für bereits erlaubte Nutzer.\n\n\
             Beispiel:\n\
             allow @freund:matrix.org"
                .to_string()
        }
        Some("disallow") => {
            "disallow <@nutzer:server>\n\n\
             Entfernt einen Matrix-Nutzer aus der Erlaubtenliste.\n\
             Admins können nicht entfernt werden.\n\
             Alle Alerts des Nutzers werden dabei deaktiviert.\n\
             Nur verfügbar für bereits erlaubte Nutzer.\n\n\
             Beispiel:\n\
             disallow @freund:matrix.org"
                .to_string()
        }
        Some("alerts") => {
            "alerts <befehl>\n\n\
             Überwacht Websites auf Änderungen und sendet Benachrichtigungen.\n\
             Nur verfügbar für erlaubte Nutzer.\n\n\
             Unterbefehle:\n\
             alerts list                              – Deine Alerts\n\
             alerts list-all                          – Alle Alerts\n\
             alerts info <name>                       – Details zu einem Alert\n\
             alerts create …                          – Neuen Alert erstellen (→ help alerts create)\n\
             alerts update <name> <feld> [wert]       – Felder: url, css, property, schedule, room, name\n\
             alerts update <name> room                – Raum auf aktuellen Raum setzen\n\
             alerts enable <name>                     – Alert aktivieren\n\
             alerts disable <name>                    – Alert deaktivieren\n\
             alerts remove <name>                     – Alert löschen\n\
             alerts cleanup                           – Alle deaktivierten Alerts löschen\n\n\
             Tipp: `help alerts create` für eine ausführliche Erklärung der Parameter."
                .to_string()
        }
        Some("alerts create") => {
            "alerts create <name> <url> <css> <property> <schedule>\n\n\
             Erstellt einen neuen Alert, der eine Website periodisch auf Änderungen prüft.\n\
             Beim Erstellen wird sofort ein erster Abruf durchgeführt und der aktuelle\n\
             Wert als Baseline gespeichert.\n\n\
             Parameter:\n\n\
             <name>\n\
               Eindeutiger Name für den Alert. Keine Leerzeichen.\n\
               Beispiel: preis-tracker\n\n\
             <url>\n\
               Die vollständige URL der zu überwachenden Seite.\n\
               Beispiel: https://shop.example.de/produkt\n\n\
             <css>\n\
               CSS-Selektor des HTML-Elements, dessen Wert überwacht werden soll.\n\
               Bei Leerzeichen im Selektor in Anführungszeichen einschließen.\n\
               Beispiele: .price     \"#main .product-price\"     h1\n\n\
             <property>\n\
               Welcher Wert des Elements ausgelesen wird:\n\
               innerText  – sichtbarer Text (häufigste Wahl)\n\
               innerHTML  – kompletter HTML-Inhalt\n\
               href       – Linkziel (bei <a>-Elementen)\n\
               src        – Bildquelle (bei <img>-Elementen)\n\
               <attr>     – beliebiges HTML-Attribut, z.B. data-price\n\n\
             <schedule>\n\
               Prüfintervall als Cron-Ausdruck (5 Felder: min std tag monat wochentag).\n\
               Bei Leerzeichen im Ausdruck in Anführungszeichen einschließen.\n\
               Felder können * (jeder Wert), */n (alle n), n-m (Bereich) enthalten.\n\
               Beispiele:\n\
               \"*/15 * * * *\"    – alle 15 Minuten\n\
               \"0 8 * * 1-5\"     – täglich um 08:00 Uhr (Mo–Fr)\n\
               \"0 */2 * * *\"     – alle 2 Stunden\n\
               \"0 9 * * 1\"       – montags um 09:00 Uhr\n\n\
             Vollständiges Beispiel:\n\
             alerts create preis https://shop.de \".product-price\" innerText \"0 8 * * 1-5\""
                .to_string()
        }
        Some(s) => {
            let known = if show_restricted {
                "mensa, alerts, allow, disallow"
            } else {
                "mensa"
            };
            format!("Unbekannter Befehl: \"{s}\". Verfügbare Befehle: {known}")
        }
    }
}

pub fn handle_allow(body: &str, access: &mut AccessControl) -> String {
    match body.splitn(2, ' ').nth(1).map(str::trim).filter(|s| !s.is_empty()) {
        None => "Verwendung: allow @nutzer:server".to_string(),
        Some(user_id) => access.add_user(user_id),
    }
}

pub async fn handle_disallow(
    body: &str,
    access: &mut AccessControl,
    alert_db: &Arc<AlertDb>,
) -> String {
    let user_id = match body.splitn(2, ' ').nth(1).map(str::trim).filter(|s| !s.is_empty()) {
        None => return "Verwendung: disallow @nutzer:server".to_string(),
        Some(id) => id,
    };
    match access.remove_user(user_id) {
        Err(msg) => msg,
        Ok(removed_id) => {
            // Disable all alerts created by the removed user
            let disabled = alert_db.disable_by_creator(&removed_id).await.unwrap_or(0);
            let alert_note = if disabled > 0 {
                format!(" ({disabled} Alert(s) deaktiviert)")
            } else {
                String::new()
            };
            format!("{user_id} wurde aus der Erlaubtenliste entfernt.{alert_note}")
        }
    }
}

pub async fn handle_mensa(body: &str) -> String {
    let arg = body
        .splitn(2, ' ')
        .nth(1)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let (week, days) = match parse_mensa_arg(arg) {
        Ok(v) => v,
        Err(e) => return e,
    };

    match crate::mensa::load_meals_for_week(week).await {
        Ok(meals) => crate::mensa::format_meals(&meals, &days, week),
        Err(e) => {
            tracing::error!("Failed to load meals: {e}");
            "Fehler beim Laden des Mensaplans. Bitte später versuchen.".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_day_arg_today() {
        let (week, days) = parse_mensa_arg(None).unwrap();
        assert_eq!(week, 0);
        assert_eq!(days.len(), 1);
        let expected = match Local::now().weekday() {
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
            Weekday::Sun => 1,
        };
        assert_eq!(days[0], expected);
    }

    #[test]
    fn parse_day_arg_all() {
        assert_eq!(parse_mensa_arg(Some("0")).unwrap(), (0, vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn parse_day_arg_monday() {
        assert_eq!(parse_mensa_arg(Some("1")).unwrap(), (0, vec![1]));
    }

    #[test]
    fn parse_day_arg_saturday() {
        assert_eq!(parse_mensa_arg(Some("6")).unwrap(), (0, vec![6]));
    }

    #[test]
    fn parse_next_week_all() {
        assert_eq!(parse_mensa_arg(Some("10")).unwrap(), (1, vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn parse_next_week_tuesday() {
        assert_eq!(parse_mensa_arg(Some("12")).unwrap(), (1, vec![2]));
    }

    #[test]
    fn parse_two_weeks_all() {
        assert_eq!(parse_mensa_arg(Some("20")).unwrap(), (2, vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn parse_two_weeks_saturday() {
        assert_eq!(parse_mensa_arg(Some("26")).unwrap(), (2, vec![6]));
    }

    #[test]
    fn parse_day_arg_invalid() {
        assert!(parse_mensa_arg(Some("7")).is_err());   // day out of range
        assert!(parse_mensa_arg(Some("17")).is_err());  // day out of range, week 1
        assert!(parse_mensa_arg(Some("30")).is_err());  // week out of range
        assert!(parse_mensa_arg(Some("foo")).is_err()); // not a number
    }

    #[test]
    fn help_no_arg_public() {
        let result = handle_help("help", false);
        assert!(result.contains("mensa"));
        assert!(result.contains("help"));
        assert!(!result.contains("allow"));
    }

    #[test]
    fn help_no_arg_restricted_shows_extra_cmds() {
        let result = handle_help("help", true);
        assert!(result.contains("mensa"));
        assert!(result.contains("alerts"));
        assert!(result.contains("allow"));
        assert!(result.contains("disallow"));
    }

    #[test]
    fn help_alerts_shows_subcommands() {
        let result = handle_help("help alerts", true);
        assert!(result.contains("list"));
        assert!(result.contains("update"));
        assert!(result.contains("remove"));
        assert!(result.contains("help alerts create")); // directs to detailed help
    }

    #[test]
    fn help_alerts_create_shows_all_params() {
        let result = handle_help("help alerts create", true);
        assert!(result.contains("<name>"));
        assert!(result.contains("<url>"));
        assert!(result.contains("<css>"));
        assert!(result.contains("<property>"));
        assert!(result.contains("<schedule>"));
        assert!(result.contains("innerText"));
        assert!(result.contains("innerHTML"));
        assert!(result.contains("href"));
        assert!(result.contains("Cron"));
    }

    #[test]
    fn help_mensa_shows_days() {
        let result = handle_help("help mensa", false);
        assert!(result.contains("Mo–Sa"));
        assert!(result.contains("nächste Woche"));
        assert!(result.contains("mensa 0"));
        assert!(result.contains("mensa 10"));
    }

    #[test]
    fn help_unknown_command() {
        let result = handle_help("help foobar", false);
        assert!(result.contains("foobar"));
    }
}
