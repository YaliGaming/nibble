use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub pet_home: PathBuf,
    pub claude_home: PathBuf,
    pub events_dir: PathBuf,
    pub custom_pets_dir: PathBuf,
    pub quick_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub config_file: PathBuf,
    pub hook_file: PathBuf,
    pub app_info_file: PathBuf,
    pub settings_file: PathBuf,
    pub registry_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub app_data_dir: PathBuf,
}

pub const HOOK_BINARY: &str = if cfg!(windows) { "nibble-hook.exe" } else { "nibble-hook" };

impl Paths {
    pub fn detect() -> Paths {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let pet_home = std::env::var_os("NIBBLE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".nibble-pet"));
        let claude_home = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        Paths::from_homes(pet_home, claude_home)
    }

    pub fn from_homes(pet_home: PathBuf, claude_home: PathBuf) -> Paths {
        Paths {
            events_dir: pet_home.join("events"),
            custom_pets_dir: pet_home.join("pets"),
            quick_dir: pet_home.join("quick-chat"),
            tmp_dir: pet_home.join("tmp"),
            backup_dir: pet_home.join("backups"),
            config_file: pet_home.join("config.json"),
            hook_file: pet_home.join(HOOK_BINARY),
            app_info_file: pet_home.join("app.json"),
            app_data_dir: pet_home.join("app-data"),
            settings_file: claude_home.join("settings.json"),
            registry_dir: claude_home.join("sessions"),
            skills_dir: claude_home.join("skills"),
            pet_home,
            claude_home,
        }
    }

    pub fn skill_dir(&self, name: &str) -> PathBuf {
        self.skills_dir.join(name)
    }
}

pub fn forward(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
