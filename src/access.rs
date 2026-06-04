use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    allowed: Vec<String>,
}

/// Tracks which Matrix users are permitted to use restricted commands.
///
/// - **Admins** are set via the `ADMIN_USERS` env var and are always allowed;
///   they cannot be removed.
/// - **Additional users** can be added/removed at runtime via `allow`/`disallow`
///   and are persisted to `session/allowed_users.json`.
#[derive(Debug)]
pub struct AccessControl {
    admins: Vec<String>,
    allowed: Vec<String>,
    file_path: PathBuf,
}

impl AccessControl {
    pub fn load(file_path: PathBuf, admins: Vec<String>) -> Self {
        let allowed = if file_path.exists() {
            std::fs::read_to_string(&file_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Store>(&s).ok())
                .map(|s| s.allowed)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            admins,
            allowed,
            file_path,
        }
    }

    fn save(&self) -> Result<()> {
        let store = Store {
            allowed: self.allowed.clone(),
        };
        std::fs::write(&self.file_path, serde_json::to_string_pretty(&store)?)?;
        Ok(())
    }

    pub fn is_allowed(&self, user_id: &str) -> bool {
        let id = user_id.to_lowercase();
        self.admins.iter().any(|a| a.to_lowercase() == id)
            || self.allowed.iter().any(|u| u.to_lowercase() == id)
    }

    fn is_admin(&self, user_id: &str) -> bool {
        let id = user_id.to_lowercase();
        self.admins.iter().any(|a| a.to_lowercase() == id)
    }

    pub fn add_user(&mut self, user_id: &str) -> String {
        if self.is_allowed(user_id) {
            return format!("{user_id} ist bereits erlaubt.");
        }
        self.allowed.push(user_id.to_lowercase());
        match self.save() {
            Ok(()) => format!("{user_id} wurde zur Erlaubtenliste hinzugefügt."),
            Err(e) => format!("Fehler beim Speichern: {e}"),
        }
    }

    pub fn remove_user(&mut self, user_id: &str) -> String {
        if self.is_admin(user_id) {
            return format!("{user_id} ist ein Admin und kann nicht entfernt werden.");
        }
        let id = user_id.to_lowercase();
        let before = self.allowed.len();
        self.allowed.retain(|u| u.to_lowercase() != id);
        if self.allowed.len() == before {
            return format!("{user_id} ist nicht in der Erlaubtenliste.");
        }
        match self.save() {
            Ok(()) => format!("{user_id} wurde aus der Erlaubtenliste entfernt."),
            Err(e) => format!("Fehler beim Speichern: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_ac(admins: &[&str], allowed: &[&str]) -> AccessControl {
        AccessControl {
            admins: admins.iter().map(|s| s.to_string()).collect(),
            allowed: allowed.iter().map(|s| s.to_string()).collect(),
            file_path: PathBuf::from("/dev/null"),
        }
    }

    #[test]
    fn admin_is_allowed() {
        let ac = make_ac(&["@admin:server"], &[]);
        assert!(ac.is_allowed("@admin:server"));
    }

    #[test]
    fn unknown_user_not_allowed() {
        let ac = make_ac(&["@admin:server"], &[]);
        assert!(!ac.is_allowed("@stranger:server"));
    }

    #[test]
    fn added_user_is_allowed() {
        let mut ac = make_ac(&["@admin:server"], &[]);
        ac.allowed.push("@user:server".to_string()); // bypass save() in test
        assert!(ac.is_allowed("@user:server"));
    }

    #[test]
    fn admin_cannot_be_removed() {
        let mut ac = make_ac(&["@admin:server"], &[]);
        let msg = ac.remove_user("@admin:server");
        assert!(msg.contains("Admin"));
        assert!(ac.is_allowed("@admin:server"));
    }

    #[test]
    fn remove_unknown_user_gives_error() {
        let mut ac = make_ac(&["@admin:server"], &[]);
        let msg = ac.remove_user("@nobody:server");
        assert!(msg.contains("nicht in der Erlaubtenliste"));
    }

    #[test]
    fn case_insensitive_check() {
        let ac = make_ac(&["@Admin:Server"], &[]);
        assert!(ac.is_allowed("@admin:server"));
    }
}
