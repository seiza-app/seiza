mod apps;
mod backup;
pub mod blocker;
pub mod bridge;
mod browsers;
mod calendar;
mod capture;
mod commands;
mod engine;
mod error;
mod hosts;
mod hotkey;
mod idle;
mod launches;
mod link;
mod processes;
mod quotes;
// Sync uses these from its next steps on (PLAN §8 Sync).
#[allow(dead_code)]
mod records;
mod samples;
mod satellites;
mod session;
mod settings;
mod sites;
mod store;
mod sync;
mod timer;
mod tray;
mod updates;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;

use apps::AppWatch;
use browsers::{Action, BrowserWatch, Response};
use engine::Engine;
use error::Result;
use idle::IdleWatch;
use session::{EndReason, Session};
use store::Store;
use timer::TimerEnd;

const MAIN_WINDOW: &str = "main";

/// The current time in epoch milliseconds, like `Date.now()`.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub(crate) fn engine(app: &AppHandle) -> MutexGuard<'_, Engine> {
    // A panic while the lock was held "poisons" it. The engine saves every change before
    // updating itself, so it's still safe to use.
    app.state::<Mutex<Engine>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn start_session(app: &AppHandle, profile_id: &str, goal: Option<&str>) -> Result<()> {
    {
        let mut engine = engine(app);
        let now = now_ms();
        engine.start_session(profile_id, now)?;
        if let Some(goal) = goal {
            engine.set_session_goal(goal, now)?;
        }
    }
    changed(app);
    sync::exchange::fetch(app);
    Ok(())
}

pub(crate) fn end_session(app: &AppHandle, reason: EndReason) -> Result<()> {
    engine(app).end_session(now_ms(), reason)?;
    changed(app);
    sync::exchange::fetch(app);
    Ok(())
}

/// Tells the window and the menu bar that satellites changed. Nothing else depends on them.
pub(crate) fn satellites_changed(app: &AppHandle) {
    let snapshot = engine(app).snapshot(now_ms());
    let _ = app.emit("snapshot", &snapshot);
    if let Err(error) = tray::refresh(app, &snapshot) {
        eprintln!("Seiza: couldn't update the menu bar: {error}");
    }
}

/// Tells the window and the menu bar about a change, schedules the session's timer, idle
/// check, and its check on browsers and apps, and sends whatever changed to the sync group.
pub(crate) fn changed(app: &AppHandle) {
    // Copy what's needed and let go of the engine first, so the menu bar (which runs on the
    // main thread) never waits on a lock another thread holds.
    let (snapshot, session) = {
        let engine = engine(app);
        (engine.snapshot(now_ms()), engine.session().cloned())
    };
    let _ = app.emit("snapshot", &snapshot);
    link::broadcast(app);
    if let Err(error) = tray::refresh(app, &snapshot) {
        eprintln!("Seiza: couldn't update the menu bar: {error}");
    }
    watch_idle(
        app,
        session.as_ref().map(|s| s.id.clone()),
        snapshot.settings.idle_minutes,
    );
    watch_session(app, session.as_ref().map(|s| s.id.clone()));
    wake_watch(app);
    sync_blocker(app);
    schedule_timer(app, session, snapshot.session.map(|s| s.profile_name));
    sync::exchange::send(app);
}

/// The running session's check on its browsers and apps, and the session it's for.
#[derive(Default)]
struct WatchTask(Mutex<Option<(String, JoinHandle<()>)>>);

/// What the blocker's helper was last asked to block, and the task that keeps asking while it
/// blocks anything, so it knows the app is still there (blocker.rs).
#[derive(Default)]
struct BlockerTask(Mutex<Blocking>);

#[derive(Default)]
struct Blocking {
    sent: Option<Vec<String>>,
    task: Option<JoinHandle<()>>,
}

