use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub pet: String,
    pub size: f64,
    pub anchor: Option<Anchor>,
    pub visible: bool,
    pub lang: String,
    pub shortcut: String,
    pub quick_mode: String,
    pub quick_cwd: String,
    pub welcomed: bool,
    pub show_background: bool,
    pub store_text: bool,
    pub sound: bool,
    pub notify: bool,
    pub approvals: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            pet: "ember".into(),
            size: 1.0,
            anchor: None,
            visible: true,
            lang: "auto".into(),
            shortcut: "Control+Alt+P".into(),
            quick_mode: "background".into(),
            quick_cwd: String::new(),
            welcomed: false,
            show_background: false,
            store_text: true,
            sound: false,
            notify: false,
            approvals: true,
            extra: Map::new(),
        }
    }
}

impl Config {
    pub fn load(file: &Path) -> Config {
        let Ok(text) = fs::read_to_string(file) else {
            return Config::default();
        };
        let Ok(Value::Object(saved)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
            return Config::default();
        };
        let Ok(Value::Object(mut merged)) = serde_json::to_value(Config::default()) else {
            return Config::default();
        };
        for (key, value) in saved {
            if !value.is_null() || key == "anchor" {
                merged.insert(key, value);
            }
        }
        match serde_json::from_value::<Config>(Value::Object(merged.clone())) {
            Ok(config) => config,
            Err(_) => {
                let mut config = Config::default();
                for (key, value) in merged {
                    let mut probe = serde_json::to_value(&config).unwrap_or(Value::Null);
                    if let Some(map) = probe.as_object_mut() {
                        map.insert(key, value);
                    }
                    if let Ok(next) = serde_json::from_value::<Config>(probe) {
                        config = next;
                    }
                }
                config
            }
        }
    }

    pub fn save(&self, file: &Path) {
        let Some(dir) = file.parent() else { return };
        let _ = fs::create_dir_all(dir);
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let mut tmp = file.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = std::path::PathBuf::from(tmp);
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, file);
        }
    }

    pub fn extra_bool(&self, key: &str) -> Option<bool> {
        self.extra.get(key).and_then(Value::as_bool)
    }
}
