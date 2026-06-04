use chrono::{Datelike, Local, Weekday};

pub fn parse_mensa_arg(arg: Option<&str>) -> Result<Vec<usize>, String> {
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
            Ok(vec![day])
        }
        Some("0") => Ok(vec![1, 2, 3, 4, 5, 6]),
        Some("1") => Ok(vec![1]),
        Some("2") => Ok(vec![2]),
        Some("3") => Ok(vec![3]),
        Some("4") => Ok(vec![4]),
        Some("5") => Ok(vec![5]),
        Some("6") => Ok(vec![6]),
        Some(s) => Err(format!(
            "Ungültiges Argument: \"{s}\". Verwende 0 (alle Tage), 1–6 (Mo–Sa) oder kein Argument (heute)."
        )),
    }
}

pub fn handle_help(body: &str) -> String {
    let arg = body
        .splitn(2, ' ')
        .nth(1)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match arg {
        None => {
            "Verfügbare Befehle:\n\n\
             mensa <tag>    – Zeigt den Speiseplan der Mensa Furtwangen\n\
             help <befehl>  – Zeigt Hilfe zu einem Befehl\n\n\
             Tipp: `help <befehl>` für mehr Details"
                .to_string()
        }
        Some("mensa") => {
            "mensa <tag>\n\n\
             Zeigt den Speiseplan der Mensa Furtwangen (HFU).\n\n\
             Parameter:\n\
             (kein)  – Heutiger Tag\n\
             0       – Ganze Woche (Mo–Sa)\n\
             1       – Montag\n\
             2       – Dienstag\n\
             3       – Mittwoch\n\
             4       – Donnerstag\n\
             5       – Freitag\n\
             6       – Samstag\n\n\
             Beispiele:\n\
             mensa    → Heute\n\
             mensa 0  → Ganze Woche\n\
             mensa 3  → Mittwoch"
                .to_string()
        }
        Some(s) => {
            format!("Unbekannter Befehl: \"{s}\". Verfügbare Befehle: mensa")
        }
    }
}

pub async fn handle_mensa(body: &str) -> String {
    let arg = body
        .splitn(2, ' ')
        .nth(1)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let days = match parse_mensa_arg(arg) {
        Ok(days) => days,
        Err(e) => return e,
    };

    match crate::mensa::load_meals().await {
        Ok(meals) => crate::mensa::format_meals(&meals, &days),
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
        let result = parse_mensa_arg(None).unwrap();
        assert_eq!(result.len(), 1);
        let expected = match Local::now().weekday() {
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
            Weekday::Sun => 1,
        };
        assert_eq!(result[0], expected);
    }

    #[test]
    fn parse_day_arg_all() {
        let result = parse_mensa_arg(Some("0")).unwrap();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn parse_day_arg_monday() {
        assert_eq!(parse_mensa_arg(Some("1")).unwrap(), vec![1]);
    }

    #[test]
    fn parse_day_arg_saturday() {
        assert_eq!(parse_mensa_arg(Some("6")).unwrap(), vec![6]);
    }

    #[test]
    fn parse_day_arg_invalid() {
        assert!(parse_mensa_arg(Some("7")).is_err());
        assert!(parse_mensa_arg(Some("foo")).is_err());
    }

    #[test]
    fn help_no_arg_lists_commands() {
        let result = handle_help("help");
        assert!(result.contains("mensa"));
        assert!(result.contains("help"));
    }

    #[test]
    fn help_mensa_shows_days() {
        let result = handle_help("help mensa");
        assert!(result.contains("Montag"));
        assert!(result.contains("Samstag"));
        assert!(result.contains("mensa 0"));
    }

    #[test]
    fn help_unknown_command() {
        let result = handle_help("help foobar");
        assert!(result.contains("foobar"));
    }
}