/// Has the blocker's helper block the running session's restricted sites, if blocking without
/// the extension is on, and unblock them once it ends or the setting is turned off. Nothing is
/// sent to a helper that was never needed.
pub(crate) fn sync_blocker(app: &AppHandle) {
    let (on, wanted) = {
        let engine = engine(app);
        let on = engine.settings().block_without_extension;
        let profile = engine.session().and_then(|s| engine.rules_of(s));
        let domains = profile
            .filter(|_| on)
            .map(blocker::restricted_domains)
            .unwrap_or_default();
        (on, blocker::hostnames_for(&domains))
    };
    let state = app.state::<BlockerTask>();
    let mut blocking = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if blocking.sent.as_ref() == Some(&wanted) {
        return;
    }
    let was_blocking = blocking.sent.as_ref().is_some_and(|names| !names.is_empty());
    blocking.sent = Some(wanted.clone());
    if let Some(previous) = blocking.task.take() {
        previous.abort();
    }
    if !on && !was_blocking {
        return;
    }
    blocking.task = Some(tauri::async_runtime::spawn(async move {
        let request = blocker::Request::Block { hostnames: wanted };
        let request = Arc::new(request);
        loop {
            let sending = Arc::clone(&request);
            let sent = tauri::async_runtime::spawn_blocking(move || blocker::send(&sending)).await;
            match sent {
                Ok(Ok(response)) if !response.ok => {
                    eprintln!(
                        "Seiza: the blocker couldn't update: {}",
                        response.error.unwrap_or_default()
                    );
                }
                Ok(Err(error)) => eprintln!("Seiza: couldn't reach the blocker: {error}"),
                _ => {}
            }
            let blocking = matches!(&*request, blocker::Request::Block { hostnames } if !hostnames.is_empty());
            if !blocking {
                return;
            }
            tokio::time::sleep(Duration::from_secs(blocker::CHECK_IN_SECS)).await;
        }
    }));
}

/// Asks the helper again, e.g. once it's been installed.
pub(crate) fn resync_blocker(app: &AppHandle) {
    app.state::<BlockerTask>()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .sent = None;
    sync_blocker(app);
}

/// Wakes the session's check on browsers and apps. Wake-ups that come while it's busy aren't
/// lost: the next wait returns straight away.
#[derive(Default)]
struct WatchWake(Arc<Notify>);

/// Has the running session check its browsers and apps now: an app launched or was unhidden
/// (launches.rs), a browser linked or unlinked, or something else changed.
pub(crate) fn wake_watch(app: &AppHandle) {
    app.state::<WatchWake>().0.notify_one();
}

/// While a session runs, checks when it starts and whenever it's woken (`wake_watch`):
/// - for the apps its profile closes or hides (apps.rs), which go as soon as it starts and
///   again whenever they're opened or unhidden;
/// - for known browsers running without the extension, acting on them as Settings and the
///   session's strictness say (browsers.rs). Sessions whose profile doesn't tame any sites
///   leave the browsers alone.
///
/// Between wake-ups it only waits on a timer when a browser's warning or close is due, or
/// where app launches can't be heard (Linux), so it doesn't tick away during a session.
fn watch_session(app: &AppHandle, session_id: Option<String>) {
    let task = app.state::<WatchTask>();
    let mut task = task.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Unlike the timer, it keeps going through changes to the same session, which happen
    // all the time while browsers sync, so its count towards closing a browser isn't lost.
    if task.as_ref().map(|(id, _)| id) == session_id.as_ref() {
        return;
    }
    if let Some((_, previous)) = task.take() {
        previous.abort();
    }
    let Some(session_id) = session_id else { return };

    let app = app.clone();
    let id = session_id.clone();
    let wake = Arc::clone(&app.state::<WatchWake>().0);
    let handle = tauri::async_runtime::spawn(async move {
        let mut browser_watch = BrowserWatch::default();
        let mut app_watch = AppWatch::default();
        loop {
            let now = now_ms();
            let (browser_paths, response, app_rules, profile_name) = {
                let engine = engine(&app);
                let Some(session) = engine.session().filter(|s| s.id == id) else {
                    return;
                };
                let browser_paths: Vec<PathBuf> = if engine.session_tames_sites(session) {
                    engine.browsers().iter().map(|b| b.path.clone().into()).collect()
                } else {
                    Vec::new()
                };
                let close = engine.settings().close_browsers;
                (
                    browser_paths,
                    Response::to(session.lock(now), close),
                    engine.session_apps(session),
                    engine.profile_name(&session.profile_id),
                )
            };
            if browser_paths.is_empty() {
                // Nothing to watch right now, e.g. every site in the profile is allowed.
                browser_watch = BrowserWatch::default();
            }

            let mut paths = browser_paths.clone();
            paths.extend(app_rules.iter().map(|rule| PathBuf::from(&rule.path)));
            if !paths.is_empty() {
                let running = tauri::async_runtime::spawn_blocking(move || processes::running(&paths))
                    .await
                    .unwrap_or_default();
                act_on_apps(&app, &profile_name, app_watch.check(&running, &app_rules));
                if !browser_paths.is_empty() {
                    let browsers: Vec<_> = running
                        .into_iter()
                        .filter(|process| browser_paths.contains(&process.path))
                        .collect();
                    let linked = link::linked_browsers(&app);
                    for action in browser_watch.check(now, &browsers, &linked, response) {
                        act_on_browser(&app, action);
                    }
                }
            }

            let due = browser_watch.next_due(response).map(|at| at - now_ms());
            let wait = [due, launches::POLL_EVERY_MS].into_iter().flatten().min();
            match wait {
                // Capped at a minute, since a computer that sleeps meanwhile would otherwise
                // leave the timer running long past when it's due.
                Some(ms) => {
                    let ms = ms.clamp(0, 60_000) as u64;
                    let _ = tokio::time::timeout(Duration::from_millis(ms), wake.notified()).await;
                }
                None => wake.notified().await,
            }
        }
    });
    *task = Some((session_id, handle));
}

