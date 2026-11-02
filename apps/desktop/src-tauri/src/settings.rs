use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::hotkey;

/// App-wide settings, as opposed to per-profile ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
// `default` fills in any field a saved copy is missing, so new settings can be added later.
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    /// Emergency passes for ending a Hard session early, per 7 days.
    pub passes_per_week: u32,
    /// Minutes without keyboard or mouse input before a session ends by itself. The time away
    /// isn't counted. 0 turns this off, as it is for new installs: stepping away for a drink
    /// shouldn't end a session. Installs from before then keep 15 (store.rs).
    pub idle_minutes: u32,
    /// Whether a Medium or Hard session closes a browser that runs without the extension
    /// (and Hard counts the session as broken). Off, it only warns. Opt-in, so the app and
    /// the extension can each be used on their own.
    pub close_browsers: bool,
    /// Whether the sky in the window animates at 30 frames a second instead of 60, to save
    /// battery. It pauses whenever the window isn't focused either way.
    pub battery_saver: bool,
    /// The color of the frames' corner ticks and the tabs' dot, by its id in the core package's
    /// `glowColors`. Colors past the first come with lifetime focus hours; the app shows the
    /// default if this one isn't unlocked (or isn't known).
    pub glow: String,
    /// The hotkey that opens the satellite box from anywhere, like "Alt+KeyP" (see hotkey.rs).
    /// Empty turns it off. It only changes once the new one is registered, through its own
    /// command (`set_satellite_hotkey`).
    pub satellite_hotkey: String,
    /// Whether sessions also block their restricted sites for every browser and app, without
    /// the extension, through the hosts file (blocker.rs). macOS only for now. Turning it on
    /// installs a small helper, which asks for an administrator's password once.
    pub block_without_extension: bool,
    /// Whether the app checks once a day for a new version (updates.rs). A check is one request
    /// to GitHub; nothing else leaves the computer.
    pub check_for_updates: bool,
}

pub const MAX_PASSES_PER_WEEK: u32 = 14;
pub const MAX_IDLE_MINUTES: u32 = 120;
pub const DEFAULT_GLOW: &str = "starlight";

impl Default for Settings {
    fn default() -> Self {
        Self {
            passes_per_week: 2,
            idle_minutes: 0,
            close_browsers: false,
            battery_saver: false,
            glow: DEFAULT_GLOW.into(),
            satellite_hotkey: hotkey::DEFAULT_HOTKEY.into(),
            block_without_extension: false,
            check_for_updates: true,
        }
    }
}

impl Settings {
    /// The same settings with numbers kept in range, for values that come from the UI.
    pub fn clamped(mut self) -> Self {
        self.passes_per_week = self.passes_per_week.min(MAX_PASSES_PER_WEEK);
        self.idle_minutes = self.idle_minutes.min(MAX_IDLE_MINUTES);
        // A glow is only ever a short id like "aurora", so anything else goes back to the default.
        let id_like = |c: char| c.is_ascii_lowercase() || c == '-';
        if self.glow.is_empty() || self.glow.len() > 32 || !self.glow.chars().all(id_like) {
            self.glow = DEFAULT_GLOW.into();
        }
        if !self.satellite_hotkey.is_empty() {
            self.satellite_hotkey = match hotkey::parse(&self.satellite_hotkey) {
                Ok(shortcut) => hotkey::text(&shortcut),
                Err(_) => hotkey::DEFAULT_HOTKEY.into(),
            };
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_saved_before_battery_saver_load_with_it_off() {
        let saved: Settings =
            serde_json::from_str(r#"{"passesPerWeek":3,"idleMinutes":10,"closeBrowsers":true}"#).unwrap();
        assert_eq!(
            saved,
            Settings {
                passes_per_week: 3,
                idle_minutes: 10,
                close_browsers: true,
                battery_saver: false,
                glow: DEFAULT_GLOW.into(),
                satellite_hotkey: hotkey::DEFAULT_HOTKEY.into(),
                block_without_extension: false,
                check_for_updates: true,
            }
        );
    }

    #[test]
    fn a_glow_that_isnt_an_id_goes_back_to_the_default() {
        for glow in ["", "Aurora", "red; color: blue", &"a".repeat(33)] {
            let settings = Settings {
                glow: glow.into(),
                ..Settings::default()
            };
            assert_eq!(settings.clamped().glow, DEFAULT_GLOW, "{glow:?}");
        }
        let aurora = Settings {
            glow: "aurora".into(),
            ..Settings::default()
        };
        assert_eq!(aurora.clamped().glow, "aurora");
    }

    #[test]
    fn a_hotkey_that_cant_be_used_goes_back_to_the_default() {
        let with = |hotkey: &str| Settings {
            satellite_hotkey: hotkey.into(),
            ..Settings::default()
        };
        assert_eq!(with("Super+KeyC").clamped().satellite_hotkey, hotkey::DEFAULT_HOTKEY);
        assert_eq!(with("nonsense").clamped().satellite_hotkey, hotkey::DEFAULT_HOTKEY);
        assert_eq!(with("super+shift+KeyK").clamped().satellite_hotkey, "Shift+Super+KeyK");
        assert_eq!(with("").clamped().satellite_hotkey, "", "off stays off");
    }
}
