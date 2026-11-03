use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuEvent, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Wry};

use crate::engine::Snapshot;
use crate::satellites::SatelliteState;

const TRAY_ID: &str = "main";
const START_PREFIX: &str = "start:";

/// Adds the menu-bar icon. Its menu is rebuilt whenever the session or profiles change.
pub fn create(app: &AppHandle, snapshot: &Snapshot) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        // On macOS a template icon is drawn in the menu bar's own colors, light or dark.
        .icon_as_template(true)
        .tooltip("Seiza")
        .menu(&menu(app, snapshot)?)
        .on_menu_event(on_menu_event)
        .build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle, snapshot: &Snapshot) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu(app, snapshot)?))?;
    }
    Ok(())
}

fn menu(app: &AppHandle, snapshot: &Snapshot) -> tauri::Result<Menu<Wry>> {
    let mut builder = MenuBuilder::new(app);
    // Satellites that came back wait quietly here and in the window. Nothing pops up when a
    // session ends, so it never pulls you out of the flow.
    let back = snapshot
        .satellites
        .iter()
        .filter(|s| s.state == SatelliteState::Back)
        .count();
    if back > 0 && snapshot.session.is_none() {
        let label = match back {
            1 => "1 satellite came back…".to_string(),
            n => format!("{n} satellites came back…"),
        };
        builder = builder.text("back", label).separator();
    }
    builder = match &snapshot.session {
        Some(session) => builder
            .item(&disabled(app, &format!("Locked in · {}", session.profile_name))?)
            .text("end", "End session"),
        None => {
            builder = builder.item(&disabled(app, "Start a session")?);
            for profile in &snapshot.profiles {
                builder = builder.text(format!("{START_PREFIX}{}", profile.id), &profile.name);
            }
            builder
        }
    };
    // The hotkey is only shown here; it works from anywhere (capture.rs). Both read the same text.
    let mut launch = MenuItemBuilder::with_id("satellite", "Launch a satellite…");
    if !snapshot.settings.satellite_hotkey.is_empty() {
        launch = launch.accelerator(&snapshot.settings.satellite_hotkey);
    }
    let launch = launch.build(app)?;
    builder
        .separator()
        .item(&launch)
        .text("open", "Open Seiza…")
        // Quitting ends a running session, so it says so.
        .text(
            "quit",
            if snapshot.session.is_some() {
                "End session and quit"
            } else {
                "Quit Seiza"
            },
        )
        .build()
}

fn disabled(app: &AppHandle, text: &str) -> tauri::Result<tauri::menu::MenuItem<Wry>> {
    tauri::menu::MenuItem::new(app, text, false, None::<&str>)
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    let result = match id {
        "end" => {
            // Its own statement, so the engine is unlocked again before ending the session.
            let early = crate::engine(app).session().and_then(|s| s.lock(crate::now_ms()));
            match early {
                // Ending early may need a pause, typing or a pass, which happen in the window.
                Some(_) => crate::open_window(app),
                None => crate::end_session(app, crate::session::EndReason::Stopped),
            }
        }
        "open" | "back" => crate::open_window(app),
        "satellite" => crate::capture::open(app),
        "quit" => {
            crate::quit(app);
            Ok(())
        }
        _ => match id.strip_prefix(START_PREFIX) {
            Some(profile_id) => crate::start_session(app, profile_id, None),
            None => Ok(()),
        },
    };
    if let Err(error) = result {
        eprintln!("Seiza: {error}");
    }
}