/// Closes and hides the session's apps, and says which were closed the first time.
fn act_on_apps(app: &AppHandle, profile_name: &str, actions: Vec<apps::Action>) {
    let mut closed = Vec::new();
    for action in actions {
        match action {
            apps::Action::Close { pid, name, first } => {
                if processes::quit(pid) && first {
                    closed.push(name);
                }
            }
            apps::Action::Hide { pid } => {
                processes::hide(pid);
            }
        }
    }
    let Some((last, rest)) = closed.split_last() else {
        return;
    };
    let (names, them) = if rest.is_empty() {
        (last.clone(), "it")
    } else {
        (format!("{} and {last}", rest.join(", ")), "them")
    };
    let notification = app
        .notification()
        .builder()
        .title(format!("Closed {names}"))
        .body(format!("{profile_name} keeps {them} closed until the session ends."));
    if let Err(error) = notification.show() {
        eprintln!("Seiza: couldn't show the app rules notification: {error}");
    }
}

fn act_on_browser(app: &AppHandle, action: Action) {
    let (title, body) = match action {
        Action::Warn { name, response } => {
            let then = match response {
                Response::Warn => "to keep sites tamed there.".to_string(),
                Response::Close => format!("or {name} will close in a minute."),
            };
            (
                format!("Seiza isn't running in {name}"),
                format!("Turn the Seiza extension back on, {then}"),
            )
        }
        Action::Close { pid, name } => {
            if !processes::quit(pid) {
                eprintln!("Seiza: couldn't close {name}");
                return;
            }
            (
                format!("Closed {name}"),
                format!("The Seiza extension was off. Turn it on before opening {name} again."),
            )
        }
    };
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        eprintln!("Seiza: couldn't show a browser notification: {error}");
    }
}

/// The running session's check for idle time.
#[derive(Default)]
struct IdleTask(Mutex<Option<JoinHandle<()>>>);

/// Checks once a minute while a session runs, and ends it after `idle_minutes` without
/// keyboard or mouse input. Nothing runs between sessions, or where the system can't say how
/// long it's been idle. Before ending it, it asks the sync group's other computers running the
/// same session, and goes on if anyone's been using theirs (sync/live.rs).
fn watch_idle(app: &AppHandle, session_id: Option<String>, idle_minutes: u32) {
    let task = app.state::<IdleTask>();
    let mut task = task.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(previous) = task.take() {
        previous.abort();
    }

    let Some(session_id) = session_id else { return };
    if idle_minutes == 0 {
        return;
    }
    let limit = i64::from(idle_minutes) * 60_000;

    let app = app.clone();
    *task = Some(tauri::async_runtime::spawn(async move {
        let Some(idle_ms) = idle_reading().await else { return };
        let mut watch = IdleWatch::new(now_ms(), idle_ms);
        let mut elsewhere = i64::MIN;
        loop {
            tokio::time::sleep(Duration::from_millis(idle::CHECK_EVERY_MS as u64)).await;
            let Some(idle_ms) = idle_reading().await else { return };
            let last_input = watch.check(now_ms(), idle_ms).max(elsewhere);
            if now_ms() - last_input < limit {
                continue;
            }
            if let Some(at) = sync::live::last_input_elsewhere(&app, &session_id).await {
                elsewhere = elsewhere.max(at);
            }
            let last_input = last_input.max(elsewhere);
            let now = now_ms();
            if now - last_input >= limit {
                end_idle_session(&app, &session_id, last_input, now, idle_minutes);
                return;
            }
        }
    }));
}

