use include_dir::{include_dir, Dir};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

static BUNDLED: Dir = include_dir!("$CARGO_MANIFEST_DIR/../locales");

pub struct Tables {
    tables: BTreeMap<String, Map<String, Value>>,
}

fn valid_code(code: &str) -> bool {
    let mut parts = code.split('-');
    let Some(lang) = parts.next() else { return false };
    if !(2..=3).contains(&lang.len()) || !lang.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    match (parts.next(), parts.next()) {
        (None, _) => true,
        (Some(region), None) => (2..=4).contains(&region.len()) && region.chars().all(|c| c.is_ascii_alphabetic()),
        _ => false,
    }
}

impl Tables {
    pub fn load() -> Tables {
        let mut tables = BTreeMap::new();
        for file in BUNDLED.files() {
            let Some(name) = file.path().file_name().and_then(|n| n.to_str()) else { continue };
            let Some(code) = name.strip_suffix(".json") else { continue };
            if !valid_code(code) {
                continue;
            }
            let Some(text) = file.contents_utf8() else { continue };
            if let Ok(Value::Object(table)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
                tables.insert(code.to_lowercase(), table);
            }
        }
        tables.entry("en".to_string()).or_insert_with(|| {
            let mut m = Map::new();
            m.insert("$name".into(), json!("English"));
            m.insert("$dir".into(), json!("ltr"));
            m
        });
        Tables { tables }
    }

    pub fn has(&self, code: &str) -> bool {
        self.tables.contains_key(code)
    }

    pub fn pick(&self, setting: &str, preferred: &[String]) -> String {
        let setting = setting.to_lowercase();
        if self.has(&setting) {
            return setting;
        }
        for tag in preferred {
            let full = tag.to_lowercase().replace('_', "-");
            if self.has(&full) {
                return full;
            }
            let base = full.split('-').next().unwrap_or("").to_string();
            if self.has(&base) {
                return base;
            }
        }
        "en".into()
    }

    pub fn strings(&self, code: &str) -> Map<String, Value> {
        let mut out = self.tables.get("en").cloned().unwrap_or_default();
        if let Some(table) = self.tables.get(code) {
            for (k, v) in table {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    pub fn list(&self) -> Vec<Value> {
        self.tables
            .iter()
            .map(|(code, table)| {
                json!({ "code": code, "name": table.get("$name").and_then(Value::as_str).unwrap_or(code) })
            })
            .collect()
    }
}

pub fn translate(strings: &Map<String, Value>, key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = strings.get(key).and_then(Value::as_str).unwrap_or(key).to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

pub fn is_rtl(strings: &Map<String, Value>) -> bool {
    strings.get("$dir").and_then(Value::as_str) == Some("rtl")
}
