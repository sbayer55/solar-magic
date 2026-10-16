//! User-given level names, kept in a JSON sidecar next to the ROM
//! (`<rom>.names.json`). They override the overworld's own names
//! (`smw_data::level::OverworldNames`), which only cover the overworld slots.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use smw_data::LevelId;

/// Names for level slots, loaded from and saved to a sidecar file.
#[derive(Debug, Clone, Default)]
pub(crate) struct LevelNames {
    names: BTreeMap<LevelId, String>,
    path: Option<PathBuf>,
}

impl LevelNames {
    /// The sidecar for a ROM path.
    fn sidecar(rom: &Path) -> PathBuf {
        let mut name = rom.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
        name.push(".names.json");
        rom.with_file_name(name)
    }

    /// Loads the names next to `rom` (none when there is no sidecar yet).
    pub(crate) fn load(rom: &Path) -> Self {
        let path = Self::sidecar(rom);
        let names = std::fs::read(&path)
            .ok()
            .and_then(|json| serde_json::from_slice::<BTreeMap<String, String>>(&json).ok())
            .map(|map| {
                map.into_iter()
                    .filter_map(|(k, v)| {
                        let id = LevelId::new(u16::from_str_radix(&k, 16).ok()?)?;
                        Some((id, v))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { names, path: Some(path) }
    }

    /// The name of a slot, if one was given.
    pub(crate) fn get(&self, id: LevelId) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    /// Sets (or, with an empty name, clears) a slot's name and writes the
    /// sidecar. Returns the write error, if any.
    pub(crate) fn set(&mut self, id: LevelId, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            self.names.remove(&id);
        } else {
            self.names.insert(id, name.to_owned());
        }
        self.save()
    }

    /// Moves the sidecar along with the ROM (after Save As).
    pub(crate) fn follow(&mut self, rom: &Path) {
        self.path = Some(Self::sidecar(rom));
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if self.names.is_empty() {
            if path.exists() {
                std::fs::remove_file(path).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        let map: BTreeMap<String, &str> =
            self.names.iter().map(|(id, name)| (id.to_string(), name.as_str())).collect();
        let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use smw_data::LevelId;

    use super::LevelNames;

    #[test]
    fn round_trips_through_the_sidecar() {
        let dir = std::env::temp_dir().join(format!("solar-magic-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rom = dir.join("game.sfc");
        let id = LevelId::new(0x105).unwrap();
        let mut names = LevelNames::load(&rom);
        assert_eq!(names.get(id), None);
        names.set(id, "  Yoshi's Island 1 ").unwrap();
        assert_eq!(names.get(id), Some("Yoshi's Island 1"));
        let reloaded = LevelNames::load(&rom);
        assert_eq!(reloaded.get(id), Some("Yoshi's Island 1"));
        assert!(dir.join("game.sfc.names.json").exists());
        names.set(id, "").unwrap();
        assert!(!dir.join("game.sfc.names.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