/// Reads the system's idle time on a thread that's allowed to wait, since on Linux it waits on
/// D-Bus.
pub(crate) async fn idle_reading() -> Option<i64> {
    tauri::async_runtime::spawn_blocking(idle::system_idle_ms)
        .await
        .ok()
        .flatten()
}

fn end_idle_session(app: &AppHandle, session_id: &str, last_input: i64, now: i64, idle_minutes: u32) {
    let ended = engine(app).end_idle_session(session_id, last_input, now);
    match ended {
        Ok(Some(record)) => {
            let notification = app.notification().builder().title("Session ended").body(format!(
                "{} ended after {idle_minutes} minutes away. The time away isn't counted.",
                record.profile_name
            ));
            if let Err(error) = notification.show() {
                eprintln!("Seiza: couldn't show the idle notification: {error}");
            }
            changed(app);
        }
        Ok(None) => {}
        Err(error) => eprintln!("Seiza: couldn't end an idle session: {error}"),
    }
}

/// The running session's timer, waiting to go off.
#[derive(Default)]
struct TimerTask(Mutex<Option<JoinHandle<()>>>);

fn schedule_timer(app: &AppHandle, session: Option<Session>, profile_name: Option<String>) {
    let task = app.state::<TimerTask>();
    let mut task = task.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(previous) = task.take() {
        previous.abort();
    }

    let Some(session) = session else { return };
    let Some(timer) = session.timer else { return };
    let Some(done_at) = timer.done_at(session.started_at) else {
        return;
    };
    if done_at <= now_ms() || timer.on_end == TimerEnd::Silent {
        return;
    }

    let app = app.clone();
    let profile_name = profile_name.unwrap_or_default();
    *task = Some(tauri::async_runtime::spawn(async move {
        // Sleeps in steps of at most a minute, checking the clock each time. If the computer
        // sleeps, one long sleep would go off late by however long it was asleep.
        loop {
            let left = done_at - now_ms();
            if left <= 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(left.min(60_000) as u64)).await;
        }

        let mut notification = app.notification().builder().title("Timer done").body(format!(
            "{} minutes of {profile_name}. Keep going if you're in the flow.",
            timer.minutes
        ));
        if timer.on_end == TimerEnd::Alert {
            notification = notification.sound(ALERT_SOUND);
        }
        if let Err(error) = notification.show() {
            eprintln!("Seiza: couldn't show the timer notification: {error}");
        }
    }));
}

#[cfg(target_os = "macos")]
const ALERT_SOUND: &str = "Glass";
#[cfg(not(target_os = "macos"))]
const ALERT_SOUND: &str = "default";

/// Opens the window, or brings it to the front if it's already open, and fetches what the sync
/// group's other computers sent meanwhile.
pub(crate) fn open_window(app: &AppHandle) -> Result<()> {
    sync::exchange::fetch(app);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }
    // While the window is open, Seiza shows in the Dock and app switcher like any other app.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
    WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::default())
        .title("Seiza")
        .inner_size(400.0, 600.0)
        .min_inner_size(340.0, 440.0)
        .build()?;
    Ok(())
}

