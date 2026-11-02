// Commands the window can call with `invoke('name', { args })`. Tauri turns the camelCase
// argument names the UI uses into these snake_case ones.

use tauri::{AppHandle, Manager};

use crate::apps::{AppRule, InstalledApp};
use crate::backup::{Backup, ImportReport, LastBackup, SavedBackup};
use crate::blocker::BlockerStatus;
use crate::engine::{Activity, ConstellationName, SkyState, Snapshot};
use crate::error::{Error, Result};
use crate::satellites::SatelliteAction;
use crate::session::{EndReason, GateVisit, SessionRecord};
use crate::settings::Settings;
use crate::sites::{SiteLevel, SiteRule};
use crate::sync::setup::{PairingState, SyncDevice, SyncStatus};
use crate::timer::{Strictness, TimerSettings};
use crate::updates::UpdateStatus;

#[tauri::command]
pub fn snapshot(app: AppHandle) -> Snapshot {
    crate::engine(&app).snapshot(crate::now_ms())
}

/// Starts a session, with a goal if its profile asked for one and one was given.
#[tauri::command]
pub fn start_session(app: AppHandle, profile_id: String, goal: Option<String>) -> Result<()> {
    crate::start_session(&app, &profile_id, goal.as_deref())
}

/// Answers "Did you do it?" for the last session's goal.
#[tauri::command]
pub fn answer_goal(app: AppHandle, session_id: String, met: bool) -> Result<()> {
    crate::engine(&app).answer_goal(&session_id, met, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_profile_asks_goal(app: AppHandle, profile_id: String, on: bool) -> Result<()> {
    crate::engine(&app).set_profile_asks_goal(&profile_id, on, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Ends the session. The window asks for a Gentle or Medium session's pause or typing first.
#[tauri::command]
pub fn end_session(app: AppHandle) -> Result<()> {
    crate::end_session(&app, EndReason::Stopped)
}

/// Ends a Hard session before its timer is done, using up an emergency pass.
#[tauri::command]
pub fn use_emergency_pass(app: AppHandle) -> Result<()> {
    crate::end_session(&app, EndReason::EmergencyPass)
}

#[tauri::command]
pub fn set_profile_timer(app: AppHandle, profile_id: String, timer: TimerSettings) -> Result<()> {
    crate::engine(&app).set_profile_timer(&profile_id, timer, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_profile_star_color(app: AppHandle, profile_id: String, color: String) -> Result<()> {
    crate::engine(&app).set_profile_star_color(&profile_id, &color, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_profile_plan(app: AppHandle, profile_id: String, plan: String) -> Result<()> {
    crate::engine(&app).set_profile_plan(&profile_id, &plan, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Changes the satellite box's hotkey, once the new one is registered. Empty turns it off.
#[tauri::command]
pub fn set_satellite_hotkey(app: AppHandle, hotkey: String) -> Result<()> {
    let old = crate::engine(&app).settings().satellite_hotkey.clone();
    let new = crate::capture::rebind(&app, &old, &hotkey)?;
    let saved = crate::engine(&app).set_satellite_hotkey(new.clone(), crate::now_ms());
    if let Err(error) = saved {
        let _ = crate::capture::rebind(&app, &new, &old);
        return Err(error);
    }
    crate::changed(&app);
    Ok(())
}

/// Ignores the satellite box's hotkey while Settings records a new one, so pressing the one
/// in use doesn't open the box.
#[tauri::command]
pub fn pause_satellite_hotkey(app: AppHandle, paused: bool) {
    app.state::<crate::capture::Paused>()
        .0
        .store(paused, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<()> {
    crate::engine(&app).set_settings(settings, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Stops watching a browser during sessions. Only between sessions.
#[tauri::command]
pub fn forget_browser(app: AppHandle, path: String) -> Result<()> {
    crate::engine(&app).forget_browser(&path, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_profile_apps(app: AppHandle, profile_id: String, apps: Vec<AppRule>) -> Result<()> {
    crate::engine(&app).set_profile_apps(&profile_id, apps, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_profile_sites(
    app: AppHandle,
    profile_id: String,
    sites: Vec<SiteRule>,
    other_sites: SiteLevel,
) -> Result<()> {
    crate::engine(&app).set_profile_sites(&profile_id, sites, other_sites, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Whether the blocker's helper is installed and running, for Settings.
#[tauri::command]
pub async fn blocker_status() -> BlockerStatus {
    let status = tauri::async_runtime::spawn_blocking(crate::blocker::status).await;
    status.unwrap_or(BlockerStatus {
        supported: false,
        installed: false,
        running: false,
        up_to_date: false,
    })
}

/// Installs or updates the blocker's helper. macOS asks for an administrator's password in its
/// own dialog; Seiza never sees it.
#[tauri::command]
pub async fn install_blocker(app: AppHandle) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let installed = tauri::async_runtime::spawn_blocking(crate::blocker::install).await;
        installed
            .map_err(|error| Error::Blocker(error.to_string()))?
            .map_err(Error::Blocker)?;
        // The helper takes a moment to start, and the app checks in again within a minute anyway.
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        crate::resync_blocker(&app);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(Error::Blocker(
            "blocking without the extension is macOS only for now".into(),
        ))
    }
}

/// Removes the blocker's helper and turns the setting off. Not while a Hard session is relying
/// on it.
#[tauri::command]
pub async fn remove_blocker(app: AppHandle) -> Result<()> {
    {
        let engine = crate::engine(&app);
        let hard = engine.session().and_then(|s| s.lock(crate::now_ms())) == Some(Strictness::Hard);
        if hard && engine.settings().block_without_extension {
            return Err(Error::Locked);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let removed = tauri::async_runtime::spawn_blocking(crate::blocker::remove).await;
        removed
            .map_err(|error| Error::Blocker(error.to_string()))?
            .map_err(Error::Blocker)?;
    }
    let settings = Settings {
        block_without_extension: false,
        ..crate::engine(&app).settings().clone()
    };
    crate::engine(&app).set_settings(settings, crate::now_ms())?;
    crate::resync_blocker(&app);
    crate::changed(&app);
    Ok(())
}

/// The apps that can be added to a profile, leaving out Seiza itself. Reading them touches
/// the disk, so it runs off the main thread.
#[tauri::command]
pub async fn installed_apps() -> Vec<InstalledApp> {
    let own = std::env::current_exe().ok();
    tauri::async_runtime::spawn_blocking(move || {
        let mut apps = crate::apps::installed();
        apps.retain(|app| own.as_deref() != Some(std::path::Path::new(&app.path)));
        apps
    })
    .await
    .unwrap_or_default()
}

/// Launches a thought into orbit. Returns whether there was anything to launch.
#[tauri::command]
pub fn launch_satellite(app: AppHandle, text: String) -> Result<bool> {
    let launched = crate::engine(&app).launch_satellite(&text, crate::now_ms())?;
    if launched {
        crate::satellites_changed(&app);
    }
    Ok(launched)
}

/// Ticks off, snoozes, keeps as a task or deletes a satellite.
#[tauri::command]
pub fn act_on_satellite(app: AppHandle, id: String, action: SatelliteAction) -> Result<()> {
    crate::engine(&app).act_on_satellite(&id, action, crate::now_ms())?;
    crate::satellites_changed(&app);
    Ok(())
}

/// Every finished session, oldest first, for the night sky.
#[tauri::command]
pub fn session_log(app: AppHandle) -> Result<Vec<SessionRecord>> {
    crate::engine(&app).session_log()
}

/// The names people gave constellations.
#[tauri::command]
pub fn constellation_names(app: AppHandle) -> Result<Vec<ConstellationName>> {
    crate::engine(&app).constellation_names()
}

/// Names a constellation, by the id of its first star. A blank name goes back to the made-up one.
#[tauri::command]
pub fn name_constellation(app: AppHandle, id: String, name: String) -> Result<()> {
    crate::engine(&app).name_constellation(&id, &name, crate::now_ms())?;
    crate::sync::exchange::send(&app);
    Ok(())
}

/// The night sky's secret for rare events, and when the Sky last showed new things.
#[tauri::command]
pub fn sky_state(app: AppHandle) -> Result<SkyState> {
    crate::engine(&app).sky_state(crate::now_ms())
}

/// Notes that the Sky has shown everything up to `at` (epoch ms), so it plays once.
#[tauri::command]
pub fn sky_seen(app: AppHandle, at: i64) -> Result<()> {
    crate::engine(&app).sky_seen(at, crate::now_ms())
}

/// Saves a backup of everything in the Downloads folder. `day` is today's local date, like
/// "2026-10-29", for the file's name.
#[tauri::command]
pub fn export_backup(app: AppHandle, day: String) -> Result<SavedBackup> {
    // Let go of the engine before writing the file.
    let backup = crate::engine(&app).backup(crate::now_ms())?;
    let path = crate::backup::save(&backup, &day)?;
    let name = |p: Option<&std::ffi::OsStr>| p.map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let saved = SavedBackup {
        file_name: name(path.file_name()),
        folder: name(path.parent().and_then(|p| p.file_name())),
    };
    *app.state::<LastBackup>()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
    Ok(saved)
}

/// Shows the last backup saved in Finder (or the system's file manager).
#[tauri::command]
pub fn show_backup(app: AppHandle) -> Result<()> {
    let path = app
        .state::<LastBackup>()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let path = path.ok_or_else(|| Error::Backup("there's no backup to show yet".into()))?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|error| Error::Backup(error.to_string()))
}

/// Brings in a backup file's contents, adding to what's here. Only between sessions.
#[tauri::command]
pub fn import_backup(app: AppHandle, json: String) -> Result<ImportReport> {
    let backup = Backup::read(&json)?;
    let report = crate::engine(&app).import(backup, crate::now_ms())?;
    crate::changed(&app);
    Ok(report)
}

/// Notes that the tour for new users has been offered, taken or turned down.
#[tauri::command]
pub fn tour_offered(app: AppHandle) -> Result<()> {
    crate::engine(&app).tour_offered(crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Whether a new version of the app is out, as of the last check.
#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    crate::updates::status(&app)
}

/// Checks for a new version now, from Settings.
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateStatus> {
    crate::updates::check(&app).await
}

/// Installs the new version and restarts the app, between sessions.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<()> {
    crate::updates::install(&app).await
}

/// Notes that the Sky has offered this version, so it doesn't offer it again.
#[tauri::command]
pub fn update_offered(app: AppHandle, version: String) -> Result<()> {
    crate::engine(&app).update_offered(&version, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Adds a quote of the user's own, for the Sky and the extension's quote page.
#[tauri::command]
pub fn add_own_quote(app: AppHandle, text: String, author: String) -> Result<()> {
    if crate::engine(&app).add_own_quote(&text, &author, crate::now_ms())? {
        crate::changed(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn remove_own_quote(app: AppHandle, id: String) -> Result<()> {
    crate::engine(&app).remove_own_quote(&id, crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Trips to redirected sites that went back to focus, oldest first, for comets.
#[tauri::command]
pub fn turned_back(app: AppHandle) -> Result<Vec<GateVisit>> {
    crate::engine(&app).turned_back()
}

/// Adds six weeks of made-up sessions, so the sky and the weekly report can be tried without
/// focusing for weeks. Development builds only.
#[tauri::command]
pub fn add_sample_data(app: AppHandle) -> Result<()> {
    if !cfg!(debug_assertions) {
        return Err(Error::DevelopmentOnly);
    }
    crate::engine(&app).add_samples(crate::now_ms())?;
    crate::changed(&app);
    Ok(())
}

/// Removes the sample data again. Development builds only.
#[tauri::command]
pub fn remove_sample_data(app: AppHandle) -> Result<()> {
    if !cfg!(debug_assertions) {
        return Err(Error::DevelopmentOnly);
    }
    crate::engine(&app).remove_samples()?;
    crate::changed(&app);
    Ok(())
}

/// Everything logged from `from` up to (not including) `to` (epoch ms), for the weekly report.
#[tauri::command]
pub fn activity(app: AppHandle, from: i64, to: i64) -> Result<Activity> {
    crate::engine(&app).activity(from, to)
}

/// The first day of the week as set on this computer, 0 being Sunday, or null if the system
/// doesn't say.
#[tauri::command]
pub fn first_weekday() -> Option<u8> {
    crate::calendar::first_weekday()
}

/// Closes the satellite box and hands focus back to the app you were in.
#[tauri::command]
pub fn close_capture(app: AppHandle) -> Result<()> {
    crate::capture::close(&app, true)
}

// Settings › Sync (sync/setup.rs). Each that talks to the server is async, so the window stays
// responsive while it waits.

#[tauri::command]
pub fn sync_status(app: AppHandle) -> Result<SyncStatus> {
    crate::sync::setup::status(&app)
}

/// Start a new sync group on the server at `address`, with this computer as its first device.
#[tauri::command]
pub async fn sync_start_group(
    app: AppHandle,
    address: String,
    device_name: String,
    invite: Option<String>,
) -> Result<()> {
    crate::sync::setup::start_group(&app, &address, &device_name, invite.as_deref()).await
}

/// Join with a pairing code shown on a computer already in the group.
#[tauri::command]
pub async fn sync_join(app: AppHandle, address: String, code: String, device_name: String) -> Result<PairingState> {
    crate::sync::setup::join(&app, &address, &code, &device_name).await
}

/// Add a device: shows a pairing code for the new computer.
#[tauri::command]
pub async fn sync_add_device(app: AppHandle) -> Result<PairingState> {
    crate::sync::setup::add_device(&app).await
}

/// Add or Don't add, once the new computer has typed the code.
#[tauri::command]
pub fn sync_answer_device(app: AppHandle, add: bool) {
    crate::sync::setup::answer_device(&app, add);
}

#[tauri::command]
pub fn sync_pairing(app: AppHandle) -> PairingState {
    crate::sync::setup::pairing(&app)
}

/// Stops the pairing that's running, so its code can't be used.
#[tauri::command]
pub fn sync_stop_pairing(app: AppHandle) {
    crate::sync::setup::stop_pairing(&app);
}

#[tauri::command]
pub async fn sync_devices(app: AppHandle) -> Result<Vec<SyncDevice>> {
    crate::sync::setup::devices(&app).await
}

#[tauri::command]
pub async fn sync_remove_device(app: AppHandle, device_id: String) -> Result<()> {
    crate::sync::setup::remove_device(&app, &device_id).await
}

/// Leave sync. Returns whether the server heard.
#[tauri::command]
pub async fn sync_leave(app: AppHandle) -> Result<bool> {
    crate::sync::setup::leave(&app).await
}

/// Move to another server, at `address`.
#[tauri::command]
pub async fn sync_move_server(app: AppHandle, address: String, invite: Option<String>) -> Result<()> {
    crate::sync::setup::move_server(&app, &address, invite.as_deref()).await
}
