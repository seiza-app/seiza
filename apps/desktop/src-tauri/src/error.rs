use std::fmt;

/// Everything that can go wrong in the app's core. Commands send these to the UI as text.
#[derive(Debug)]
pub enum Error {
    Database(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    Tauri(tauri::Error),
    SessionRunning,
    NoSession,
    UnknownProfile,
    /// A Hard session's timer is still running.
    Locked,
    NoPassesLeft,
    UnknownSatellite,
    InvalidStarColor,
    UnknownConstellation,
    DevelopmentOnly,
    Hotkey(crate::hotkey::Problem),
    TooManyQuotes,
    /// Installing, removing or reaching the blocker's helper went wrong.
    Blocker(String),
    /// A backup couldn't be saved or brought in.
    Backup(String),
    /// Checking for, downloading or installing a new version went wrong.
    Update(tauri_plugin_updater::Error),
    /// There's no new version to install, or no check has found one yet.
    NoUpdate,
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Database(error) => write!(f, "database error: {error}"),
            Error::Json(error) => write!(f, "couldn't read saved data: {error}"),
            Error::Io(error) => write!(f, "{error}"),
            Error::Tauri(error) => write!(f, "{error}"),
            Error::SessionRunning => f.write_str("a session is already running"),
            Error::NoSession => f.write_str("no session is running"),
            Error::UnknownProfile => f.write_str("that profile doesn't exist"),
            Error::Locked => f.write_str("this session is in Hard mode until its timer is done"),
            Error::NoPassesLeft => f.write_str("no emergency passes are left this week"),
            Error::UnknownSatellite => f.write_str("that satellite is already gone"),
            Error::InvalidStarColor => f.write_str("a star color has to be a hex color like #f5c86b"),
            Error::UnknownConstellation => f.write_str("that constellation doesn't exist"),
            Error::DevelopmentOnly => f.write_str("that's only in development builds"),
            Error::TooManyQuotes => write!(
                f,
                "that's the most quotes Seiza keeps ({}); remove one first",
                crate::quotes::MAX_OWN_QUOTES
            ),
            Error::Hotkey(problem) => write!(f, "{problem}"),
            Error::Blocker(problem) | Error::Backup(problem) => f.write_str(problem),
            Error::Update(error) => write!(f, "{error}"),
            Error::NoUpdate => f.write_str("there's no new version to install"),
        }
    }
}

impl std::error::Error for Error {}

// `?` converts library errors into ours through these.
impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::Database(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<crate::hotkey::Problem> for Error {
    fn from(problem: crate::hotkey::Problem) -> Self {
        Error::Hotkey(problem)
    }
}

impl From<tauri_plugin_updater::Error> for Error {
    fn from(error: tauri_plugin_updater::Error) -> Self {
        Error::Update(error)
    }
}

impl From<tauri::Error> for Error {
    fn from(error: tauri::Error) -> Self {
        Error::Tauri(error)
    }
}

// Tauri sends a command's error to the UI, so it has to be serializable.
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