/// Creates the app's data folder, readable by this user only.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Opening Seiza again brings up the running app's window instead of starting a second copy.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(error) = open_window(app) {
                eprintln!("Seiza: couldn't open the window: {error}");
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(capture::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Seiza lives in the menu bar, with no Dock icon until its window opens.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // The same folder Tauri would pick, worked out without Tauri so the bridge finds it too.
            let dir = link::data_dir().ok_or("no data folder")?;
            create_private_dir(&dir)?;
            let engine = Engine::load(Store::open(&dir.join("lock-in.db"))?, now_ms())?;
            let snapshot = engine.snapshot(now_ms());
            let session = engine.session().cloned();
            app.manage(Mutex::new(engine));
            app.manage(TimerTask::default());
            app.manage(IdleTask::default());
            app.manage(WatchTask::default());
            app.manage(BlockerTask::default());
            app.manage(backup::LastBackup::default());
            let wake = WatchWake::default();
            launches::listen(Arc::clone(&wake.0));
            app.manage(wake);
            let handle = app.handle().clone();
            launches::on_wake(move || {
                sync::live::woke(&handle);
                sync::exchange::fetch(&handle);
            });
            app.manage(link::Links::default());
            app.manage(capture::CameFrom::default());
            app.manage(capture::Paused::default());
            app.manage(updates::Updates::default());
            app.manage(sync::setup::Pairings::default());
            app.manage(sync::exchange::Exchanges::default());
            app.manage(sync::live::Listener::default());

            tray::create(app.handle(), &snapshot)?;
            capture::listen(app.handle());
            watch_idle(
                app.handle(),
                session.as_ref().map(|s| s.id.clone()),
                snapshot.settings.idle_minutes,
            );
            watch_session(app.handle(), session.as_ref().map(|s| s.id.clone()));
            schedule_timer(app.handle(), session, snapshot.session.map(|s| s.profile_name));
            sync_blocker(app.handle());
            sync::live::watch(app.handle());
            tauri::async_runtime::spawn(link::serve(app.handle().clone()));
            updates::watch(app.handle());
            match std::env::current_exe() {
                Ok(executable) => {
                    for result in hosts::install(&executable) {
                        if let Err(error) = result {
                            eprintln!("Seiza: couldn't link a browser: {error}");
                        }
                    }
                }
                Err(error) => eprintln!("Seiza: couldn't find its own executable: {error}"),
            }

            open_window(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window destroys it, webview and all, so only the small core keeps running.
            WindowEvent::Destroyed if window.label() == MAIN_WINDOW => {
                #[cfg(target_os = "macos")]
                let _ = window
                    .app_handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            // Settings stops recording a new hotkey once you leave the window, so the hotkey
            // can't stay ignored.
            WindowEvent::Focused(false) if window.label() == MAIN_WINDOW => {
                window
                    .state::<capture::Paused>()
                    .0
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            // The satellite box goes away when you click somewhere else, like Spotlight.
            WindowEvent::Focused(false) if window.label() == capture::WINDOW => {
                if let Err(error) = capture::close(window.app_handle(), false) {
                    eprintln!("Seiza: couldn't close the satellite box: {error}");
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::start_session,
            commands::end_session,
            commands::set_profile_timer,
            commands::set_profile_star_color,
            commands::set_profile_plan,
            commands::set_profile_asks_goal,
            commands::answer_goal,
            commands::use_emergency_pass,
            commands::set_settings,
            commands::set_satellite_hotkey,
            commands::pause_satellite_hotkey,
            commands::forget_browser,
            commands::set_profile_apps,
            commands::set_profile_sites,
            commands::blocker_status,
            commands::install_blocker,
            commands::remove_blocker,
            commands::installed_apps,
            commands::launch_satellite,
            commands::act_on_satellite,
            commands::close_capture,
            commands::activity,
            commands::session_log,
            commands::constellation_names,
            commands::name_constellation,
            commands::sky_state,
            commands::sky_seen,
            commands::tour_offered,
            commands::update_status,
            commands::check_for_updates,
            commands::install_update,
            commands::update_offered,
            commands::export_backup,
            commands::show_backup,
            commands::import_backup,
            commands::add_own_quote,
            commands::remove_own_quote,
            commands::turned_back,
            commands::add_sample_data,
            commands::remove_sample_data,
            commands::first_weekday,
            commands::sync_status,
            commands::sync_start_group,
            commands::sync_join,
            commands::sync_add_device,
            commands::sync_answer_device,
            commands::sync_pairing,
            commands::sync_stop_pairing,
            commands::sync_devices,
            commands::sync_remove_device,
            commands::sync_set_follow,
            commands::sync_leave,
            commands::sync_move_server,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Seiza")
        .run(|_app, event| {
            // With the window closed there are no windows left, which would normally quit the
            // app. Seiza keeps running in the menu bar until you choose Quit.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
