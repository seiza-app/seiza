use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::apps::{self, AppRule};
use crate::backup::{Backup, History, ImportReport};
use crate::browsers::KnownBrowser;
use crate::error::{Error, Result};
use crate::quotes::{MAX_OWN_QUOTES, OwnQuote};
use crate::records::Record;
use crate::satellites::{self, DONE_TASKS_SHOWN_MS, Satellite, SatelliteAction, SatelliteRecord, SatelliteState};
use crate::session::{
    EndReason, GateVisit, MAX_GOAL_LENGTH, Profile, Session, SessionGoal, SessionRecord, is_star_color,
    starter_profiles, truncate,
};
use crate::settings::Settings;
use crate::sites::{self, SiteRule, SiteSettings};
use crate::store::{Store, Synced};
use crate::sync::settings::SyncSettings;
use crate::timer::{Strictness, TimerSettings};

const PROFILES: &str = "profiles";
const ACTIVE_SESSION: &str = "activeSession";
const SETTINGS: &str = "settings";
const BROWSERS: &str = "browsers";
const SKY: &str = "sky";
const TOUR: &str = "tour";
const UPDATE_OFFERED: &str = "updateOffered";
const OWN_QUOTES: &str = "ownQuotes";
const SESSION_GOAL: &str = "sessionGoal";
const PROFILE_STAMPS: &str = "profileStamps";
const SITE_SETTINGS: &str = "siteSettings";
/// This device's sync settings (sync/settings.rs), kept out of backups.
const SYNC: &str = "sync";

/// Emergency passes come back a week after they're used.
const PASS_WEEK_MS: i64 = 7 * 24 * 60 * 60_000;

/// The session engine: the source of truth for profiles and the running session. It knows
/// nothing about windows or the menu bar; the app reacts to its changes.
pub struct Engine {
    store: Store,
    profiles: Synced<Vec<Profile>>,
    /// Profiles that were deleted, as they last were, so their stars keep their color and a
    /// session running on one keeps its name.
    deleted_profiles: Vec<Profile>,
    /// When each profile last changed in the list, by the list's `updated_at`, deleted ones
    /// included. A list from the extension made before a change here can't undo it.
    profile_stamps: BTreeMap<String, i64>,
    session: Synced<Option<Session>>,
    settings: Settings,
    /// When each emergency pass was used (epoch ms).
    pass_uses: Vec<i64>,
    /// The browsers that have linked to the app.
    browsers: Vec<KnownBrowser>,
    /// Satellites in orbit, back or on the task list, and recently ticked-off tasks, oldest
    /// first.
    satellites: Vec<Satellite>,
    /// Whether the tour for new users has been offered (taken or turned down).
    tour_offered: bool,
    /// The newest version of the app the Sky has offered (updates.rs).
    update_offered: Option<String>,
    /// Quotes the user added, synced with the extension.
    own_quotes: Synced<Vec<OwnQuote>>,
    /// The running session's goal, or the last one's until "Did you do it?" is answered.
    goal: Option<SessionGoal>,
    /// What's set for sites outside any profile in the extension (hidden parts, taming all the
    /// time), synced with it.
    site_settings: Synced<Vec<SiteSettings>>,
}

/// Everything the UI shows, sent whenever it changes.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Snapshot {
    pub profiles: Vec<ProfileSummary>,
    /// The star color each deleted profile had, if it had one picked, so its stars keep it.
    pub deleted_star_colors: BTreeMap<String, String>,
    pub session: Option<ActiveSession>,
    pub settings: Settings,
    /// Emergency passes left for the last 7 days.
    pub passes_left: u32,
    /// The browsers that have linked to the app, which are watched during sessions.
    pub browsers: Vec<KnownBrowser>,
    /// Whether this system can hide another app's windows, for app rules.
    pub can_hide_apps: bool,
    /// Satellites in orbit, back or on the task list, and tasks ticked off in the last day,
    /// oldest first.
    pub satellites: Vec<Satellite>,
    /// Whether the tour for new users has been offered. Until it has, the Sky offers it once;
    /// after that it's only in Settings.
    pub tour_offered: bool,
    /// The newest version of the app the Sky has offered. The Sky offers each new version once;
    /// after that it's in Settings until it's installed.
    pub update_offered: Option<String>,
    /// Quotes the user added, for the Sky's quote for the day and Settings.
    pub own_quotes: Vec<OwnQuote>,
    /// The last session's goal, between sessions, until "Did you do it?" is answered.
    pub goal_to_answer: Option<SessionGoal>,
}

/// Everything logged between two times, for the weekly report. The window works the report
/// out from it (packages/core/src/insights.ts), since it knows the local time zone.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Activity {
    /// The sessions that started in that time, oldest first.
    pub sessions: Vec<SessionRecord>,
    pub gate_visits: Vec<GateVisit>,
    /// The satellites launched in that time, and what became of them.
    pub satellites: Vec<SatelliteRecord>,
    /// When anything was first logged (epoch ms), so the report knows how far back to go.
    pub first_logged_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub timer: TimerSettings,
    pub apps: Vec<AppRule>,
    /// How it treats each site, set here or in the extension.
    pub sites: Vec<SiteRule>,
    /// The color of its stars, if it was given one.
    pub star_color: Option<String>,
    /// Whether starting one of its sessions in the window asks for a goal.
    pub ask_goal: bool,
    /// The if-then plan its redirect page shows, or "" if it has none.
    pub plan: String,
}

/// A name someone gave a constellation (PLAN §5). The constellations themselves are worked out
/// from the session log in the UI; `id` is the id of its first star, which is a session's id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConstellationName {
    pub id: String,
    pub name: String,
}

/// What the night sky keeps for itself (PLAN §5). Rare events are worked out from the session
/// log and this device's secret, so they can't be predicted from the log alone, and `seen_at` is
/// when the Sky last showed new ones, so each plays once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkyState {
    pub secret: String,
    /// Epoch milliseconds.
    pub seen_at: i64,
}

/// The longest name a constellation can have, in characters (as in packages/core).
const MAX_CONSTELLATION_NAME: usize = 40;

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActiveSession {
    pub id: String,
    pub profile_id: String,
    pub profile_name: String,
    /// Epoch milliseconds.
    pub started_at: i64,
    pub timer: Option<TimerSettings>,
    /// What it's for, if its profile asked and a goal was given.
    pub goal: Option<String>,
}

impl Engine {
    /// Loads saved state. A new install starts with the starter profiles, marked as never
    /// changed so the extension's own profiles win when it first connects.
    pub fn load(store: Store, now: i64) -> Result<Self> {
        let profiles = store.load(PROFILES)?.unwrap_or_else(|| Synced {
            value: starter_profiles(),
            updated_at: 0,
        });
        let session = store.load(ACTIVE_SESSION)?.unwrap_or(Synced {
            value: None,
            updated_at: 0,
        });
        let settings = store
            .load(SETTINGS)?
            .map(|s: Synced<Settings>| s.value)
            .unwrap_or_default();
        let pass_uses = store.pass_uses()?;
        let browsers = store.load(BROWSERS)?.map(|s| s.value).unwrap_or_default();
        let satellites = store.shown_satellites(now - DONE_TASKS_SHOWN_MS)?;
        let tour_offered = store.load::<bool>(TOUR)?.is_some_and(|s| s.value);
        let update_offered = store.load::<String>(UPDATE_OFFERED)?.map(|s| s.value);
        // Like the starter profiles, none at first counts as never changed, so quotes the
        // extension already has win when it first connects.
        let own_quotes = store.load(OWN_QUOTES)?.unwrap_or(Synced {
            value: vec![],
            updated_at: 0,
        });
        let goal = store.load(SESSION_GOAL)?.and_then(|s| s.value);
        // None at first counts as never changed, so the extension's own settings win.
        let site_settings = store.load(SITE_SETTINGS)?.unwrap_or(Synced {
            value: vec![],
            updated_at: 0,
        });
        let deleted_profiles = deleted_profiles(&store)?;
        // Saved before these were kept, every profile counts as changed when the list last did.
        let profile_stamps = store.load(PROFILE_STAMPS)?.map_or_else(
            || {
                profiles
                    .value
                    .iter()
                    .map(|p| (p.id.clone(), profiles.updated_at))
                    .collect()
            },
            |s| s.value,
        );
        Ok(Self {
            store,
            profiles,
            deleted_profiles,
            profile_stamps,
            session,
            settings,
            pass_uses,
            browsers,
            satellites,
            tour_offered,
            update_offered,
            own_quotes,
            goal,
            site_settings,
        })
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.value.as_ref()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The apps the profile with this id closes or hides during its sessions.
    pub fn profile_apps(&self, id: &str) -> Vec<AppRule> {
        self.profile(id).map(Profile::apps).unwrap_or_default()
    }

    /// Whether the profile with this id tames any sites, so its sessions need the extension.
    pub fn profile_tames_sites(&self, id: &str) -> bool {
        self.profile(id).is_some_and(Profile::tames_sites)
    }

    pub fn browsers(&self) -> &[KnownBrowser] {
        &self.browsers
    }

    fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.value.iter().find(|p| p.id == id)
    }

    /// Emergency passes left at `now`. Each one used comes back a week later.
    pub fn passes_left(&self, now: i64) -> u32 {
        let used = self.pass_uses.iter().filter(|&&at| at > now - PASS_WEEK_MS).count();
        self.settings.passes_per_week.saturating_sub(used as u32)
    }

    pub fn snapshot(&self, now: i64) -> Snapshot {
        Snapshot {
            profiles: self
                .profiles
                .value
                .iter()
                .map(|p| ProfileSummary {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    timer: p.timer(),
                    apps: p.apps(),
                    sites: p.sites(),
                    star_color: p.star_color(),
                    ask_goal: p.asks_goal(),
                    plan: p.plan(),
                })
                .collect(),
            deleted_star_colors: self
                .deleted_profiles
                .iter()
                .filter_map(|p| Some((p.id.clone(), p.star_color()?)))
                .collect(),
            session: self.session().map(|s| ActiveSession {
                id: s.id.clone(),
                profile_id: s.profile_id.clone(),
                profile_name: self.profile_name(&s.profile_id),
                started_at: s.started_at,
                timer: s.timer.clone(),
                goal: self
                    .goal
                    .as_ref()
                    .filter(|g| g.session_id == s.id)
                    .map(|g| g.text.clone()),
            }),
            settings: self.settings.clone(),
            passes_left: self.passes_left(now),
            browsers: self.browsers.clone(),
            can_hide_apps: cfg!(target_os = "macos"),
            satellites: self
                .satellites
                .iter()
                .filter(|s| s.state != SatelliteState::Done || s.state_at >= now - DONE_TASKS_SHOWN_MS)
                .cloned()
                .collect(),
            tour_offered: self.tour_offered,
            update_offered: self.update_offered.clone(),
            own_quotes: self.own_quotes.value.clone(),
            goal_to_answer: self.goal.clone().filter(|_| self.session().is_none()),
        }
    }

    /// A profile's name, or the last one it had if it's been deleted.
    pub fn profile_name(&self, id: &str) -> String {
        self.profile(id)
            .or_else(|| self.deleted_profiles.iter().find(|p| p.id == id))
            .map_or_else(|| "Unknown profile".into(), |p| p.name.clone())
    }

