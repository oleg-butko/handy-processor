use std::{fs, io::Write, path::Path};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const SETTINGS_FILE: &str = "template-hotkey.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateHotkeySettings {
    pub enabled: bool,
    pub key: String,
    pub interval_ms: u32,
}

impl Default for TemplateHotkeySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            key: "Win".into(),
            interval_ms: 200,
        }
    }
}

pub fn key_code(name: &str) -> Option<u16> {
    Some(match name {
        "Win" | "MetaLeft" => 0x5b,
        "MetaRight" => 0x5c,
        "ControlLeft" => 0xa2,
        "ControlRight" => 0xa3,
        "ShiftLeft" => 0xa0,
        "ShiftRight" => 0xa1,
        "AltLeft" => 0xa4,
        "AltRight" => 0xa5,
        "Space" => 0x20,
        "Enter" => 0x0d,
        "Escape" => 0x1b,
        "Tab" => 0x09,
        "Backspace" => 0x08,
        "Insert" => 0x2d,
        "Delete" => 0x2e,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        _ => {
            if let Some(key) = name
                .strip_prefix("Key")
                .filter(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase())
            {
                return Some(key.as_bytes()[0] as u16);
            }
            if let Some(key) = name
                .strip_prefix("Digit")
                .filter(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_digit())
            {
                return Some(key.as_bytes()[0] as u16);
            }
            let function = name.strip_prefix('F')?.parse::<u16>().ok()?;
            if !(1..=24).contains(&function) {
                return None;
            }
            return Some(0x70 + function - 1);
        }
    })
}

pub fn key_name(code: u16) -> Option<String> {
    for name in [
        "MetaLeft",
        "MetaRight",
        "ControlLeft",
        "ControlRight",
        "ShiftLeft",
        "ShiftRight",
        "AltLeft",
        "AltRight",
        "Space",
        "Enter",
        "Escape",
        "Tab",
        "Backspace",
        "Insert",
        "Delete",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "ArrowLeft",
        "ArrowUp",
        "ArrowRight",
        "ArrowDown",
    ] {
        if key_code(name) == Some(code) {
            return Some(name.into());
        }
    }
    match code {
        0x41..=0x5a => Some(format!("Key{}", char::from(code as u8))),
        0x30..=0x39 => Some(format!("Digit{}", char::from(code as u8))),
        0x70..=0x87 => Some(format!("F{}", code - 0x70 + 1)),
        _ => None,
    }
}

impl TemplateHotkeySettings {
    pub fn validate(&self) -> Result<()> {
        if !(50..=2000).contains(&self.interval_ms) {
            bail!("Интервал должен быть от 50 до 2000 мс");
        }
        if key_code(&self.key).is_none() {
            bail!("Неподдерживаемая клавиша");
        }
        Ok(())
    }

    pub fn matches(&self, code: u16) -> bool {
        key_code(&self.key) == Some(code) || (self.key == "Win" && code == 0x5c)
    }

    pub fn load(directory: &Path) -> Result<Self> {
        let path = directory.join(SETTINGS_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error).context("Не удалось прочитать настройки хоткея"),
        };
        let settings: Self = serde_json::from_str(&text).context("Повреждены настройки хоткея")?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, directory: &Path) -> Result<()> {
        self.validate()?;
        let path = directory.join(SETTINGS_FILE);
        let temporary = directory.join(format!(".{SETTINGS_FILE}.{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> Result<()> {
            writeln!(file, "{}", serde_json::to_string_pretty(self)?)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.context("Не удалось сохранить настройки хоткея")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_settings_and_physical_keys() {
        let mut settings = TemplateHotkeySettings::default();
        assert!(settings.validate().is_ok());
        assert!(settings.matches(0x5b) && settings.matches(0x5c));
        settings.interval_ms = 2001;
        assert!(settings.validate().is_err());
        settings.interval_ms = 500;
        settings.key = "ControlRight".into();
        assert!(settings.validate().is_ok());
        assert!(!settings.matches(0xa2));
        assert_eq!(key_name(0xa3).as_deref(), Some("ControlRight"));
        assert!(key_code("KeyAA").is_none());
        assert!(key_code("F25").is_none());
    }

    #[test]
    fn saves_and_replaces_settings_without_truncating_old_file() {
        let directory =
            std::env::temp_dir().join(format!("handy-hotkey-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let mut settings = TemplateHotkeySettings::default();
        settings.save(&directory).unwrap();
        settings.interval_ms = 400;
        settings.save(&directory).unwrap();
        assert_eq!(TemplateHotkeySettings::load(&directory).unwrap(), settings);
        settings.interval_ms = 0;
        assert!(settings.save(&directory).is_err());
        assert_eq!(
            TemplateHotkeySettings::load(&directory)
                .unwrap()
                .interval_ms,
            400
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
