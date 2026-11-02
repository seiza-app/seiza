//! New versions of the app (PLAN §8, Updates). Unless Settings turns it off, the app checks when
//! it starts and then once a day, with one request for `latest.json` on GitHub Releases (the
//! endpoint in tauri.conf.json). The Sky offers each new version once, and Settings keeps it
//! until it's installed. Installing checks the download's signature against the public key in
//! tauri.conf.json, replaces the app and restarts it, between sessions only.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use ts_rs::TS;

use crate::error::{Error, Result};

const DAY_MS: i64 = 24 * 60 * 60_000;

/// What the window shows about updates, sent as "updates" whenever it changes.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateStatus {
    /// The version running now, like "0.2.0".
    pub current_version: String,
    /// The newer version that's out, if a check found one.
    pub available: Option<String>,
    /// When the app last checked (epoch ms), if it has since it started.
    pub checked_at: Option<i64>,
    pub checking: bool,
    /// Whether the new version is downloading and installing, before the app restarts.
    pub installing: bool,
    /// Whether this copy can install a new version itself. Copies that weren't installed from a
    /// release, like `tauri dev`'s, can't.
    pub can_install: bool,
}

#[derive(Default)]
pub struct Updates(Mutex<State>);

#[derive(Default)]
struct State {
    found: Option<Update>,
    checked_at: Option<i64>,
    checking: bool,
    installing: bool,
}

fn state(app: &AppHandle) -> MutexGuard<'_, State> {
    app.state::<Updates>()
        .inner()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    let state = state(app);
    UpdateStatus {
        current_version: app.package_info().version.to_string(),
        available: state.found.as_ref().map(|update| update.version.clone()),
        checked_at: state.checked_at,
        checking: state.checking,
        installing: state.installing,
        can_install: tauri::utils::platform::bundle_type().is_some(),
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("updates", status(app));
}

/// Asks whether a new version is out.
pub async fn check(app: &AppHandle) -> Result<UpdateStatus> {
    {
        let mut state = state(app);
        if state.checking || state.installing {
            drop(state);
            return Ok(status(app));
        }
        state.checking = true;
    }
    changed(app);
    let checked = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };
    {
        let mut state = state(app);
        state.checking = false;
        if let Ok(found) = &checked {
            state.found = found.clone();
            state.checked_at = Some(crate::now_ms());
        }
    }
    changed(app);
    checked?;
    Ok(status(app))
}

/// Checks when the app starts and then once a day, while Settings allows it. It wakes every
/// hour to look at the clock, since one long sleep would be put off by however long the computer
/// slept, and a check that fails (say, offline) is tried again then.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let on = crate::engine(&app).settings().check_for_updates;
            let checked_at = state(&app).checked_at;
            if on
                && checked_at.is_none_or(|at| crate::now_ms() - at >= DAY_MS)
                && let Err(error) = check(&app).await
            {
                eprintln!("Seiza: couldn't check for updates: {error}");
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

/// Downloads and installs the new version a check found, then restarts the app. Not during a
/// session, since restarting would drop its blocking for a moment.
pub async fn install(app: &AppHandle) -> Result<()> {
    if crate::engine(app).session().is_some() {
        return Err(Error::SessionRunning);
    }
    let update = {
        let mut state = state(app);
        if state.installing {
            return Ok(());
        }
        let update = state.found.clone().ok_or(Error::NoUpdate)?;
        state.installing = true;
        update
    };
    changed(app);
    if let Err(error) = update.download_and_install(|_, _| {}, || {}).await {
        state(app).installing = false;
        changed(app);
        return Err(error.into());
    }
    app.restart()
}
