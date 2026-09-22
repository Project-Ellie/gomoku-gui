//! Reading and writing files.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::RecordError;
use crate::game::Game;
use crate::record::Record;

/// Write text to `path` through a temporary file and a rename.
///
/// A rename inside one file system is atomic, so a crash leaves either the
/// old file or the new file, never a half-written file.
///
/// # Errors
/// The error from the file system.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let temporary = temporary_path(path);
    let mut file = fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

/// Read a whole file as text.
///
/// # Errors
/// The error from the file system.
pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

/// Save a game to `path`.
///
/// # Errors
/// `RecordError::Io` if the file cannot be written.
/// `RecordError::Parse` if the record cannot be encoded.
pub fn save(path: &Path, game: &Game) -> Result<(), RecordError> {
    let text = Record::from_game(game).to_json()?;
    write_atomic(path, &text).map_err(|source| RecordError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Load a game from `path` and check every field of the record.
///
/// # Errors
/// `RecordError::Io` if the file cannot be read.
/// `RecordError::Parse` if the text is not a record.
/// A validation error if the contents are wrong.
pub fn load(path: &Path) -> Result<Game, RecordError> {
    let text = read_to_string(path).map_err(|source| RecordError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Record::from_json(&text)?.into_game()
}

/// The path of the temporary file that `write_atomic` uses.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}