    pub fn start_session(&mut self, profile_id: &str, now: i64) -> Result<&Session> {
        if self.session.value.is_some() {
            return Err(Error::SessionRunning);
        }
        let profile = self.profile(profile_id).ok_or(Error::UnknownProfile)?;
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            profile_id: profile.id.clone(),
            started_at: now,
            timer: Some(profile.timer()),
            rest: Default::default(),
        };
        let synced = Synced {
            value: Some(session),
            updated_at: now,
        };
        self.store.save(ACTIVE_SESSION, &synced)?;
        self.session = synced;
        // Whatever the last session's goal was, it's no longer waiting for an answer.
        self.save_goal(None, now)?;
        Ok(self.session().expect("just started"))
    }

    /// Ends the running session and logs it. A Hard session can only end before its timer is
    /// done with an emergency pass; a pass isn't spent on any other session.
    ///
    /// Gentle and Medium sessions ask for their pause or typing in the window. They're there to
    /// make you stop and think, not to stop you, so this doesn't check for them.
    pub fn end_session(&mut self, now: i64, reason: EndReason) -> Result<SessionRecord> {
        let session = self.session().ok_or(Error::NoSession)?;
        let hard = session.lock(now) == Some(Strictness::Hard);
        let reason = match reason {
            EndReason::Stopped if hard => return Err(Error::Locked),
            EndReason::EmergencyPass if !hard => EndReason::Stopped,
            EndReason::EmergencyPass if self.passes_left(now) == 0 => return Err(Error::NoPassesLeft),
            reason => reason,
        };
        let mut record = SessionRecord::new(session, &self.profile_name(&session.profile_id), now, reason);
        let none = Synced {
            value: None,
            updated_at: now,
        };
        self.log_end(&mut record, none)?;
        if reason == EndReason::EmergencyPass {
            self.pass_uses.push(now);
        }
        Ok(record)
    }

    /// Ends the session for being idle, if it's still the one with this id. Only the time up
    /// to the last input counts. Strictness doesn't apply: there's no one there to keep in.
    pub fn end_idle_session(&mut self, session_id: &str, last_input: i64, now: i64) -> Result<Option<SessionRecord>> {
        let Some(session) = self.session().filter(|s| s.id == session_id) else {
            return Ok(None);
        };
        let mut record = SessionRecord::idle(session, &self.profile_name(&session.profile_id), last_input, now);
        let none = Synced {
            value: None,
            updated_at: now,
        };
        self.log_end(&mut record, none)?;
        Ok(Some(record))
    }

    /// Changes the app-wide settings. While a Hard session is running, passes per week can
    /// only go down and closing browsers can't be turned off, or they'd be easy ways out.
    pub fn set_settings(&mut self, settings: Settings, now: i64) -> Result<()> {
        let mut settings = settings.clamped();
        let hard = self.session().and_then(|s| s.lock(now)) == Some(Strictness::Hard);
        let looser = settings.passes_per_week > self.settings.passes_per_week
            || (self.settings.close_browsers && !settings.close_browsers)
            || (self.settings.block_without_extension && !settings.block_without_extension);
        if hard && looser {
            return Err(Error::Locked);
        }
        // The hotkey has its own command, since it changes only once it's registered.
        settings.satellite_hotkey = self.settings.satellite_hotkey.clone();
        self.save_settings(settings, now)
    }

    /// Saves the satellite box's hotkey, once capture.rs has registered it.
    pub fn set_satellite_hotkey(&mut self, hotkey: String, now: i64) -> Result<()> {
        let settings = Settings {
            satellite_hotkey: hotkey,
            ..self.settings.clone()
        };
        self.save_settings(settings.clamped(), now)
    }

    fn save_settings(&mut self, settings: Settings, now: i64) -> Result<()> {
        self.store.save(
            SETTINGS,
            &Synced {
                value: &settings,
                updated_at: now,
            },
        )?;
        self.settings = settings;
        Ok(())
    }

    /// Remembers a browser that linked to the app, so it's watched during sessions. Returns
    /// whether it's new (or renamed).
    pub fn remember_browser(&mut self, browser: KnownBrowser, now: i64) -> Result<bool> {
        if self.browsers.contains(&browser) {
            return Ok(false);
        }
        let mut browsers = self.browsers.clone();
        browsers.retain(|b| b.path != browser.path);
        browsers.push(browser);
        self.save_browsers(browsers, now)?;
        Ok(true)
    }

    /// Stops watching a browser, e.g. one that's no longer used. Only between sessions, or
    /// forgetting it would be a way around the session.
    pub fn forget_browser(&mut self, path: &str, now: i64) -> Result<()> {
        if self.session().is_some() {
            return Err(Error::SessionRunning);
        }
        let mut browsers = self.browsers.clone();
        browsers.retain(|b| b.path != path);
        self.save_browsers(browsers, now)
    }

    fn save_browsers(&mut self, browsers: Vec<KnownBrowser>, now: i64) -> Result<()> {
        self.store.save(
            BROWSERS,
            &Synced {
                value: &browsers,
                updated_at: now,
            },
        )?;
        self.browsers = browsers;
        Ok(())
    }

    /// Saves a new version of the list of profiles made here, and notes what changed for sync.
    fn save_profiles(&mut self, profiles: Synced<Vec<Profile>>, now: i64) -> Result<()> {
        self.replace_profiles(profiles, Some(now))
    }

    /// Saves a new version of the list of profiles, with when each one changed in it. Changes
    /// made here are noted for sync as made at `changed_here_at`; ones from sync have none, so
    /// they aren't sent back.
    fn replace_profiles(&mut self, profiles: Synced<Vec<Profile>>, changed_here_at: Option<i64>) -> Result<()> {
        let ids = |list: &[Profile]| list.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        let added_or_deleted = ids(&self.profiles.value) != ids(&profiles.value);
        let mut stamps = self.profile_stamps.clone();
        restamp(&mut stamps, &self.profiles.value, &profiles.value, profiles.updated_at);
        let stamps = Synced {
            value: stamps,
            updated_at: profiles.updated_at,
        };
        self.store.save_profiles(
            (PROFILES, &profiles),
            (PROFILE_STAMPS, &stamps),
            changed_here_at.map(|now| (&self.profiles.value[..], now)),
        )?;
        self.profiles = profiles;
        self.profile_stamps = stamps.value;
        if added_or_deleted {
            self.deleted_profiles = deleted_profiles(&self.store)?;
        }
        Ok(())
    }

    /// Changes the apps a profile closes or hides. While one of its Hard sessions is running,
    /// they can only get stricter.
    pub fn set_profile_apps(&mut self, profile_id: &str, apps: Vec<AppRule>, now: i64) -> Result<()> {
        let mut apps = apps;
        apps.truncate(apps::MAX_RULES);
        let mut profiles = self.profiles.value.clone();
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or(Error::UnknownProfile)?;
        let running = self.session().filter(|s| s.profile_id == profile_id);
        let hard = running.and_then(|s| s.lock(now)) == Some(Strictness::Hard);
        if hard && apps::looser(&profile.apps(), &apps) {
            return Err(Error::Locked);
        }
        profile.set_apps(&apps);
        self.save_profiles(
            Synced {
                value: profiles,
                updated_at: now,
            },
            now,
        )
    }

    /// Changes how a profile treats sites. While one of its Hard sessions is running, they can
    /// only get stricter.
    pub fn set_profile_sites(&mut self, profile_id: &str, sites: Vec<SiteRule>, now: i64) -> Result<()> {
        let sites = sites::tidy(sites);
        let mut profiles = self.profiles.value.clone();
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or(Error::UnknownProfile)?;
        let running = self.session().filter(|s| s.profile_id == profile_id);
        let hard = running.and_then(|s| s.lock(now)) == Some(Strictness::Hard);
        if hard && sites::looser(&profile.sites(), &sites) {
            return Err(Error::Locked);
        }
        profile.set_sites(&sites);
        self.save_profiles(
            Synced {
                value: profiles,
                updated_at: now,
            },
            now,
        )
    }

    /// Changes a profile's timer, for every future session that uses it.
    pub fn set_profile_timer(&mut self, profile_id: &str, timer: TimerSettings, now: i64) -> Result<()> {
        let mut profiles = self.profiles.value.clone();
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or(Error::UnknownProfile)?;
        profile.set_timer(&timer.clamped());
        self.save_profiles(
            Synced {
                value: profiles,
                updated_at: now,
            },
            now,
        )
    }

    /// Sets the color of a profile's stars, a hex color like "#f5c86b".
    pub fn set_profile_star_color(&mut self, profile_id: &str, color: &str, now: i64) -> Result<()> {
        if !is_star_color(color) {
            return Err(Error::InvalidStarColor);
        }
        let mut profiles = self.profiles.value.clone();
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or(Error::UnknownProfile)?;
        profile.set_star_color(color);
        self.save_profiles(
            Synced {
                value: profiles,
                updated_at: now,
            },
            now,
        )
    }

    /// Changes the if-then plan a profile's redirect page shows (PLAN §3). It can't loosen a
    /// Hard session, so it can be changed any time.
    pub fn set_profile_plan(&mut self, profile_id: &str, plan: &str, now: i64) -> Result<()> {
        let mut profiles = self.profiles.value.clone();
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or(Error::UnknownProfile)?;
        profile.set_plan(plan);
        self.save_profiles(
            Synced {
                value: profiles,
                updated_at: now,
            },
            now,
        )
    }

    /// Turns asking for a goal at the start of a profile's sessions on or off.
    pub fn set_profile_asks_goal(&mut self, profile_id: &str, on: bool, now: i64) -> Result<()> {
        let mut profiles = self.profiles.value.clone();
        let profile = profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or(Error::UnknownProfile)?;
        profile.set_asks_goal(on);
        self.save_profiles(
            Synced {
                value: profiles,
                updated_at: now,
            },
            now,
        )
    }

    /// Sets the running session's goal. An empty one clears it.
    pub fn set_session_goal(&mut self, text: &str, now: i64) -> Result<()> {
        let session = self.session().ok_or(Error::NoSession)?;
        let goal = SessionGoal::new(session, text);
        self.save_goal(goal, now)
    }

    /// Answers "Did you do it?" for the last session's goal, which then stops waiting.
    pub fn answer_goal(&mut self, session_id: &str, met: bool, now: i64) -> Result<()> {
        let waiting = self.session().is_none() && self.goal.as_ref().is_some_and(|g| g.session_id == session_id);
        if !waiting {
            return Ok(());
        }
        self.store.set_goal_met(session_id, met, now)?;
        self.save_goal(None, now)
    }

    fn save_goal(&mut self, goal: Option<SessionGoal>, now: i64) -> Result<()> {
        if goal == self.goal {
            return Ok(());
        }
        self.store.save(
            SESSION_GOAL,
            &Synced {
                value: &goal,
                updated_at: now,
            },
        )?;
        self.goal = goal;
        Ok(())
    }

    /// The profiles and session with when they last changed, for sending to the extension.
    pub fn synced(&self) -> (&Synced<Vec<Profile>>, &Synced<Option<Session>>) {
        (&self.profiles, &self.session)
    }

    /// The sites' settings outside any profile, with when they last changed, for the extension.
    pub fn site_settings(&self) -> &Synced<Vec<SiteSettings>> {
        &self.site_settings
    }

    /// Takes the extension's site settings if they're newer than the app's. Returns whether they
    /// changed. The extension makes sure a Hard session never shows a hidden part again.
    pub fn apply_remote_site_settings(&mut self, incoming: Synced<Vec<SiteSettings>>, now: i64) -> Result<bool> {
        let updated_at = incoming.updated_at.min(now);
        if updated_at <= self.site_settings.updated_at {
            return Ok(false);
        }
        let synced = Synced {
            value: sites::tidy_settings(incoming.value),
            updated_at,
        };
        self.store.save(SITE_SETTINGS, &synced)?;
        self.site_settings = synced;
        Ok(true)
    }

    /// Adds a quote of the user's own. Returns whether there was a quote to add.
    pub fn add_own_quote(&mut self, text: &str, author: &str, now: i64) -> Result<bool> {
        let Some(quote) = OwnQuote::new(text, author) else {
            return Ok(false);
        };
        if self.own_quotes.value.len() >= MAX_OWN_QUOTES {
            return Err(Error::TooManyQuotes);
        }
        let mut list = self.own_quotes.value.clone();
        list.push(quote);
        self.save_quotes(list, now)?;
        Ok(true)
    }

    pub fn remove_own_quote(&mut self, id: &str, now: i64) -> Result<()> {
        let mut list = self.own_quotes.value.clone();
        list.retain(|quote| quote.id != id);
        if list.len() != self.own_quotes.value.len() {
            self.save_quotes(list, now)?;
        }
        Ok(())
    }

    fn save_quotes(&mut self, value: Vec<OwnQuote>, now: i64) -> Result<()> {
        // Always newer than the last change, even if the clock went back.
        let synced = Synced {
            value,
            updated_at: now.max(self.own_quotes.updated_at + 1),
        };
        self.store.save(OWN_QUOTES, &synced)?;
        self.own_quotes = synced;
        Ok(())
    }

    /// Takes the extension's profiles and session where they're newer than the app's.
    /// Returns whether anything changed.
    ///
    /// A session that started in the extension gets its profile's timer here. After that the
    /// app owns the timer, so the extension can't change it. A session the extension ended
    /// (or replaced) is logged as ending when the extension changed it, unless it's a Hard
    /// session whose timer is still running: that one carries on, and goes back to the browser.
    pub fn apply_remote(
        &mut self,
        profiles: Option<Synced<Vec<Profile>>>,
        profiles_based_on: Option<i64>,
        session: Option<Synced<Option<Session>>>,
        now: i64,
    ) -> Result<bool> {
        let mut changed = false;

        // Nothing can come from the future; a clock that ran ahead mustn't freeze the app's state.
        if let Some(mut incoming) = profiles.filter(|p| p.updated_at.min(now) > self.profiles.updated_at) {
            incoming.updated_at = incoming.updated_at.min(now);
            if let Some(based_on) = profiles_based_on {
                let merged =
                    merge_from_extension(&self.profiles.value, &self.profile_stamps, &incoming.value, based_on);
                if merged != incoming.value {
                    incoming.value = merged;
                    // Newer than the extension's list, so it takes back what it was missing.
                    incoming.updated_at += 1;
                }
            }
            self.save_profiles(incoming, now)?;
            changed = true;
        }

        if let Some(mut incoming) = session.filter(|s| s.updated_at.min(now) > self.session.updated_at) {
            incoming.updated_at = incoming.updated_at.min(now);
            let current = self.session().cloned();
            let same = matches!((&current, &incoming.value), (Some(c), Some(n)) if c.id == n.id);

            if !same && current.as_ref().and_then(|s| s.lock(now)) == Some(Strictness::Hard) {
                // Newer than the browser's change, so the browser takes the session back.
                self.session.updated_at = incoming.updated_at + 1;
                self.store.save(ACTIVE_SESSION, &self.session)?;
                return Ok(true);
            }

            if let Some(new) = incoming.value.as_mut() {
                let timer = match &current {
                    Some(c) if same && c.timer.is_some() => c.timer.clone(),
                    _ => new
                        .timer
                        .clone()
                        .or_else(|| Some(self.profile(&new.profile_id).map(Profile::timer).unwrap_or_default())),
                };
                if new.timer != timer {
                    new.timer = timer;
                    // Newer than the extension's copy, so it takes the timer when this is sent back.
                    incoming.updated_at += 1;
                }
            }
            let ended = match (&self.session.value, &incoming.value) {
                (Some(previous), Some(new)) if previous.id == new.id => None,
                (previous, _) => previous.as_ref(),
            };
            match ended {
                Some(previous) => {
                    let name = self.profile_name(&previous.profile_id);
                    let ended_at = incoming.updated_at.min(now);
                    let mut record = SessionRecord::new(previous, &name, ended_at, EndReason::Stopped);
                    self.log_end(&mut record, incoming)?;
                }
                None => {
                    self.store.save(ACTIVE_SESSION, &incoming)?;
                    self.session = incoming;
                }
            }
            // A session the extension started has no goal, and the last one's no longer waits.
            let new_session = self.session().map(|s| s.id.clone());
            if new_session.is_some_and(|id| self.goal.as_ref().is_some_and(|g| g.session_id != id)) {
                self.save_goal(None, now)?;
            }
            changed = true;
        }

        Ok(changed)
    }

    /// Logs a session that ended and takes on what replaced it (no session, or one the
    /// extension started). Satellites in orbit come back.
    fn log_end(&mut self, record: &mut SessionRecord, replacement: Synced<Option<Session>>) -> Result<()> {
        record.goal = self
            .goal
            .as_ref()
            .filter(|g| g.session_id == record.id)
            .map(|g| g.text.clone());
        self.store.end_session(record, ACTIVE_SESSION, &replacement)?;
        self.session = replacement;
        for satellite in &mut self.satellites {
            if satellite.state == SatelliteState::Orbiting {
                satellite.state = SatelliteState::Back;
                satellite.state_at = record.ended_at;
            }
        }
        Ok(())
    }

    /// Launches a thought into orbit, to come back when the running session ends, or the next
    /// one if none is running. Returns whether there was anything to launch.
    pub fn launch_satellite(&mut self, text: &str, now: i64) -> Result<bool> {
        let Some(text) = satellites::cleaned(text) else {
            return Ok(false);
        };
        let satellite = Satellite {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            launched_at: now,
            state: SatelliteState::Orbiting,
            state_at: now,
        };
        let session_id = self.session().map(|s| s.id.as_str());
        self.store.save_satellite(&satellite, session_id)?;
        self.satellites.push(satellite);
        Ok(true)
    }

    /// Ticks off, snoozes, keeps as a task or deletes a satellite. A ticked-off task can go
    /// back on the list for a day afterwards.
    pub fn act_on_satellite(&mut self, id: &str, action: SatelliteAction, now: i64) -> Result<()> {
        let satellite = self
            .satellites
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(Error::UnknownSatellite)?;
        let state = action.state();
        let was_task = satellite.state == SatelliteState::Task;
        self.store.move_satellite(id, state, now)?;
        satellite.state = state;
        satellite.state_at = now;
        // Only tasks stay once ticked off, and only for a day.
        let kept = |s: &Satellite| match s.state {
            SatelliteState::Done if s.id == id => was_task,
            SatelliteState::Done => s.state_at >= now - DONE_TASKS_SHOWN_MS,
            SatelliteState::Deleted => false,
            _ => true,
        };
        self.satellites.retain(kept);
        Ok(())
    }

    /// Logs a trip to a redirected site, with the session it happened in.
    pub fn record_gate_visit(&self, visit: GateVisit, now: i64) -> Result<()> {
        let visit = visit.trimmed();
        let session_id = self
            .session()
            .filter(|s| visit.at >= s.started_at)
            .map(|s| s.id.as_str());
        self.store.save_gate_visit(&visit, session_id, now)
    }

    /// Up to `limit` records changed here since `cursor`, and the cursor for next time, for
    /// sync to send (records.rs).
    #[allow(dead_code)]
    pub fn changed_records(&self, cursor: i64, limit: usize) -> Result<(Vec<Record>, i64)> {
        self.store.changed_since(cursor, limit)
    }

    /// Saves records sync fetched from other devices, where they win over the copy here.
    /// Emergency passes used elsewhere count here too, since they're in the log. Returns how
    /// many changed something.
    #[allow(dead_code)]
    pub fn apply_records(&mut self, records: &[Record], now: i64) -> Result<usize> {
        let applied = self.store.apply(records)?;
        if applied > 0 {
            self.pass_uses = self.store.pass_uses()?;
            if records.iter().any(|r| matches!(r, Record::Profile(_))) {
                self.take_synced_profiles(now)?;
            }
        }
        Ok(applied)
    }

    /// Brings the list of profiles in line with sync's copy of each one: edited ones change in
    /// place, deleted ones go, and new ones go on the end. Newer than the extension's list, so
    /// it takes them on too. These aren't changes made here, so they aren't sent back.
    fn take_synced_profiles(&mut self, now: i64) -> Result<()> {
        let known = self.store.known_profiles()?;
        let synced = |id: &str| known.iter().find(|(p, _)| p.id == id);
        let mut profiles: Vec<Profile> = self
            .profiles
            .value
            .iter()
            .filter_map(|here| match synced(&here.id) {
                Some((_, true)) => None,
                Some((there, false)) => Some(there.clone()),
                None => Some(here.clone()),
            })
            .collect();
        for (there, deleted) in &known {
            if !deleted && !profiles.iter().any(|p| p.id == there.id) {
                profiles.push(there.clone());
            }
        }
        self.deleted_profiles = deleted_profiles(&self.store)?;
        if profiles != self.profiles.value {
            let synced = Synced {
                value: profiles,
                updated_at: now.max(self.profiles.updated_at + 1),
            };
            self.replace_profiles(synced, None)?;
        }
        Ok(())
    }

    /// Marks every record to be sent again, for moving to another server or starting a new
    /// group.
    pub fn resend_all_records(&mut self) -> Result<()> {
        self.store.resend_all()
    }

    /// The session log, oldest first.
    pub fn session_log(&self) -> Result<Vec<SessionRecord>> {
        self.store.sessions()
    }

    /// The names people gave constellations.
    pub fn constellation_names(&self) -> Result<Vec<ConstellationName>> {
        let names = self.store.constellation_names()?;
        Ok(names
            .into_iter()
            .map(|(id, name)| ConstellationName { id, name })
            .collect())
    }

    /// Names the constellation whose first star is the session `id`, which may be the one
    /// running. A blank name goes back to the made-up one.
    pub fn name_constellation(&self, id: &str, name: &str, now: i64) -> Result<()> {
        let running = self.session().is_some_and(|s| s.id == id);
        if !running && !self.store.has_session(id)? {
            return Err(Error::UnknownConstellation);
        }
        self.store.name_constellation(id, cleaned_name(name).as_deref(), now)
    }

    /// The night sky's secret and when it last showed new things. The first time, it makes a new
    /// secret, and everything from before counts as seen, so it doesn't all play at once.
    pub fn sky_state(&self, now: i64) -> Result<SkyState> {
        if let Some(saved) = self.store.load::<SkyState>(SKY)? {
            return Ok(saved.value);
        }
        let state = SkyState {
            // Random from the system's secure source.
            secret: uuid::Uuid::new_v4().simple().to_string(),
            seen_at: now,
        };
        self.store.save(
            SKY,
            &Synced {
                value: state.clone(),
                updated_at: now,
            },
        )?;
        Ok(state)
    }

    /// Notes that the Sky has shown everything up to `at`. It never goes back, or past now.
    pub fn sky_seen(&self, at: i64, now: i64) -> Result<()> {
        let mut state = self.sky_state(now)?;
        state.seen_at = state.seen_at.max(at.min(now));
        self.store.save(
            SKY,
            &Synced {
                value: state,
                updated_at: now,
            },
        )
    }

    /// Takes the sync group's sky secret on joining it, so every device draws one sky. When the
    /// Sky last showed new things stays this device's, so the rare events and comets that are
    /// drawn again don't all play at once.
    pub fn match_sky(&self, secret: &str, now: i64) -> Result<()> {
        let mut state = self.sky_state(now)?;
        state.secret = secret.to_owned();
        self.store.save(
            SKY,
            &Synced {
                value: state,
                updated_at: now,
            },
        )
    }

    /// Whether this computer has logged sessions of its own (not sample ones), whose rare events
    /// would be drawn again on joining a sync group.
    pub fn has_own_sessions(&self) -> Result<bool> {
        Ok(self
            .store
            .sessions()?
            .iter()
            .any(|s| !s.id.starts_with(crate::samples::SAMPLE_PREFIX)))
    }

    /// This device's sync settings, while it syncs.
    pub fn sync_settings(&self) -> Result<Option<SyncSettings>> {
        Ok(self
            .store
            .load::<Option<SyncSettings>>(SYNC)?
            .and_then(|saved| saved.value))
    }

    /// Saves this device's sync settings, or `None` once it leaves sync.
    pub fn set_sync_settings(&self, settings: Option<&SyncSettings>, now: i64) -> Result<()> {
        self.store.save(
            SYNC,
            &Synced {
                value: settings,
                updated_at: now,
            },
        )
    }

    /// Notes that the tour for new users has been offered, so the Sky doesn't offer it again.
    pub fn tour_offered(&mut self, now: i64) -> Result<()> {
        self.store.save(
            TOUR,
            &Synced {
                value: true,
                updated_at: now,
            },
        )?;
        self.tour_offered = true;
        Ok(())
    }

    /// Notes that the Sky has offered this version of the app, so it doesn't offer it again.
    pub fn update_offered(&mut self, version: &str, now: i64) -> Result<()> {
        self.store.save(
            UPDATE_OFFERED,
            &Synced {
                value: version,
                updated_at: now,
            },
        )?;
        self.update_offered = Some(version.into());
        Ok(())
    }

    /// Trips to redirected sites that went back to focus instead of continuing, oldest first,
    /// for comets.
    pub fn turned_back(&self) -> Result<Vec<GateVisit>> {
        let visits = self.store.gate_visits()?;
        Ok(visits
            .into_iter()
            .map(|(_, visit)| visit)
            .filter(|visit| !visit.continued)
            .collect())
    }

    /// Adds six weeks of made-up sessions and gate visits for the current profiles, for trying
    /// the app in development builds.
    pub fn add_samples(&mut self, now: i64) -> Result<()> {
        let profiles: Vec<_> = self
            .profiles
            .value
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        let (sessions, visits) = crate::samples::sample_log(now, &profiles);
        self.store.add_samples(&sessions, &visits)
    }

    /// Removes all sample data, leaving real sessions and visits as they are.
    pub fn remove_samples(&self) -> Result<()> {
        self.store.remove_samples()
    }

    /// Everything logged from `from` up to (not including) `to`, for the weekly report.
    pub fn activity(&self, from: i64, to: i64) -> Result<Activity> {
        let gate_visits = self.store.gate_visits_between(from, to)?;
        Ok(Activity {
            sessions: self.store.sessions_between(from, to)?,
            gate_visits: gate_visits.into_iter().map(|(_, visit)| visit).collect(),
            satellites: self.store.satellites_launched_between(from, to)?,
            first_logged_at: self.store.first_logged_at()?,
        })
    }

    /// Everything the app keeps, for a backup file (backup.rs). This computer's browsers and
    /// whatever's running stay out.
    pub fn backup(&self, now: i64) -> Result<Backup> {
        let mut backup = Backup::new(now);
        backup.profiles = self.profiles.value.clone();
        backup.settings = Some(self.settings.clone());
        backup.own_quotes = self.own_quotes.value.clone();
        backup.site_settings = self.site_settings.value.clone();
        backup.sky = self.store.load::<SkyState>(SKY)?.map(|s| s.value);
        backup.history = self.store.history()?;
        Ok(backup)
    }

    /// Brings in a backup, adding to what's here and taking nothing away. A profile in the file
    /// replaces the one here with the same id, and the file's settings replace these, except the
    /// hotkey and blocking without the extension, which are set up on each computer. Only
    /// between sessions, so an import can't loosen a session or change its profile under it.
    pub fn import(&mut self, backup: Backup, now: i64) -> Result<ImportReport> {
        if self.session().is_some() {
            return Err(Error::SessionRunning);
        }
        let mut state = vec![];

        let mut profiles = self.profiles.value.clone();
        let (mut new_profiles, mut updated_profiles) = (0, 0);
        for profile in backup.profiles.into_iter().take(MAX_IMPORTED_PROFILES) {
            let Some(profile) = imported_profile(profile) else {
                continue;
            };
            match profiles.iter_mut().find(|p| p.id == profile.id) {
                Some(here) if *here == profile => {}
                Some(here) => {
                    *here = profile;
                    updated_profiles += 1;
                }
                None => {
                    profiles.push(profile);
                    new_profiles += 1;
                }
            }
        }
        // Newer than anything the extension has, so it takes them on too.
        let profiles = Synced {
            value: profiles,
            updated_at: now.max(self.profiles.updated_at + 1),
        };
        let mut profile_stamps = self.profile_stamps.clone();
        if new_profiles + updated_profiles > 0 {
            restamp(
                &mut profile_stamps,
                &self.profiles.value,
                &profiles.value,
                profiles.updated_at,
            );
            state.push((PROFILES, json(&profiles)?));
            state.push((
                PROFILE_STAMPS,
                json(&Synced {
                    value: &profile_stamps,
                    updated_at: profiles.updated_at,
                })?,
            ));
            self.store
                .note_profile_changes(&self.profiles.value, &profiles.value, now)?;
        }

        let mut quotes = self.own_quotes.value.clone();
        let mut new_quotes = 0;
        for quote in backup.own_quotes.into_iter().filter_map(OwnQuote::cleaned) {
            let known = quotes.iter().any(|q| q.id == quote.id || q.text == quote.text);
            if !known && quotes.len() < MAX_OWN_QUOTES {
                quotes.push(quote);
                new_quotes += 1;
            }
        }
        let quotes = Synced {
            value: quotes,
            updated_at: now.max(self.own_quotes.updated_at + 1),
        };
        if new_quotes > 0 {
            state.push((OWN_QUOTES, json(&quotes)?));
        }

        // Parts hidden in either are hidden; a site tamed all the time here keeps its toggles.
        let mut site_settings = self.site_settings.value.clone();
        let site_settings_changed = sites::merge_settings(&mut site_settings, backup.site_settings);
        let site_settings = Synced {
            value: site_settings,
            updated_at: now.max(self.site_settings.updated_at + 1),
        };
        if site_settings_changed {
            state.push((SITE_SETTINGS, json(&site_settings)?));
        }

        let settings = backup.settings.map_or_else(
            || self.settings.clone(),
            |settings| Settings {
                satellite_hotkey: self.settings.satellite_hotkey.clone(),
                block_without_extension: self.settings.block_without_extension,
                ..settings.clamped()
            },
        );
        if settings != self.settings {
            let synced = Synced {
                value: &settings,
                updated_at: now,
            };
            state.push((SETTINGS, json(&synced)?));
        }

        // The same log and secret draw the same sky, so a computer with nothing logged of its own
        // takes the backup's secret. One that has its own sky keeps it, so its stars don't change.
        // Either way, everything brought in counts as seen, so it doesn't all play at once.
        let here = self.store.load::<SkyState>(SKY)?.map(|s| s.value);
        let fresh = self
            .store
            .sessions()?
            .iter()
            .all(|s| s.id.starts_with(crate::samples::SAMPLE_PREFIX));
        let theirs = backup
            .sky
            .map(|sky| sky.secret)
            .filter(|secret| !secret.is_empty() && secret.len() <= 64);
        let secret = match (theirs, &here) {
            (Some(theirs), _) if fresh => theirs,
            (_, Some(here)) => here.secret.clone(),
            (theirs, None) => theirs.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
        };
        let sky = SkyState {
            secret,
            seen_at: here.map_or(now, |here| here.seen_at.max(now)),
        };
        state.push((
            SKY,
            json(&Synced {
                value: sky,
                updated_at: now,
            })?,
        ));

        let report = self.store.import(&imported_history(backup.history), &state)?;
        if new_profiles + updated_profiles > 0 {
            self.profiles = profiles;
            self.profile_stamps = profile_stamps;
        }
        if new_quotes > 0 {
            self.own_quotes = quotes;
        }
        if site_settings_changed {
            self.site_settings = site_settings;
        }
        self.settings = settings;
        self.pass_uses = self.store.pass_uses()?;
        self.satellites = self.store.shown_satellites(now - DONE_TASKS_SHOWN_MS)?;
        Ok(ImportReport {
            new_profiles,
            updated_profiles,
            quotes: new_quotes,
            ..report
        })
    }
}

