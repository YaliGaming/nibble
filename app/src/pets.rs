use include_dir::{include_dir, Dir};
use nibble_core::paths::Paths;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;

static BUILTIN: Dir = include_dir!("$CARGO_MANIFEST_DIR/../pets");
static HATCH_SKILL: &str = include_str!("../../skill/hatch-pet/SKILL.md");

const MAX_SIDE: usize = 32;
const FRAME_NAMES: [&str; 7] = ["blink", "walk1", "walk2", "happy", "sad", "alert", "sleep"];
pub const TUCK: i32 = 3;
const MARGIN_X: usize = 4;
const MARGIN_TOP: usize = 7;
const MARGIN_BOTTOM: usize = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pet {
    #[serde(default)]
    pub format: Option<Value>,
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub palette: BTreeMap<String, String>,
    pub base: Vec<String>,
    #[serde(default)]
    pub frames: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Value>,
    #[serde(default)]
    pub custom: bool,
}

fn is_clear(c: char) -> bool {
    c == '.' || c == ' '
}

fn is_hex(s: &str) -> bool {
    let Some(h) = s.strip_prefix('#') else { return false };
    (h.len() == 3 || h.len() == 6) && h.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn validate(pet: &Value) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    let Some(obj) = pet.as_object() else {
        return Err(vec!["pet must be a JSON object".into()]);
    };
    if !obj.get("name").and_then(Value::as_str).is_some_and(|n| !n.trim().is_empty()) {
        errors.push("name is required".into());
    }
    let palette = obj.get("palette").and_then(Value::as_object);
    match palette {
        None => errors.push("palette is required".into()),
        Some(palette) => {
            for (ch, colour) in palette {
                if ch.chars().count() != 1 || is_clear(ch.chars().next().unwrap_or(' ')) {
                    errors.push(format!("palette key \"{ch}\" must be a single character other than \".\" or space"));
                }
                if !colour.as_str().is_some_and(is_hex) {
                    errors.push(format!("palette colour for \"{ch}\" must be #rgb or #rrggbb"));
                }
            }
        }
    }
    if let Some(outline) = obj.get("outline") {
        let ok = outline == &Value::Bool(false) || outline.as_str().is_some_and(is_hex);
        if !ok {
            errors.push("outline must be false or a hex colour".into());
        }
    }
    let base: Option<Vec<&str>> = obj
        .get("base")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .and_then(|a| a.iter().map(Value::as_str).collect());
    let Some(base) = base else {
        errors.push("base must be a non-empty array of strings".into());
        return Err(errors);
    };
    let w = base[0].chars().count();
    let h = base.len();
    if w < 4 || h < 4 || w > MAX_SIDE || h > MAX_SIDE {
        errors.push(format!("base must be between 4x4 and {MAX_SIDE}x{MAX_SIDE} (got {w}x{h})"));
    }
    let check_row = |row: &str, place: &str, errors: &mut Vec<String>| {
        if row.chars().count() != w {
            errors.push(format!("{place} must be exactly {w} characters"));
            return;
        }
        for ch in row.chars() {
            if !is_clear(ch) && !palette.is_some_and(|p| p.get(&ch.to_string()).is_some_and(Value::is_string)) {
                errors.push(format!("{place}: \"{ch}\" is not in the palette"));
                return;
            }
        }
    };
    for (i, row) in base.iter().enumerate() {
        check_row(row, &format!("base row {i}"), &mut errors);
    }
    match obj.get("frames") {
        None => {}
        Some(Value::Object(frames)) => {
            for (name, patch) in frames {
                if !FRAME_NAMES.contains(&name.as_str()) {
                    errors.push(format!("unknown frame \"{name}\" (use {})", FRAME_NAMES.join(", ")));
                    continue;
                }
                let Some(patch) = patch.as_object() else {
                    errors.push(format!("frame {name} must map row numbers to rows"));
                    continue;
                };
                for (key, row) in patch {
                    match key.parse::<usize>() {
                        Ok(i) if i < h => match row.as_str() {
                            Some(row) => check_row(row, &format!("frame {name} row {key}"), &mut errors),
                            None => errors.push(format!("frame {name} row {key} must be exactly {w} characters")),
                        },
                        _ => errors.push(format!("frame {name}: row \"{key}\" is out of range")),
                    }
                }
            }
        }
        Some(_) => errors.push("frames must be an object".into()),
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

pub fn frame_rows(pet: &Pet, name: &str) -> Vec<String> {
    let mut current = name.to_string();
    while current != "base" && !pet.frames.contains_key(&current) {
        current = if current == "sleep" { "blink".into() } else { "base".into() };
    }
    let mut rows = pet.base.clone();
    if current != "base" {
        if let Some(patch) = pet.frames.get(&current) {
            for (key, row) in patch {
                if let Ok(i) = key.parse::<usize>() {
                    if i < rows.len() {
                        rows[i] = row.clone();
                    }
                }
            }
        }
    }
    rows
}

pub struct Metrics {
    pub w: usize,
    pub h: usize,
    pub lw: usize,
    pub lh: usize,
}

pub fn metrics(pet: &Pet) -> Metrics {
    let w = pet.base[0].chars().count();
    let h = pet.base.len();
    Metrics { w, h, lw: w + MARGIN_X * 2, lh: h + MARGIN_TOP + MARGIN_BOTTOM }
}

fn hex_to_rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    let full: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let n = u32::from_str_radix(&full, 16).unwrap_or(0);
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

pub fn tray_rgba(pet: &Pet, size: usize) -> Vec<u8> {
    let rows: Vec<Vec<char>> = frame_rows(pet, "base").iter().map(|r| r.chars().collect()).collect();
    let w = rows[0].len();
    let h = rows.len();
    let scale = (size / w.max(h)).max(1);
    let ox = (size.saturating_sub(w * scale)) / 2;
    let oy = (size.saturating_sub(h * scale)) / 2;
    let mut buf = vec![0u8; size * size * 4];
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.iter().enumerate() {
            if is_clear(*ch) {
                continue;
            }
            let Some(colour) = pet.palette.get(&ch.to_string()) else { continue };
            let [r, g, b] = hex_to_rgb(colour);
            for sy in 0..scale {
                for sx in 0..scale {
                    let px = ox + x * scale + sx;
                    let py = oy + y * scale + sy;
                    if px >= size || py >= size {
                        continue;
                    }
                    let i = (py * size + px) * 4;
                    buf[i..i + 4].copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
    buf
}

fn clean_id(stem: &str) -> String {
    stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect()
}

fn read_pet(text: &str, stem: &str, custom: bool) -> Option<Pet> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    validate(&value).ok()?;
    let mut pet: Pet = serde_json::from_value(value).ok()?;
    pet.id = format!("{}{}", if custom { "custom-" } else { "" }, clean_id(stem));
    pet.custom = custom;
    pet.name = pet.name.chars().take(40).collect();
    Some(pet)
}

pub fn load_all(paths: &Paths) -> Vec<Pet> {
    let mut builtin: Vec<(String, &str)> = BUILTIN
        .files()
        .filter_map(|f| {
            let name = f.path().file_name()?.to_str()?.to_string();
            Some((name, f.contents_utf8()?))
        })
        .filter(|(n, _)| n.ends_with(".json"))
        .collect();
    builtin.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<Pet> = builtin
        .iter()
        .filter_map(|(name, text)| read_pet(text, name.trim_end_matches(".json"), false))
        .collect();
    if let Ok(entries) = fs::read_dir(&paths.custom_pets_dir) {
        let mut files: Vec<_> = entries
            .flatten()
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".json")))
            .collect();
        files.sort_by_key(|e| e.file_name());
        for entry in files {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(pet) = fs::read_to_string(entry.path()).ok().and_then(|t| read_pet(&t, name.trim_end_matches(".json"), true)) {
                out.push(pet);
            }
        }
    }
    out
}

pub fn portable(pet: &Pet) -> Value {
    let mut out = Map::new();
    out.insert("format".into(), json!(1));
    out.insert("name".into(), json!(pet.name));
    out.insert("palette".into(), json!(pet.palette));
    out.insert("base".into(), json!(pet.base));
    out.insert("frames".into(), json!(pet.frames));
    if let Some(outline) = &pet.outline {
        out.insert("outline".into(), outline.clone());
    }
    Value::Object(out)
}

pub fn slug(name: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    trimmed.chars().take(limit).collect()
}

pub fn save_custom(paths: &Paths, value: &Value) -> Result<String, String> {
    validate(value).map_err(|errors| errors.into_iter().take(3).collect::<Vec<_>>().join("; "))?;
    let pet: Pet = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    fs::create_dir_all(&paths.custom_pets_dir).map_err(|e| e.to_string())?;
    let mut slug = slug(&pet.name, 24);
    if slug.is_empty() {
        slug = "pet".into();
    }
    let rand = uuid::Uuid::new_v4().simple().to_string();
    let file = format!("{slug}-{}", &rand[..4]);
    let text = serde_json::to_string_pretty(&portable(&pet)).map_err(|e| e.to_string())?;
    fs::write(paths.custom_pets_dir.join(format!("{file}.json")), text + "\n").map_err(|e| e.to_string())?;
    Ok(format!("custom-{file}"))
}

pub fn delete_custom(paths: &Paths, id: &str) {
    let Some(file) = id.strip_prefix("custom-") else { return };
    let file = clean_id(file);
    if !file.is_empty() {
        let _ = fs::remove_file(paths.custom_pets_dir.join(format!("{file}.json")));
    }
}

pub fn hatch_prompt(description: &str) -> String {
    let start = HATCH_SKILL.find("## Pet format").unwrap_or(0);
    let end = HATCH_SKILL.find("## Saving the pet").filter(|e| *e > start).unwrap_or(HATCH_SKILL.len());
    let spec = HATCH_SKILL[start..end].trim();
    [
        "Design a pixel-art desktop pet for the \"Nibble\" app.",
        "",
        spec,
        "",
        "Reply with ONLY the pet JSON object: no prose, no code fence. Do not use any tools.",
        "",
        &format!("The pet to design: {description}"),
    ]
    .join("\n")
}

pub fn extract_json(text: &str) -> Result<Value, String> {
    let start = text.find('{').ok_or("No JSON found in the reply")?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return serde_json::from_str(&text[start..start + i + 1]).map_err(|e| e.to_string());
            }
        }
    }
    Err("The pet JSON was cut off".into())
}
