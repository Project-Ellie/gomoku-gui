//! The settings file. A missing file is normal; the defaults apply.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::storage::{read_to_string, write_atomic};

/// The most recent record paths that are kept.
pub const MAX_RECENT: usize = 10;

/// Window geometry and state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    /// Window width in logical pixels.
    pub width: f32,
    /// Window height in logical pixels.
    pub height: f32,
    /// Window x position in logical pixels, if it is known.
    pub x: Option<f32>,
    /// Window y position in logical pixels, if it is known.
    pub y: Option<f32>,
    /// True when the window was maximised.
    pub maximized: bool,
}

impl Default for WindowSettings {
    fn default() -> WindowSettings {
        WindowSettings {
            width: 1180.0,
            height: 900.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

/// The zoom and the position of the view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewSettings {
    /// Pixels per board cell. Zero means "fit the board at start-up".
    pub pixels_per_cell: f32,
    /// The board point at the centre of the view, in cells.
    pub center: [f32; 2],
    /// True when the board is shown from the other side.
    pub flipped: bool,
}

impl Default for ViewSettings {
    fn default() -> ViewSettings {
        ViewSettings {
            pixels_per_cell: 0.0,
            center: [7.0, 7.0],
            flipped: false,
        }
    }
}

/// The four board overlays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    /// Show the coordinate labels.
    pub coordinates: bool,
    /// Show the move numbers on the stones.
    pub move_numbers: bool,
    /// Mark the last move.
    pub last_move: bool,
    /// Mark the winning line.
    pub win_line: bool,
}

impl Default for OverlaySettings {
    fn default() -> OverlaySettings {
        OverlaySettings {
            coordinates: true,
            move_numbers: false,
            last_move: true,
            win_line: true,
        }
    }
}

/// The board material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardSettings {
    /// The name of the material preset.
    pub material: String,
}

impl Default for BoardSettings {
    fn default() -> BoardSettings {
        BoardSettings {
            material: "aged_wood".to_string(),
        }
    }
}

/// The stone set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoneSettings {
    /// The name of the stone set.
    pub set: String,
}

impl Default for StoneSettings {
    fn default() -> StoneSettings {
        StoneSettings {
            set: "slate_shell".to_string(),
        }
    }
}

/// The sound settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// True when the stone knock plays.
    pub enabled: bool,
    /// The volume, from 0.0 to 1.0.
    pub volume: f32,
}

impl Default for AudioSettings {
    fn default() -> AudioSettings {
        AudioSettings {
            enabled: true,
            volume: 0.7,
        }
    }
}

/// The file settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FileSettings {
    /// The directory of the last save or open.
    pub last_directory: Option<PathBuf>,
    /// The most recently used record paths, newest first.
    pub recent: Vec<PathBuf>,
}

/// The settings of the application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    /// Window geometry and state.
    pub window: WindowSettings,
    /// Zoom and position of the view.
    pub view: ViewSettings,
    /// The board overlays.
    pub overlays: OverlaySettings,
    /// The board material.
    pub board: BoardSettings,
    /// The stone set.
    pub stones: StoneSettings,
    /// The sound settings.
    pub audio: AudioSettings,
    /// The file settings.
    pub files: FileSettings,
}

impl Settings {
    /// Clamp the values that a hand-edited file can put out of range.
    pub fn sanitize(&mut self) {
        self.audio.volume = self.audio.volume.clamp(0.0, 1.0);

        let mut recent: Vec<PathBuf> = Vec::with_capacity(self.files.recent.len());
        for path in &self.files.recent {
            if recent.len() < MAX_RECENT && !recent.contains(path) {
                recent.push(path.clone());
            }
        }
        self.files.recent = recent;
    }
}

/// What the caller should tell the user about a load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigNotice {
    /// There was no settings file.
    Missing,
    /// The settings file could not be read. It was moved to `backup`.
    Corrupt {
        /// The path of the quarantined file.
        backup: PathBuf,
    },
}

/// The result of a load.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoad {
    /// The settings, either read from the file or the defaults.
    pub settings: Settings,
    /// Anything the caller should report.
    pub notice: Option<ConfigNotice>,
}

/// Read the settings from `path`.
///
/// A missing file gives the defaults. A damaged file is moved aside so that
/// the next start is clean. This function never fails.
pub fn load_settings(path: &Path) -> ConfigLoad {
    let Ok(text) = read_to_string(path) else {
        return ConfigLoad {
            settings: Settings::default(),
            notice: Some(ConfigNotice::Missing),
        };
    };

    match toml::from_str::<Settings>(&text) {
        Ok(mut settings) => {
            settings.sanitize();
            ConfigLoad {
                settings,
                notice: None,
            }
        }
        Err(_) => {
            let backup = path.with_extension("toml.corrupt");
            let notice = match fs::rename(path, &backup) {
                Ok(()) => ConfigNotice::Corrupt { backup },
                Err(_) => ConfigNotice::Missing,
            };
            ConfigLoad {
                settings: Settings::default(),
                notice: Some(notice),
            }
        }
    }
}