/// The most profiles brought in from one backup.
const MAX_IMPORTED_PROFILES: usize = 100;

/// The longest profile name, as in packages/core (edits.ts).
const MAX_PROFILE_NAME: usize = 40;

/// The longest id kept from a backup. Seiza's own ids are far shorter.
const MAX_ID: usize = 100;

/// A profile from a backup with a sensible id and name, or None. The rest is kept as it is, as
/// it is from the extension; reading it skips anything that can't be right.
fn imported_profile(mut profile: Profile) -> Option<Profile> {
    let mut name = profile.name.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&mut name, MAX_PROFILE_NAME);
    if profile.id.is_empty() || profile.id.len() > MAX_ID || name.is_empty() {
        return None;
    }
    profile.name = name;
    if profile.rest.contains_key("starColor") && profile.star_color().is_none() {
        profile.rest.remove("starColor");
    }
    Some(profile)
}

/// A backup's history with its text cut down to the usual limits, and anything without a
/// sensible id left out.
fn imported_history(mut history: History) -> History {
    let usable = |id: &str| !id.is_empty() && id.len() <= MAX_ID;
    history.sessions.retain(|s| usable(&s.id));
    for session in &mut history.sessions {
        truncate(&mut session.profile_name, MAX_PROFILE_NAME * 2);
        if let Some(goal) = &mut session.goal {
            truncate(goal, MAX_GOAL_LENGTH);
        }
        if session.goal.is_none() {
            session.goal_met = None;
        }
    }
    history.gate_visits.retain(|v| usable(&v.visit.id));
    for logged in &mut history.gate_visits {
        logged.visit = logged.visit.clone().trimmed();
        logged.session_id = logged.session_id.take().filter(|id| usable(id));
    }
    history.satellites.retain(|s| usable(&s.id));
    for satellite in &mut history.satellites {
        if satellite.state == SatelliteState::Deleted {
            satellite.text.clear();
        }
        truncate(&mut satellite.text, satellites::MAX_TEXT);
        satellite.session_id = satellite.session_id.take().filter(|id| usable(id));
    }
    history.constellation_names = history
        .constellation_names
        .into_iter()
        .filter(|c| usable(&c.id))
        .filter_map(|c| cleaned_name(&c.name).map(|name| ConstellationName { name, ..c }))
        .collect();
    history
}

