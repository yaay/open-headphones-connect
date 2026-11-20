use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    #[default]
    Black,
    Silver,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Show the card when the headphones connect.
    pub show_connect_card: bool,
    pub low_battery_alert: bool,
    /// Colour of the headphones in the illustration.
    pub finish: Finish,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { show_connect_card: true, low_battery_alert: true, finish: Finish::Black }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = fs::write(path, text) {
                    crate::log!("Could not save settings: {e}");
                }
            }
            Err(e) => crate::log!("Could not serialize settings: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"finish":"silver"}"#).unwrap();
        assert_eq!(parsed.finish, Finish::Silver);
        assert!(parsed.show_connect_card && parsed.low_battery_alert);
    }
}