/// Write the settings to `path`.
///
/// # Errors
/// `ConfigError::Io` if the file cannot be written.
/// `ConfigError::Encode` if the settings cannot be encoded.
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), ConfigError> {
    let text = toml::to_string(settings).map_err(|err| ConfigError::Encode(err.to_string()))?;
    write_atomic(path, &text).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_file() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.toml");
        (directory, path)
    }

    #[test]
    fn the_defaults_match_the_specification() {
        let settings = Settings::default();
        assert_eq!(settings.window.width, 1180.0);
        assert_eq!(settings.window.height, 900.0);
        assert_eq!(settings.view.pixels_per_cell, 0.0);
        assert_eq!(settings.view.center, [7.0, 7.0]);
        assert!(!settings.view.flipped);
        assert!(settings.overlays.coordinates);
        assert!(!settings.overlays.move_numbers);
        assert!(settings.overlays.last_move);
        assert!(settings.overlays.win_line);
        assert_eq!(settings.board.material, "aged_wood");
        assert_eq!(settings.stones.set, "slate_shell");
        assert!(settings.audio.enabled);
        assert_eq!(settings.audio.volume, 0.7);
        assert!(settings.files.recent.is_empty());
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_a_notice() {
        let (_directory, path) = settings_file();
        let loaded = load_settings(&path);
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.notice, Some(ConfigNotice::Missing));
    }

    #[test]
    fn the_settings_round_trip_through_a_file() {
        let (_directory, path) = settings_file();
        let settings = Settings {
            view: ViewSettings {
                pixels_per_cell: 42.5,
                flipped: true,
                ..ViewSettings::default()
            },
            overlays: OverlaySettings {
                move_numbers: true,
                ..OverlaySettings::default()
            },
            board: BoardSettings {
                material: "walnut".to_string(),
            },
            audio: AudioSettings {
                volume: 0.25,
                ..AudioSettings::default()
            },
            ..Settings::default()
        };

        save_settings(&path, &settings).expect("the settings save");
        let loaded = load_settings(&path);
        assert_eq!(loaded.settings, settings);
        assert_eq!(loaded.notice, None);
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_the_missing_fields() {
        let (_directory, path) = settings_file();
        std::fs::write(&path, "[audio]\nvolume = 0.5\n").expect("the file writes");

        let loaded = load_settings(&path);
        assert_eq!(loaded.notice, None);
        assert_eq!(loaded.settings.audio.volume, 0.5);
        assert!(
            loaded.settings.audio.enabled,
            "the missing field keeps its default"
        );
        assert_eq!(loaded.settings.board.material, "aged_wood");
    }

    #[test]
    fn a_damaged_file_is_quarantined_and_the_defaults_apply() {
        let (_directory, path) = settings_file();
        let damaged = "this is not toml === ";
        std::fs::write(&path, damaged).expect("the file writes");

        let loaded = load_settings(&path);
        match loaded.notice {
            Some(ConfigNotice::Corrupt { backup }) => {
                assert_eq!(backup, path.with_extension("toml.corrupt"));
                assert_eq!(
                    std::fs::read_to_string(&backup).expect("the backup reads"),
                    damaged,
                    "the damaged content must be preserved, not deleted"
                );
            }
            other => panic!("expected a corrupt notice, got {other:?}"),
        }
        assert_eq!(loaded.settings, Settings::default());
        assert!(!path.exists(), "the damaged file was moved away");
    }

    #[test]
    fn sanitize_clamps_the_volume() {
        let mut settings = Settings {
            audio: AudioSettings {
                volume: 4.2,
                ..AudioSettings::default()
            },
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.audio.volume, 1.0);

        settings.audio.volume = -1.0;
        settings.sanitize();
        assert_eq!(settings.audio.volume, 0.0);
    }

    #[test]
    fn sanitize_caps_and_deduplicates_the_recent_list() {
        let mut recent: Vec<std::path::PathBuf> = (0..12)
            .map(|index| std::path::PathBuf::from(format!("/games/{index}.json")))
            .collect();
        // The duplicate must sit INSIDE the retained window. Further back it
        // would be dropped by the cap alone, so the assertion would hold even
        // if the deduplication were removed.
        recent.insert(3, std::path::PathBuf::from("/games/0.json"));
        let mut settings = Settings {
            files: FileSettings {
                recent,
                ..FileSettings::default()
            },
            ..Settings::default()
        };

        settings.sanitize();

        // Assert the whole list. A cap-only implementation keeps the repeat and
        // stops one entry short, so it produces a different list.
        let expected: Vec<std::path::PathBuf> = (0..MAX_RECENT)
            .map(|index| std::path::PathBuf::from(format!("/games/{index}.json")))
            .collect();
        assert_eq!(settings.files.recent, expected);
    }
}