/// A value as it's saved, for `Store::import`.
fn json<T: Serialize>(synced: &Synced<T>) -> Result<Synced<serde_json::Value>> {
    Ok(Synced {
        value: serde_json::to_value(&synced.value)?,
        updated_at: synced.updated_at,
    })
}

/// A constellation's name worth keeping from what was typed: on one line, trimmed and cut down to
/// a sensible length, or None if there's nothing there.
fn cleaned_name(text: &str) -> Option<String> {
    // Runs of spaces and line breaks become one space.
    let mut name = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&mut name, MAX_CONSTELLATION_NAME);
    let name = name.trim_end().to_owned();
    (!name.is_empty()).then_some(name)
}

/// Notes `at` as when each profile that's new, edited or gone between two versions of the list
/// last changed in it.
fn restamp(stamps: &mut BTreeMap<String, i64>, before: &[Profile], after: &[Profile], at: i64) {
    let changed = after
        .iter()
        .filter(|p| before.iter().find(|b| b.id == p.id) != Some(*p));
    let gone = before.iter().filter(|b| !after.iter().any(|p| p.id == b.id));
    for profile in changed.chain(gone) {
        stamps.insert(profile.id.clone(), at);
    }
}

/// Lays the extension's list of profiles over the one here. `based_on` is the version of this
/// list the extension last took: what it changed since then wins, but a profile that changed
/// here after that (from sync, say) stays as it is here, since the extension's list was made
/// without it. So a profile that arrived as the extension saved its list isn't dropped.
fn merge_from_extension(
    here: &[Profile],
    stamps: &BTreeMap<String, i64>,
    theirs: &[Profile],
    based_on: i64,
) -> Vec<Profile> {
    let changed_here = |id: &str| stamps.get(id).is_some_and(|&at| at > based_on);
    let mut merged: Vec<Profile> = theirs
        .iter()
        .filter_map(|profile| match here.iter().find(|p| p.id == profile.id) {
            // Deleted here since.
            None if changed_here(&profile.id) => None,
            Some(mine) if changed_here(&profile.id) => Some(mine.clone()),
            _ => Some(profile.clone()),
        })
        .collect();
    for mine in here {
        if changed_here(&mine.id) && !theirs.iter().any(|p| p.id == mine.id) {
            merged.push(mine.clone());
        }
    }
    merged
}

/// The profiles sync knows were deleted, as they last were.
fn deleted_profiles(store: &Store) -> Result<Vec<Profile>> {
    Ok(store
        .known_profiles()?
        .into_iter()
        .filter_map(|(p, deleted)| deleted.then_some(p))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::TimerEnd;

    const MIN: i64 = 60_000;

    fn engine() -> Engine {
        Engine::load(Store::open_in_memory().unwrap(), 0).unwrap()
    }

    fn profile_record(id: &str, name: &str, deleted: bool, updated_at: i64) -> Record {
        use crate::records::SyncedProfile;

        let profile: Profile =
            serde_json::from_value(serde_json::json!({ "id": id, "name": name, "sites": [], "starColor": "#9fe3c9" }))
                .unwrap();
        Record::Profile(SyncedProfile {
            id: id.into(),
            profile,
            deleted,
            updated_at,
        })
    }

    fn profile_ids(engine: &Engine) -> Vec<String> {
        engine.profiles.value.iter().map(|p| p.id.clone()).collect()
    }

    #[test]
    fn profiles_changed_here_are_sent_one_by_one() {
        let mut engine = engine();
        let (_, cursor) = engine.changed_records(0, 100).unwrap();
        engine.set_profile_star_color("writing", "#9FE3C9", MIN).unwrap();
        let (changed, cursor) = engine.changed_records(cursor, 100).unwrap();
        assert!(matches!(&changed[..], [Record::Profile(p)] if p.id == "writing" && !p.deleted && p.updated_at == MIN));

        // The extension deleting one is sent as deleted, with its last copy.
        let mut fewer = engine.profiles.value.clone();
        fewer.retain(|p| p.id != "writing");
        let incoming = Synced {
            value: fewer,
            updated_at: 2 * MIN,
        };
        assert!(engine.apply_remote(Some(incoming), None, None, 2 * MIN).unwrap());
        let (changed, _) = engine.changed_records(cursor, 100).unwrap();
        assert!(
            matches!(&changed[..], [Record::Profile(p)] if p.id == "writing" && p.deleted
            && p.profile.star_color().as_deref() == Some("#9fe3c9"))
        );
        assert_eq!(
            engine
                .snapshot(2 * MIN)
                .deleted_star_colors
                .get("writing")
                .map(String::as_str),
            Some("#9fe3c9")
        );
    }

    #[test]
    fn profiles_from_other_devices_join_the_list() {
        let mut engine = engine();
        let before = profile_ids(&engine);
        let (_, cursor) = engine.changed_records(0, 100).unwrap();

        // A new one goes on the end, newer than the extension's list so it takes it on too.
        engine
            .apply_records(&[profile_record("reading", "Reading", false, 5 * MIN)], 10 * MIN)
            .unwrap();
        assert_eq!(profile_ids(&engine), [before.clone(), vec!["reading".into()]].concat());
        assert_eq!(engine.profiles.updated_at, 10 * MIN);
        // It isn't sent back.
        assert_eq!(engine.changed_records(cursor, 100).unwrap().0, vec![]);

        // An edit changes it in place, and an older one changes nothing.
        let first = before[0].clone();
        engine
            .apply_records(&[profile_record(&first, "Renamed", false, 6 * MIN)], 11 * MIN)
            .unwrap();
        assert_eq!(profile_ids(&engine)[0], first);
        assert_eq!(engine.profile_name(&first), "Renamed");
        assert_eq!(
            engine
                .apply_records(&[profile_record(&first, "Older", false, MIN)], 12 * MIN)
                .unwrap(),
            0
        );

        // Deleted elsewhere, it goes, but its stars keep its color and its name stays known.
        engine
            .apply_records(&[profile_record(&first, "Renamed", true, 7 * MIN)], 13 * MIN)
            .unwrap();
        assert!(!profile_ids(&engine).contains(&first));
        assert_eq!(engine.profile_name(&first), "Renamed");
        let snapshot = engine.snapshot(13 * MIN);
        assert_eq!(
            snapshot.deleted_star_colors.get(&first).map(String::as_str),
            Some("#9fe3c9")
        );
        assert!(!snapshot.profiles.iter().any(|p| p.id == first));
    }

    #[test]
    fn a_session_keeps_going_when_its_profile_is_deleted_elsewhere() {
        let mut engine = engine();
        let id = engine.start_session("deep-work", 0).unwrap().id.clone();
        let name = engine.profile_name("deep-work");
        engine
            .apply_records(&[profile_record("deep-work", &name, true, MIN)], MIN)
            .unwrap();
        assert_eq!(engine.snapshot(2 * MIN).session.map(|s| s.id), Some(id));
        let record = engine.end_session(30 * MIN, EndReason::Stopped).unwrap();
        assert_eq!(record.profile_name, name);
    }

    #[test]
    fn passes_used_on_other_devices_count_here() {
        use crate::records::SyncedSession;

        let mut engine = engine();
        assert_eq!(engine.passes_left(60 * MIN), 2);
        let session = SessionRecord {
            id: "elsewhere".into(),
            profile_id: "deep-work".into(),
            profile_name: "Deep Work".into(),
            started_at: 0,
            ended_at: 30 * MIN,
            timer_minutes: Some(60),
            focused_ms: 30 * MIN,
            overtime_ms: 0,
            idle_tail_ms: 0,
            end_reason: EndReason::EmergencyPass,
            broken: false,
            goal: None,
            goal_met: None,
        };
        let records = [Record::Session(SyncedSession {
            session,
            updated_at: 30 * MIN,
        })];
        assert_eq!(engine.apply_records(&records, 60 * MIN).unwrap(), 1);
        assert_eq!(engine.passes_left(60 * MIN), 1);
        assert_eq!(engine.changed_records(0, 10).unwrap().0, vec![]);
    }

    #[test]
    fn starts_with_the_starter_profiles() {
        let engine = engine();
        assert_eq!(engine.profiles.updated_at, 0);
        let snapshot = engine.snapshot(0);
        assert_eq!(snapshot.profiles.len(), 3);
        assert_eq!(snapshot.session, None);
    }

    #[test]
    fn runs_a_session_with_the_profile_timer() {
        let mut engine = engine();
        let session = engine.start_session("writing", 1_000).unwrap();
        assert_eq!(
            session.timer.as_ref().map(|t| (t.minutes, t.on_end)),
            Some((45, TimerEnd::Silent))
        );
        assert_eq!(engine.snapshot(0).session.unwrap().profile_name, "Writing");
        assert!(matches!(
            engine.start_session("deep-work", 2_000),
            Err(Error::SessionRunning)
        ));

        let record = engine.end_session(1_000 + 50 * MIN, EndReason::Stopped).unwrap();
        assert_eq!((record.focused_ms, record.overtime_ms), (45 * MIN, 5 * MIN));
        assert_eq!(engine.session(), None);
        assert_eq!(engine.session_log().unwrap(), vec![record]);
        assert!(matches!(
            engine.end_session(0, EndReason::Stopped),
            Err(Error::NoSession)
        ));
    }

    #[test]
    fn rejects_unknown_profiles() {
        let mut engine = engine();
        assert!(matches!(engine.start_session("nope", 1), Err(Error::UnknownProfile)));
        assert!(matches!(
            engine.set_profile_timer("nope", TimerSettings::default(), 1),
            Err(Error::UnknownProfile)
        ));
    }

    #[test]
    fn saves_profile_timers_for_next_time() {
        let mut engine = engine();
        let timer = TimerSettings {
            minutes: 9_999,
            on_end: TimerEnd::Alert,
            show_since_done: true,
            strictness: Strictness::Medium,
        };
        engine.set_profile_timer("deep-work", timer, 7).unwrap();

        let reloaded = Engine::load(
            std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap()),
            0,
        )
        .unwrap();
        let deep_work = reloaded.profile("deep-work").unwrap();
        assert_eq!(deep_work.timer().minutes, crate::timer::MAX_MINUTES);
        assert_eq!(deep_work.timer().on_end, TimerEnd::Alert);
        assert_eq!(reloaded.profiles.updated_at, 7);
    }

    #[test]
    fn sets_star_colors_and_keeps_the_starter_ones() {
        let mut engine = engine();
        let color = |engine: &Engine, id: &str| {
            let profiles = engine.snapshot(0).profiles;
            profiles.into_iter().find(|p| p.id == id).unwrap().star_color
        };
        assert_eq!(color(&engine, "writing").as_deref(), Some("#f5c86b"));

        engine.set_profile_star_color("writing", "#9FE3C9", 3).unwrap();
        assert_eq!(color(&engine, "writing").as_deref(), Some("#9fe3c9"));
        assert_eq!(engine.profiles.updated_at, 3);
        for bad in ["red", "#9fe3c", "#9fe3c9ff", "#gggggg", "url(x)"] {
            assert!(matches!(
                engine.set_profile_star_color("writing", bad, 4),
                Err(Error::InvalidStarColor)
            ));
        }
        assert!(matches!(
            engine.set_profile_star_color("nope", "#9fe3c9", 4),
            Err(Error::UnknownProfile)
        ));
        assert_eq!(color(&engine, "writing").as_deref(), Some("#9fe3c9"));
    }

    #[test]
    fn keeps_a_goal_until_did_you_do_it_is_answered() {
        let mut engine = engine();
        let asks = |engine: &Engine| {
            engine
                .snapshot(0)
                .profiles
                .iter()
                .find(|p| p.id == "writing")
                .unwrap()
                .ask_goal
        };
        assert!(!asks(&engine));
        engine.set_profile_asks_goal("writing", true, 1).unwrap();
        assert!(asks(&engine));

        let id = engine.start_session("writing", 1_000).unwrap().id.clone();
        engine.set_session_goal("  Finish   the intro\n", 1_000).unwrap();
        let snapshot = engine.snapshot(2_000);
        assert_eq!(snapshot.session.unwrap().goal.as_deref(), Some("Finish the intro"));
        assert_eq!(snapshot.goal_to_answer, None);

        let record = engine.end_session(30 * MIN, EndReason::Stopped).unwrap();
        assert_eq!(record.goal.as_deref(), Some("Finish the intro"));
        let waiting = engine.snapshot(30 * MIN).goal_to_answer.unwrap();
        assert_eq!(
            (waiting.session_id.as_str(), waiting.text.as_str()),
            (id.as_str(), "Finish the intro")
        );

        // It waits through a restart.
        let mut engine = Engine::load(
            std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap()),
            0,
        )
        .unwrap();
        assert!(engine.snapshot(31 * MIN).goal_to_answer.is_some());
        engine.answer_goal("someone-else", false, 31 * MIN).unwrap();
        assert!(engine.snapshot(31 * MIN).goal_to_answer.is_some());
        engine.answer_goal(&id, true, 31 * MIN).unwrap();
        assert_eq!(engine.snapshot(31 * MIN).goal_to_answer, None);
        assert_eq!(engine.store.sessions().unwrap()[0].goal_met, Some(true));

        // Left unanswered, it stops waiting when the next session starts, which can skip its own.
        engine.start_session("writing", 40 * MIN).unwrap();
        engine.set_session_goal("Outline part two", 40 * MIN).unwrap();
        engine.end_session(50 * MIN, EndReason::Stopped).unwrap();
        engine.start_session("writing", 60 * MIN).unwrap();
        engine.set_session_goal("   ", 60 * MIN).unwrap();
        assert_eq!(engine.snapshot(61 * MIN).session.unwrap().goal, None);
        engine.end_session(70 * MIN, EndReason::Stopped).unwrap();
        assert_eq!(engine.snapshot(70 * MIN).goal_to_answer, None);
        let log = engine.store.sessions().unwrap();
        assert_eq!(log[1].goal.as_deref(), Some("Outline part two"));
        assert_eq!(log[1].goal_met, None);
        assert_eq!(log[2].goal, None);
    }

    #[test]
    fn sets_plans_during_hard_sessions_too() {
        let mut engine = hard_engine();
        let plan = |engine: &Engine| {
            let profiles = engine.snapshot(0).profiles;
            profiles.into_iter().find(|p| p.id == "deep-work").unwrap().plan
        };
        assert_eq!(plan(&engine), "");

        engine.start_session("deep-work", 1).unwrap();
        engine
            .set_profile_plan("deep-work", " When I'm stuck, I'll write the next step. ", 2)
            .unwrap();
        assert_eq!(plan(&engine), "When I'm stuck, I'll write the next step.");
        assert_eq!(engine.profiles.updated_at, 2);
        engine.set_profile_plan("deep-work", "", 3).unwrap();
        assert_eq!(plan(&engine), "");
        assert!(matches!(
            engine.set_profile_plan("nope", "Plan", 4),
            Err(Error::UnknownProfile)
        ));
    }

    #[test]
    fn adds_and_removes_samples_without_touching_real_sessions() {
        let mut engine = engine();
        engine.start_session("writing", 1_000).unwrap();
        let real = engine.end_session(1_000 + 50 * MIN, EndReason::Stopped).unwrap();
        engine.add_samples(1_800_000_000_000).unwrap();
        let log = engine.session_log().unwrap();
        assert!(log.len() > 20);
        assert!(log.contains(&real));

        let sample = log.iter().find(|s| s.id != real.id).unwrap();
        engine.name_constellation(&sample.id, "Sample week", MIN).unwrap();
        engine.name_constellation(&real.id, "Real week", MIN).unwrap();

        engine.remove_samples().unwrap();
        assert_eq!(engine.session_log().unwrap(), vec![real.clone()]);
        assert!(engine.store.gate_visits().unwrap().is_empty());
        let names = engine.constellation_names().unwrap();
        assert_eq!(names.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), [real.id]);
    }

    #[test]
    fn keeps_sync_settings_out_of_backups() {
        let engine = engine();
        assert_eq!(engine.sync_settings().unwrap(), None);
        let settings = crate::sync::settings::SyncSettings::new(
            "https://sync.example.com".into(),
            crate::sync::client::Membership {
                group_id: "group".into(),
                device_id: "device".into(),
                token: "the-token".into(),
            },
            &crate::sync::crypto::GroupKey::generate(),
            "Mac mini".into(),
        );
        engine.set_sync_settings(Some(&settings), 1_000).unwrap();
        assert_eq!(engine.sync_settings().unwrap(), Some(settings.clone()));
        let backup = serde_json::to_string(&engine.backup(2_000).unwrap()).unwrap();
        assert!(!backup.contains("the-token") && !backup.contains(&settings.key));

        engine.set_sync_settings(None, 3_000).unwrap();
        assert_eq!(engine.sync_settings().unwrap(), None);
    }

    #[test]
    fn joining_a_group_takes_its_sky_and_keeps_what_was_seen_here() {
        let mut engine = engine();
        let here = engine.sky_state(1_000).unwrap();
        engine.sky_seen(4_000, 5_000).unwrap();
        assert!(!engine.has_own_sessions().unwrap());
        engine.add_samples(5_000).unwrap();
        assert!(!engine.has_own_sessions().unwrap());

        engine.match_sky("the group's secret", 6_000).unwrap();
        let joined = engine.sky_state(7_000).unwrap();
        assert_ne!(joined.secret, here.secret);
        assert_eq!((joined.secret.as_str(), joined.seen_at), ("the group's secret", 4_000));
    }

    #[test]
    fn keeps_one_sky_secret_and_only_moves_seen_forward() {
        let engine = engine();
        let first = engine.sky_state(1_000).unwrap();
        assert_eq!(first.seen_at, 1_000);
        assert_eq!(first.secret.len(), 32);
        assert_eq!(engine.sky_state(5_000).unwrap(), first);

        engine.sky_seen(3_000, 4_000).unwrap();
        assert_eq!(engine.sky_state(0).unwrap().seen_at, 3_000);
        engine.sky_seen(2_000, 4_000).unwrap();
        assert_eq!(engine.sky_state(0).unwrap().seen_at, 3_000);
        // Never past now, so a wrong time can't hide what's still to come.
        engine.sky_seen(i64::MAX, 6_000).unwrap();
        let after = engine.sky_state(0).unwrap();
        assert_eq!((after.seen_at, &after.secret), (6_000, &first.secret));
        assert_ne!(
            Engine::load(Store::open_in_memory().unwrap(), 0)
                .unwrap()
                .sky_state(0)
                .unwrap()
                .secret,
            after.secret
        );
    }

    #[test]
    fn hands_the_sky_trips_that_went_back_to_focus() {
        let engine = engine();
        let visit = |id: &str, continued| GateVisit {
            id: id.into(),
            recipe_id: "reddit".into(),
            at: 1_000,
            destination: Some("gate".into()),
            why: None,
            intention: None,
            continued,
        };
        engine.record_gate_visit(visit("back", false), MIN).unwrap();
        engine.record_gate_visit(visit("went", true), MIN).unwrap();
        let ids: Vec<_> = engine.turned_back().unwrap().into_iter().map(|v| v.id).collect();
        assert_eq!(ids, ["back"]);
    }

    #[test]
    fn names_constellations_and_forgets_blank_names() {
        let mut engine = engine();
        engine.start_session("writing", 1_000).unwrap();
        let running = engine.session().unwrap().id.clone();
        // The week's first star can be the session that's still running.
        engine
            .name_constellation(&running, "  The   Big\nDipper ", MIN)
            .unwrap();
        engine.end_session(1_000 + 50 * MIN, EndReason::Stopped).unwrap();
        let named = |engine: &Engine| {
            let names = engine.constellation_names().unwrap();
            names.into_iter().map(|n| (n.id, n.name)).collect::<Vec<_>>()
        };
        assert_eq!(named(&engine), [(running.clone(), "The Big Dipper".to_owned())]);

        let long = "é".repeat(MAX_CONSTELLATION_NAME + 10);
        engine.name_constellation(&running, &long, MIN).unwrap();
        assert_eq!(named(&engine)[0].1.chars().count(), MAX_CONSTELLATION_NAME);

        engine.name_constellation(&running, " \n ", MIN).unwrap();
        assert_eq!(named(&engine), []);
        assert!(matches!(
            engine.name_constellation("nope", "Orion", MIN),
            Err(Error::UnknownConstellation)
        ));
    }

    #[test]
    fn keeps_own_quotes() {
        let mut engine = engine();
        assert!(engine.add_own_quote(" Ship it. ", "", 1_000).unwrap());
        assert!(!engine.add_own_quote("  ", "Me", 1_100).unwrap());
        let quotes = engine.snapshot(1_100).own_quotes;
        assert_eq!(quotes.len(), 1);
        assert_eq!(
            (quotes[0].text.as_str(), quotes[0].author.as_deref()),
            ("Ship it.", None)
        );
        assert_eq!(engine.own_quotes.updated_at, 1_000);

        let id = quotes[0].id.clone();
        engine.remove_own_quote(&id, 900).unwrap();
        assert!(engine.snapshot(1_100).own_quotes.is_empty());
        assert_eq!(
            engine.own_quotes.updated_at, 1_001,
            "always newer than the last change, even if the clock went back"
        );
    }

    #[test]
    fn takes_newer_site_settings_from_the_extension_tidied() {
        let mut engine = engine();
        let incoming = |updated_at: i64, id: &str| Synced {
            value: serde_json::from_value::<Vec<SiteSettings>>(serde_json::json!([
                { "recipeId": id, "allTheTime": true, "hidden": [{ "selector": "a{}", "label": "Bad" }] },
                { "recipeId": "Not a site" },
            ]))
            .unwrap(),
            updated_at,
        };
        assert!(engine.apply_remote_site_settings(incoming(5, "reddit"), 1_000).unwrap());
        assert!(
            !engine
                .apply_remote_site_settings(incoming(4, "youtube"), 1_000)
                .unwrap()
        );
        let saved = &engine.site_settings().value;
        assert_eq!(saved.len(), 1);
        assert_eq!(
            (saved[0].recipe_id.as_str(), saved[0].all_the_time),
            ("reddit", Some(true))
        );
        assert_eq!(saved[0].hidden, None);
        // Nothing comes from the future.
        assert!(engine.apply_remote_site_settings(incoming(9_000, "x"), 1_000).unwrap());
        assert_eq!(engine.site_settings().updated_at, 1_000);
    }

    #[test]
    fn stops_adding_quotes_at_the_limit() {
        let mut engine = engine();
        for i in 0..MAX_OWN_QUOTES {
            engine.add_own_quote(&format!("Line {i}"), "", 1_000).unwrap();
        }
        assert!(matches!(
            engine.add_own_quote("One more", "", 2_000),
            Err(Error::TooManyQuotes)
        ));
    }

    fn remote_session(id: &str, profile_id: &str, timer: Option<TimerSettings>) -> Session {
        Session {
            id: id.into(),
            profile_id: profile_id.into(),
            started_at: 1_000,
            timer,
            rest: serde_json::from_value(serde_json::json!({ "passes": { "reddit": 5 } })).unwrap(),
        }
    }

    #[test]
    fn takes_newer_changes_from_the_extension() {
        let mut engine = engine();
        let mut profiles = starter_profiles();
        profiles[0].name = "Essays".into();
        let changed = engine
            .apply_remote(
                Some(Synced {
                    value: profiles,
                    updated_at: 50,
                }),
                None,
                None,
                100,
            )
            .unwrap();
        assert!(changed);
        assert_eq!(engine.snapshot(0).profiles[0].name, "Essays");

        // Older or equal changes are ignored.
        let stale = Synced {
            value: starter_profiles(),
            updated_at: 50,
        };
        assert!(!engine.apply_remote(Some(stale), None, None, 100).unwrap());
        assert_eq!(engine.snapshot(0).profiles[0].name, "Essays");
    }

    #[test]
    fn the_extension_cant_undo_profile_changes_it_hadnt_seen() {
        let mut engine = engine();
        let based_on = engine.profiles.updated_at;
        let theirs = engine.profiles.value.clone();
        let [first, second, third] = [0, 1, 2].map(|i| theirs[i].id.clone());
        let (_, cursor) = engine.changed_records(0, 100).unwrap();

        // From other devices: a new profile, an edit to one and the other deleted.
        engine
            .apply_records(
                &[
                    profile_record("reading", "Reading", false, MIN),
                    profile_record(&first, "Renamed", false, MIN),
                    profile_record(&second, "Gone", true, MIN),
                ],
                10 * MIN,
            )
            .unwrap();

        // Meanwhile the extension added one to the list it had, before it heard of those.
        let mut added = theirs;
        added.push(serde_json::from_value(serde_json::json!({ "id": "music", "name": "Music", "sites": [] })).unwrap());
        let incoming = Synced {
            value: added,
            updated_at: 11 * MIN,
        };
        assert!(
            engine
                .apply_remote(Some(incoming), Some(based_on), None, 12 * MIN)
                .unwrap()
        );
        assert_eq!(
            profile_ids(&engine),
            [first.clone(), third, "music".into(), "reading".into()]
        );
        assert_eq!(engine.profile_name(&first), "Renamed");
        // Newer than the extension's, so it takes the ones it was missing.
        assert_eq!(engine.profiles.updated_at, 11 * MIN + 1);
        // Only the extension's new one is sent: nothing came back to life or was deleted again.
        let (changed, _) = engine.changed_records(cursor, 100).unwrap();
        assert!(matches!(&changed[..], [Record::Profile(p)] if p.id == "music" && !p.deleted));

        // Once it has taken that list, what it changes wins again, deleting included.
        let based_on = engine.profiles.updated_at;
        let mut fewer = engine.profiles.value.clone();
        fewer.retain(|p| p.id != "reading");
        fewer[0].name = "Essays".into();
        let incoming = Synced {
            value: fewer.clone(),
            updated_at: 13 * MIN,
        };
        assert!(
            engine
                .apply_remote(Some(incoming), Some(based_on), None, 13 * MIN)
                .unwrap()
        );
        assert_eq!(engine.profiles.value, fewer);
        assert_eq!(engine.profiles.updated_at, 13 * MIN);
    }

    #[test]
    fn gives_sessions_from_the_extension_their_timer() {
        let mut engine = engine();
        let incoming = Synced {
            value: Some(remote_session("s1", "writing", None)),
            updated_at: 2_000,
        };
        assert!(engine.apply_remote(None, None, Some(incoming), 3_000).unwrap());
        let (_, session) = engine.synced();
        assert_eq!(session.updated_at, 2_001);
        let value = session.value.as_ref().unwrap();
        assert_eq!(value.timer.as_ref().map(|t| t.minutes), Some(45));
        assert_eq!(value.rest["passes"]["reddit"], 5);
    }

    #[test]
    fn logs_sessions_the_extension_ends() {
        let mut engine = engine();
        engine.start_session("writing", 1_000).unwrap();
        let ended = Synced {
            value: None,
            updated_at: 1_000 + 30 * MIN,
        };
        assert!(engine.apply_remote(None, None, Some(ended), 1_000 + 31 * MIN).unwrap());
        assert_eq!(engine.session(), None);
        let log = engine.session_log().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!((log[0].ended_at, log[0].focused_ms), (1_000 + 30 * MIN, 30 * MIN));
    }

    #[test]
    fn keeps_the_goal_of_a_session_the_extension_ends_or_replaces() {
        let mut engine = engine();
        engine.start_session("writing", 1_000).unwrap();
        engine.set_session_goal("Draft the intro", 1_000).unwrap();
        let ended = Synced {
            value: None,
            updated_at: 30 * MIN,
        };
        engine.apply_remote(None, None, Some(ended), 31 * MIN).unwrap();
        assert_eq!(
            engine.session_log().unwrap()[0].goal.as_deref(),
            Some("Draft the intro")
        );
        assert!(engine.snapshot(31 * MIN).goal_to_answer.is_some());

        // A session the extension starts has no goal, and the last one stops waiting.
        let started = Synced {
            value: Some(remote_session("s2", "deep-work", None)),
            updated_at: 40 * MIN,
        };
        engine.apply_remote(None, None, Some(started), 40 * MIN).unwrap();
        assert_eq!(engine.snapshot(40 * MIN).session.unwrap().goal, None);
        let ended = Synced {
            value: None,
            updated_at: 50 * MIN,
        };
        engine.apply_remote(None, None, Some(ended), 50 * MIN).unwrap();
        assert_eq!(engine.snapshot(50 * MIN).goal_to_answer, None);
    }

    #[test]
    fn logs_a_session_replaced_by_another() {
        let mut engine = engine();
        engine.start_session("writing", 1_000).unwrap();
        let timer = Some(TimerSettings::default());
        let replaced = Synced {
            value: Some(remote_session("s2", "deep-work", timer)),
            updated_at: 5_000,
        };
        engine.apply_remote(None, None, Some(replaced), 6_000).unwrap();
        assert_eq!(engine.session().map(|s| s.id.as_str()), Some("s2"));
        assert_eq!(engine.session_log().unwrap().len(), 1);
    }

    #[test]
    fn keeps_the_same_session_going() {
        let mut engine = engine();
        let id = engine.start_session("writing", 1_000).unwrap().id.clone();
        let mut same = engine.session().cloned().unwrap();
        same.rest.insert("passes".into(), serde_json::json!({ "x": 9 }));
        let incoming = Synced {
            value: Some(same),
            updated_at: 2_000,
        };
        assert!(engine.apply_remote(None, None, Some(incoming), 3_000).unwrap());
        assert_eq!(engine.session().map(|s| s.id.clone()), Some(id));
        assert_eq!(engine.session_log().unwrap(), vec![]);
    }

    #[test]
    fn ignores_changes_from_the_future() {
        let mut engine = engine();
        let far_future = Synced {
            value: Some(remote_session("s1", "writing", None)),
            updated_at: i64::MAX - 10,
        };
        engine.apply_remote(None, None, Some(far_future), 5_000).unwrap();
        assert_eq!(engine.synced().1.updated_at, 5_001);
    }

    #[test]
    fn logs_gate_visits_with_their_session() {
        let mut engine = engine();
        let visit = |id: &str, at: i64| -> GateVisit {
            serde_json::from_value(serde_json::json!({ "id": id, "recipeId": "reddit", "at": at })).unwrap()
        };
        engine.record_gate_visit(visit("before", 10), MIN).unwrap();
        let session_id = engine.start_session("writing", 1_000).unwrap().id.clone();
        engine.record_gate_visit(visit("during", 2_000), MIN).unwrap();
        let mut answered = visit("during", 2_000);
        answered.why = Some("bored".into());
        answered.continued = true;
        engine.record_gate_visit(answered, MIN).unwrap();

        let visits = engine.store.gate_visits().unwrap();
        assert_eq!(visits.len(), 2);
        assert_eq!(visits[0].0, None);
        assert_eq!(visits[1].0, Some(session_id));
        assert_eq!(visits[1].1.why.as_deref(), Some("bored"));
        assert!(visits[1].1.continued);
    }

    /// An engine with Deep Work set to a 50-minute Hard timer.
    fn hard_engine() -> Engine {
        let mut engine = engine();
        let timer = TimerSettings {
            minutes: 50,
            strictness: Strictness::Hard,
            ..TimerSettings::default()
        };
        engine.set_profile_timer("deep-work", timer, 1).unwrap();
        engine
    }

    #[test]
    fn hard_sessions_end_early_only_with_a_pass() {
        let mut engine = hard_engine();
        engine.start_session("deep-work", 0).unwrap();
        assert!(matches!(
            engine.end_session(10 * MIN, EndReason::Stopped),
            Err(Error::Locked)
        ));
        assert_eq!(engine.passes_left(10 * MIN), 2);

        let record = engine.end_session(10 * MIN, EndReason::EmergencyPass).unwrap();
        assert_eq!(record.end_reason, EndReason::EmergencyPass);
        assert_eq!(engine.passes_left(10 * MIN), 1);

        // Once the timer is done it ends like any other, and no pass is spent.
        engine.start_session("deep-work", 20 * MIN).unwrap();
        let record = engine.end_session(70 * MIN, EndReason::EmergencyPass).unwrap();
        assert_eq!(record.end_reason, EndReason::Stopped);
        assert_eq!(engine.passes_left(70 * MIN), 1);
    }

    #[test]
    fn passes_run_out_and_come_back_a_week_later() {
        let mut engine = hard_engine();
        for start in [0, 10 * MIN] {
            engine.start_session("deep-work", start).unwrap();
            engine.end_session(start + MIN, EndReason::EmergencyPass).unwrap();
        }
        engine.start_session("deep-work", 20 * MIN).unwrap();
        assert!(matches!(
            engine.end_session(21 * MIN, EndReason::EmergencyPass),
            Err(Error::NoPassesLeft)
        ));
        assert_eq!(engine.snapshot(21 * MIN).passes_left, 0);
        assert_eq!(engine.passes_left(MIN + PASS_WEEK_MS), 1);

        // Kept across restarts, since they're read from the session log.
        let reloaded = Engine::load(
            std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap()),
            0,
        )
        .unwrap();
        assert_eq!(reloaded.passes_left(21 * MIN), 0);
    }

    #[test]
    fn the_hotkey_only_changes_through_its_own_command() {
        let mut engine = hard_engine();
        engine
            .set_settings(
                Settings {
                    satellite_hotkey: "Control+Alt+KeyS".into(),
                    ..Settings::default()
                },
                0,
            )
            .unwrap();
        assert_eq!(engine.settings().satellite_hotkey, crate::hotkey::DEFAULT_HOTKEY);

        engine.set_satellite_hotkey("Control+Alt+KeyS".into(), 0).unwrap();
        assert_eq!(engine.settings().satellite_hotkey, "Control+Alt+KeyS");
        engine.set_satellite_hotkey(String::new(), 0).unwrap();
        assert_eq!(engine.settings().satellite_hotkey, "", "turned off");
    }

    #[test]
    fn passes_per_week_only_go_down_during_a_hard_session() {
        let mut engine = hard_engine();
        engine
            .set_settings(
                Settings {
                    passes_per_week: 99,
                    ..Settings::default()
                },
                0,
            )
            .unwrap();
        assert_eq!(
            engine.snapshot(0).settings.passes_per_week,
            crate::settings::MAX_PASSES_PER_WEEK
        );

        engine.start_session("deep-work", 0).unwrap();
        engine
            .set_settings(
                Settings {
                    passes_per_week: 1,
                    ..Settings::default()
                },
                MIN,
            )
            .unwrap();
        assert!(matches!(
            engine.set_settings(
                Settings {
                    passes_per_week: 5,
                    ..Settings::default()
                },
                MIN
            ),
            Err(Error::Locked)
        ));
        engine
            .set_settings(
                Settings {
                    passes_per_week: 5,
                    ..Settings::default()
                },
                60 * MIN,
            )
            .unwrap();
    }

    #[test]
    fn closing_browsers_stays_on_during_a_hard_session() {
        let mut engine = hard_engine();
        assert!(!engine.settings().close_browsers);
        let on = Settings {
            close_browsers: true,
            ..Settings::default()
        };
        engine.start_session("deep-work", 0).unwrap();
        // Turning it on is stricter, so that's fine; turning it off isn't, until the timer's done.
        engine.set_settings(on.clone(), MIN).unwrap();
        assert!(matches!(
            engine.set_settings(Settings::default(), 2 * MIN),
            Err(Error::Locked)
        ));
        engine.set_settings(Settings::default(), 60 * MIN).unwrap();
        assert!(!engine.settings().close_browsers);
    }

    #[test]
    fn blocking_without_the_extension_stays_on_until_a_hard_timer_is_done() {
        let mut engine = hard_engine();
        let on = Settings {
            block_without_extension: true,
            ..Settings::default()
        };
        engine.start_session("deep-work", 0).unwrap();
        engine.set_settings(on, MIN).unwrap();
        assert!(matches!(
            engine.set_settings(Settings::default(), 2 * MIN),
            Err(Error::Locked)
        ));
        engine.set_settings(Settings::default(), 60 * MIN).unwrap();
        assert!(!engine.settings().block_without_extension);
    }

    #[test]
    fn the_extension_cannot_end_a_hard_session_early() {
        let mut engine = hard_engine();
        let id = engine.start_session("deep-work", 1_000).unwrap().id.clone();
        let ended = Synced {
            value: None,
            updated_at: 10 * MIN,
        };
        assert!(engine.apply_remote(None, None, Some(ended), 11 * MIN).unwrap());
        let (_, session) = engine.synced();
        assert_eq!(session.value.as_ref().map(|s| s.id.clone()), Some(id));
        // Newer than the browser's change, so the browser takes it back.
        assert_eq!(session.updated_at, 10 * MIN + 1);
        assert_eq!(engine.session_log().unwrap(), vec![]);

        // Once the timer is done, it can.
        let ended = Synced {
            value: None,
            updated_at: 60 * MIN,
        };
        engine.apply_remote(None, None, Some(ended), 60 * MIN).unwrap();
        assert_eq!(engine.session(), None);
    }

    #[test]
    fn the_app_owns_a_running_session_timer() {
        let mut engine = hard_engine();
        engine.start_session("deep-work", 1_000).unwrap();
        let mut softened = engine.session().cloned().unwrap();
        softened.timer = Some(TimerSettings {
            minutes: 0,
            ..TimerSettings::default()
        });
        let incoming = Synced {
            value: Some(softened),
            updated_at: 2_000,
        };
        engine.apply_remote(None, None, Some(incoming), 3_000).unwrap();
        let (_, session) = engine.synced();
        let timer = session.value.as_ref().unwrap().timer.as_ref().unwrap();
        assert_eq!((timer.minutes, timer.strictness), (50, Strictness::Hard));
        assert_eq!(session.updated_at, 2_001);
    }

    #[test]
    fn ends_idle_sessions_without_the_time_away() {
        let mut engine = hard_engine();
        let id = engine.start_session("deep-work", 0).unwrap().id.clone();
        assert_eq!(engine.end_idle_session("other", 5 * MIN, 20 * MIN).unwrap(), None);

        let record = engine.end_idle_session(&id, 5 * MIN, 20 * MIN).unwrap().unwrap();
        assert_eq!((record.focused_ms, record.idle_tail_ms), (5 * MIN, 15 * MIN));
        assert_eq!(engine.session(), None);
        assert_eq!(engine.session_log().unwrap(), vec![record]);
    }

    fn chrome(name: &str) -> KnownBrowser {
        KnownBrowser {
            name: name.into(),
            path: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
        }
    }

    #[test]
    fn remembers_browsers_until_forgotten_between_sessions() {
        let mut engine = engine();
        assert!(engine.remember_browser(chrome("Google Chrome"), 1).unwrap());
        assert!(!engine.remember_browser(chrome("Google Chrome"), 2).unwrap());
        // Same browser, new name: replaced, not added.
        assert!(engine.remember_browser(chrome("Chrome"), 3).unwrap());
        assert_eq!(engine.snapshot(3).browsers, vec![chrome("Chrome")]);

        engine.start_session("deep-work", 4).unwrap();
        assert!(matches!(
            engine.forget_browser(&chrome("Chrome").path, 5),
            Err(Error::SessionRunning)
        ));
        engine.end_session(5, EndReason::Stopped).unwrap();
        engine.forget_browser(&chrome("Chrome").path, 6).unwrap();
        assert_eq!(engine.browsers(), &[]);

        engine.remember_browser(chrome("Chrome"), 7).unwrap();
        let reloaded = Engine::load(
            std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap()),
            0,
        )
        .unwrap();
        assert_eq!(reloaded.browsers(), &[chrome("Chrome")]);
    }

    #[test]
    fn app_rules_only_get_stricter_during_a_hard_session() {
        use crate::apps::AppAction;
        let rule = |name: &str, action| AppRule {
            name: name.into(),
            path: format!("/Applications/{name}.app/Contents/MacOS/{name}"),
            action,
        };
        let mut engine = hard_engine();
        let discord = vec![rule("Discord", AppAction::Close)];
        engine.set_profile_apps("deep-work", discord.clone(), 1).unwrap();
        assert_eq!(engine.profile_apps("deep-work"), discord);
        assert_eq!(engine.snapshot(1).profiles[1].apps, discord);
        assert!(matches!(
            engine.set_profile_apps("nope", vec![], 1),
            Err(Error::UnknownProfile)
        ));

        engine.start_session("deep-work", 0).unwrap();
        let more = vec![rule("Discord", AppAction::Close), rule("Steam", AppAction::Hide)];
        engine.set_profile_apps("deep-work", more.clone(), MIN).unwrap();
        for looser in [
            vec![],
            vec![rule("Discord", AppAction::Hide), rule("Steam", AppAction::Hide)],
        ] {
            assert!(matches!(
                engine.set_profile_apps("deep-work", looser, MIN),
                Err(Error::Locked)
            ));
        }
        // Other profiles, and this one once the timer's done, can change freely.
        engine.set_profile_apps("writing", vec![], MIN).unwrap();
        engine.set_profile_apps("deep-work", vec![], 60 * MIN).unwrap();
        assert_eq!(engine.profile_apps("deep-work"), vec![]);
    }

    #[test]
    fn picks_up_a_running_session_after_a_restart() {
        let mut engine = engine();
        engine.start_session("wind-down", 5).unwrap();
        let reloaded = Engine::load(
            std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap()),
            0,
        )
        .unwrap();
        assert_eq!(reloaded.session().map(|s| s.profile_id.as_str()), Some("wind-down"));
    }

    /// The satellites' text and state, oldest first.
    fn satellites(engine: &Engine) -> Vec<(&str, SatelliteState)> {
        engine.satellites.iter().map(|s| (s.text.as_str(), s.state)).collect()
    }

    fn satellite_id(engine: &Engine, text: &str) -> String {
        engine.satellites.iter().find(|s| s.text == text).unwrap().id.clone()
    }

    #[test]
    fn satellites_come_back_when_the_session_ends() {
        let mut engine = engine();
        engine.start_session("writing", 0).unwrap();
        assert!(engine.launch_satellite("  reply to Sam ", MIN).unwrap());
        assert!(!engine.launch_satellite(" \n ", MIN).unwrap());
        assert_eq!(satellites(&engine), vec![("reply to Sam", SatelliteState::Orbiting)]);

        engine.end_session(60 * MIN, EndReason::Stopped).unwrap();
        assert_eq!(satellites(&engine), vec![("reply to Sam", SatelliteState::Back)]);
        assert_eq!(engine.snapshot(60 * MIN).satellites, engine.satellites);

        // Launched between sessions, it waits for the next one to end.
        engine.launch_satellite("book dentist", 61 * MIN).unwrap();
        engine.start_session("writing", 62 * MIN).unwrap();
        engine.end_session(70 * MIN, EndReason::Stopped).unwrap();
        assert_eq!(
            satellites(&engine),
            vec![
                ("reply to Sam", SatelliteState::Back),
                ("book dentist", SatelliteState::Back)
            ]
        );
    }

    #[test]
    fn satellites_come_back_however_the_session_ends() {
        let mut engine = engine();
        engine.start_session("writing", 0).unwrap();
        engine.launch_satellite("idle", MIN).unwrap();
        let id = engine.session().unwrap().id.clone();
        engine.end_idle_session(&id, 2 * MIN, 10 * MIN).unwrap();
        assert_eq!(satellites(&engine), vec![("idle", SatelliteState::Back)]);

        let mut engine = self::engine();
        engine.start_session("writing", 0).unwrap();
        engine.launch_satellite("browser", MIN).unwrap();
        let ended = Synced {
            value: None,
            updated_at: 5 * MIN,
        };
        engine.apply_remote(None, None, Some(ended), 5 * MIN).unwrap();
        assert_eq!(satellites(&engine), vec![("browser", SatelliteState::Back)]);
    }

    #[test]
    fn acts_on_satellites_that_came_back() {
        let mut engine = engine();
        for text in ["done", "snooze", "task", "delete"] {
            engine.launch_satellite(text, 0).unwrap();
        }
        engine.start_session("writing", MIN).unwrap();
        engine.end_session(2 * MIN, EndReason::Stopped).unwrap();

        let act = |engine: &mut Engine, text: &str, action| {
            let id = satellite_id(engine, text);
            engine.act_on_satellite(&id, action, 3 * MIN).unwrap();
        };
        act(&mut engine, "done", SatelliteAction::Done);
        act(&mut engine, "snooze", SatelliteAction::Snooze);
        act(&mut engine, "task", SatelliteAction::Task);
        act(&mut engine, "delete", SatelliteAction::Delete);
        assert_eq!(
            satellites(&engine),
            vec![("snooze", SatelliteState::Orbiting), ("task", SatelliteState::Task)]
        );
        assert!(matches!(
            engine.act_on_satellite("nope", SatelliteAction::Done, 0),
            Err(Error::UnknownSatellite)
        ));

        // Snoozed ones come back after the next session; tasks stay on the list.
        engine.start_session("writing", 4 * MIN).unwrap();
        engine.end_session(5 * MIN, EndReason::Stopped).unwrap();
        assert_eq!(
            satellites(&engine),
            vec![("snooze", SatelliteState::Back), ("task", SatelliteState::Task)]
        );

        // A ticked-off task stays, as done.
        act(&mut engine, "task", SatelliteAction::Done);
        assert_eq!(
            satellites(&engine),
            vec![("snooze", SatelliteState::Back), ("task", SatelliteState::Done)]
        );
    }

    #[test]
    fn ticked_off_tasks_can_be_unticked_for_a_day() {
        const DAY: i64 = DONE_TASKS_SHOWN_MS;
        let mut engine = engine();
        engine.launch_satellite("oops", 0).unwrap();
        engine.launch_satellite("old", 0).unwrap();
        for text in ["oops", "old"] {
            let id = satellite_id(&engine, text);
            engine.act_on_satellite(&id, SatelliteAction::Task, MIN).unwrap();
        }
        let oops = satellite_id(&engine, "oops");
        let old = satellite_id(&engine, "old");
        engine.act_on_satellite(&old, SatelliteAction::Done, 2 * MIN).unwrap();
        engine.act_on_satellite(&oops, SatelliteAction::Done, 2 * MIN).unwrap();
        engine.act_on_satellite(&oops, SatelliteAction::Task, 3 * MIN).unwrap();
        assert_eq!(
            satellites(&engine),
            vec![("oops", SatelliteState::Task), ("old", SatelliteState::Done)]
        );

        // A day on, the done one drops off the list, and stays off after a restart.
        let shown = |engine: &Engine, now| -> Vec<String> {
            engine.snapshot(now).satellites.into_iter().map(|s| s.text).collect()
        };
        assert_eq!(shown(&engine, 2 * MIN + DAY), vec!["oops", "old"]);
        assert_eq!(shown(&engine, 3 * MIN + DAY), vec!["oops"]);
        let store = std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap());
        assert_eq!(
            shown(&Engine::load(store, 3 * MIN + DAY).unwrap(), 3 * MIN + DAY),
            vec!["oops"]
        );
    }

    #[test]
    fn offers_the_tour_until_it_has_been_offered_once() {
        let mut engine = engine();
        assert!(!engine.snapshot(0).tour_offered);
        engine.tour_offered(MIN).unwrap();
        assert!(engine.snapshot(MIN).tour_offered);

        let store = std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap());
        assert!(Engine::load(store, MIN).unwrap().snapshot(MIN).tour_offered);
    }

    #[test]
    fn offers_each_new_version_once() {
        let mut engine = engine();
        assert_eq!(engine.snapshot(0).update_offered, None);
        engine.update_offered("0.2.0", MIN).unwrap();
        assert_eq!(engine.snapshot(MIN).update_offered.as_deref(), Some("0.2.0"));

        let store = std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap());
        let mut engine = Engine::load(store, MIN).unwrap();
        assert_eq!(engine.snapshot(MIN).update_offered.as_deref(), Some("0.2.0"));
        engine.update_offered("0.3.0", 2 * MIN).unwrap();
        assert_eq!(engine.snapshot(2 * MIN).update_offered.as_deref(), Some("0.3.0"));
    }

    #[test]
    fn keeps_satellites_through_a_restart_but_not_deleted_text() {
        let mut engine = engine();
        engine.launch_satellite("keep me", 0).unwrap();
        engine.launch_satellite("secret", 0).unwrap();
        let id = satellite_id(&engine, "secret");
        engine.act_on_satellite(&id, SatelliteAction::Delete, MIN).unwrap();

        let store = std::mem::replace(&mut engine.store, Store::open_in_memory().unwrap());
        let texts: Vec<String> = store
            .db_for_tests()
            .prepare("SELECT text FROM satellites ORDER BY text")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(texts, vec!["", "keep me"]);

        let reloaded = Engine::load(store, 0).unwrap();
        assert_eq!(satellites(&reloaded), vec![("keep me", SatelliteState::Orbiting)]);
    }

    #[test]
    fn gathers_activity_for_the_weekly_report() {
        let mut engine = engine();
        assert_eq!(engine.activity(0, i64::MAX).unwrap().first_logged_at, None);

        let visit = |id: &str, at: i64| -> GateVisit {
            serde_json::from_value(serde_json::json!({ "id": id, "recipeId": "reddit", "at": at })).unwrap()
        };
        engine.start_session("writing", 10 * MIN).unwrap();
        engine.launch_satellite("last week", 11 * MIN).unwrap();
        engine.end_session(40 * MIN, EndReason::Stopped).unwrap();
        engine.record_gate_visit(visit("last week", 41 * MIN), MIN).unwrap();

        engine.start_session("writing", 100 * MIN).unwrap();
        engine.record_gate_visit(visit("this week", 110 * MIN), MIN).unwrap();
        engine.launch_satellite("this week", 120 * MIN).unwrap();
        engine.end_session(130 * MIN, EndReason::Stopped).unwrap();
        let id = satellite_id(&engine, "this week");
        engine.act_on_satellite(&id, SatelliteAction::Task, 131 * MIN).unwrap();

        let activity = engine.activity(100 * MIN, 200 * MIN).unwrap();
        assert_eq!(activity.first_logged_at, Some(10 * MIN));
        assert_eq!(activity.sessions.len(), 1);
        assert_eq!(activity.sessions[0].started_at, 100 * MIN);
        assert_eq!(activity.gate_visits.len(), 1);
        assert_eq!(activity.gate_visits[0].id, "this week");
        assert_eq!(
            activity.satellites,
            vec![SatelliteRecord {
                launched_at: 120 * MIN,
                state: SatelliteState::Task,
                state_at: 131 * MIN,
                snoozes: 0,
                tasked_at: Some(131 * MIN),
            }]
        );
        // The end is left out, so weeks side by side don't share anything.
        assert_eq!(engine.activity(0, 100 * MIN).unwrap().sessions.len(), 1);
    }

    /// A computer with a bit of everything: a session with a goal, a trip to a site, a
    /// satellite on the task list, a named constellation, a quote and its own settings.
    fn used_engine() -> Engine {
        let mut engine = engine();
        engine.set_profile_asks_goal("writing", true, 0).unwrap();
        engine.start_session("writing", MIN).unwrap();
        engine.set_session_goal("Draft the intro", MIN).unwrap();
        engine
            .record_gate_visit(
                GateVisit {
                    id: "visit".into(),
                    recipe_id: "reddit".into(),
                    at: 2 * MIN,
                    destination: Some("page".into()),
                    why: Some("stuck".into()),
                    intention: None,
                    continued: false,
                },
                MIN,
            )
            .unwrap();
        engine.launch_satellite("reply to Sam", 3 * MIN).unwrap();
        let session = engine.session().unwrap().id.clone();
        engine.end_session(50 * MIN, EndReason::Stopped).unwrap();
        engine.answer_goal(&session, true, 51 * MIN).unwrap();
        let id = satellite_id(&engine, "reply to Sam");
        engine.act_on_satellite(&id, SatelliteAction::Task, 52 * MIN).unwrap();
        engine.name_constellation(&session, "Orion", MIN).unwrap();
        engine.add_own_quote("Ship it.", "Me", 53 * MIN).unwrap();
        let settings = Settings {
            passes_per_week: 5,
            glow: "aurora".into(),
            satellite_hotkey: "Shift+Super+KeyK".into(),
            ..Settings::default()
        };
        engine.save_settings(settings, 54 * MIN).unwrap();
        engine.sky_state(54 * MIN).unwrap();
        let sites = serde_json::json!([{ "recipeId": "reddit", "allTheTime": true, "hidden": [{ "selector": "aside", "label": "Sidebar" }] }]);
        let sites = Synced {
            value: serde_json::from_value(sites).unwrap(),
            updated_at: 55 * MIN,
        };
        engine.apply_remote_site_settings(sites, 55 * MIN).unwrap();
        engine
    }

    /// Writes a backup the way a file holds it and reads it back.
    fn through_a_file(backup: &Backup) -> Backup {
        Backup::read(&serde_json::to_string(backup).unwrap()).unwrap()
    }

    #[test]
    fn a_backup_brings_everything_to_another_computer() {
        let old = used_engine();
        let backup = through_a_file(&old.backup(60 * MIN).unwrap());
        let mut new = engine();
        new.set_satellite_hotkey("Alt+KeyL".into(), 0).unwrap();

        let report = new.import(backup, 100 * MIN).unwrap();
        assert_eq!(
            report,
            ImportReport {
                new_profiles: 0,
                updated_profiles: 1,
                sessions: 1,
                gate_visits: 1,
                satellites: 1,
                quotes: 1,
            }
        );
        assert_eq!(new.session_log().unwrap(), old.session_log().unwrap());
        assert_eq!(new.session_log().unwrap()[0].goal_met, Some(true));
        assert_eq!(new.store.gate_visits().unwrap(), old.store.gate_visits().unwrap());
        assert_eq!(new.store.history().unwrap(), old.store.history().unwrap());
        assert_eq!(satellites(&new), vec![("reply to Sam", SatelliteState::Task)]);
        assert_eq!(new.constellation_names().unwrap(), old.constellation_names().unwrap());
        assert_eq!(new.snapshot(0).own_quotes, old.snapshot(0).own_quotes);
        assert_eq!(new.site_settings().value, old.site_settings().value);
        assert_eq!(new.site_settings().updated_at, 100 * MIN);
        assert!(new.snapshot(0).profiles.iter().any(|p| p.id == "writing" && p.ask_goal));
        // Newer than the extension's copy, so the extension takes the profiles too.
        assert_eq!(new.profiles.updated_at, 100 * MIN);
        // The hotkey and blocking are set up on each computer.
        assert_eq!(
            (new.settings.passes_per_week, new.settings.glow.as_str()),
            (5, "aurora")
        );
        assert_eq!(new.settings.satellite_hotkey, "Alt+KeyL");
        // Nothing was logged here, so the sky is drawn as it was, with what came in already seen.
        let sky = new.sky_state(0).unwrap();
        assert_eq!(sky.secret, old.sky_state(0).unwrap().secret);
        assert_eq!(sky.seen_at, 100 * MIN);

        // The same file again adds nothing.
        let again = through_a_file(&old.backup(60 * MIN).unwrap());
        assert_eq!(new.import(again, 110 * MIN).unwrap(), ImportReport::default());
    }

    #[test]
    fn an_import_adds_to_whats_here_and_takes_nothing_away() {
        let mut backup = used_engine().backup(60 * MIN).unwrap();
        let mut theirs = backup.profiles[0].clone();
        theirs.id = "theirs".into();
        theirs.name = "  Their   profile ".into();
        theirs.rest.insert("starColor".into(), "red; display: none".into());
        backup.profiles.push(theirs);

        let mut here = engine();
        here.start_session("deep-work", 0).unwrap();
        assert!(matches!(
            here.import(through_a_file(&backup), MIN),
            Err(Error::SessionRunning)
        ));
        here.end_session(30 * MIN, EndReason::Stopped).unwrap();
        here.launch_satellite("mine", 31 * MIN).unwrap();
        here.add_own_quote("Ship it.", "", 31 * MIN).unwrap();
        let secret = here.sky_state(31 * MIN).unwrap().secret;

        let report = here.import(through_a_file(&backup), 100 * MIN).unwrap();
        assert_eq!((report.new_profiles, report.updated_profiles), (1, 1));
        assert_eq!(report.quotes, 0, "the same quote isn't added twice");
        let names: Vec<_> = here.snapshot(0).profiles.into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Writing", "Deep Work", "Wind Down", "Their profile"]);
        let theirs = here.profile("theirs").unwrap();
        assert!(
            !theirs.rest.contains_key("starColor"),
            "a star color that isn't one is dropped"
        );
        assert_eq!(here.session_log().unwrap().len(), 2);
        assert_eq!(satellites(&here).len(), 2);
        // This computer has its own sky, which stays as it is.
        assert_eq!(here.sky_state(0).unwrap().secret, secret);
    }
}
