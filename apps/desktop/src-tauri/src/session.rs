use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::apps::AppRule;
use crate::sites::SiteRule;
use crate::timer::{Strictness, TimerSettings};

/// The longest if-then plan, in characters, the same as `maxPlanLength` in core's settings.ts.
pub const MAX_PLAN_LENGTH: usize = 140;

/// The longest session goal, in characters.
pub const MAX_GOAL_LENGTH: usize = 140;

/// A profile, as the extension saves it. The app only reads a few fields; everything else
/// (site levels, redirect settings) is kept as it is, so nothing is lost when it's saved back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Profile {
    pub fn timer(&self) -> TimerSettings {
        TimerSettings::from_saved(self.rest.get("timer"))
    }

    pub fn set_timer(&mut self, timer: &TimerSettings) {
        let value = serde_json::to_value(timer).expect("timer settings serialize");
        self.rest.insert("timer".into(), value);
    }

    /// The apps this profile closes or hides during its sessions. Ones that can't be read are
    /// skipped.
    pub fn apps(&self) -> Vec<AppRule> {
        let saved = self.rest.get("apps").and_then(Value::as_array);
        saved
            .into_iter()
            .flatten()
            .filter_map(|rule| AppRule::deserialize(rule).ok())
            .collect()
    }

    pub fn set_apps(&mut self, apps: &[AppRule]) {
        let value = serde_json::to_value(apps).expect("app rules serialize");
        self.rest.insert("apps".into(), value);
    }

    /// How the profile treats each site. Ones that can't be read are skipped.
    pub fn sites(&self) -> Vec<SiteRule> {
        let saved = self.rest.get("sites").and_then(Value::as_array);
        saved
            .into_iter()
            .flatten()
            .filter_map(|rule| SiteRule::deserialize(rule).ok())
            .collect()
    }

    pub fn set_sites(&mut self, sites: &[SiteRule]) {
        let value = serde_json::to_value(sites).expect("site rules serialize");
        self.rest.insert("sites".into(), value);
    }

    /// The color of the profile's stars, if it was given one. The window picks a default
    /// otherwise (packages/core/src/sky.ts).
    pub fn star_color(&self) -> Option<String> {
        let color = self.rest.get("starColor").and_then(Value::as_str)?;
        is_star_color(color).then(|| color.to_owned())
    }

    pub fn set_star_color(&mut self, color: &str) {
        self.rest.insert("starColor".into(), color.to_ascii_lowercase().into());
    }

    /// The if-then plan the redirect page shows, or "" if it has none (PLAN §3).
    pub fn plan(&self) -> String {
        let plan = self.rest.get("redirect").and_then(|r| r.get("plan"));
        let mut plan = plan.and_then(Value::as_str).unwrap_or_default().trim().to_owned();
        truncate(&mut plan, MAX_PLAN_LENGTH);
        plan
    }

    /// Changes the plan, trimmed and cut to length, leaving the rest of the redirect settings
    /// (the wait and links) as they are.
    pub fn set_plan(&mut self, plan: &str) {
        let mut plan = plan.trim().to_owned();
        truncate(&mut plan, MAX_PLAN_LENGTH);
        let redirect = self.rest.entry("redirect").or_insert_with(|| Value::Object(Map::new()));
        if !redirect.is_object() {
            *redirect = Value::Object(Map::new());
        }
        redirect["plan"] = plan.into();
    }

    /// Whether starting one of its sessions in the window asks for a goal first (PLAN §7). Off
    /// unless it's been turned on.
    pub fn asks_goal(&self) -> bool {
        self.rest.get("askGoal").and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn set_asks_goal(&mut self, on: bool) {
        self.rest.insert("askGoal".into(), on.into());
    }

    /// Whether the profile tames, redirects or restricts any site or page, so it needs the
    /// browser extension. A profile for writing in a desktop app might not.
    pub fn tames_sites(&self) -> bool {
        let changes = |rule: &Value| {
            rule.get("level")
                .and_then(Value::as_str)
                .is_some_and(|l| l != "allowed")
        };
        let sites = self.rest.get("sites").and_then(Value::as_array);
        sites.into_iter().flatten().any(|site| {
            let pages = site.get("paths").and_then(Value::as_array);
            changes(site) || pages.into_iter().flatten().any(changes)
        })
    }
}

/// Whether `color` is a hex color like "#f5c86b", the only kind a star can be.
pub fn is_star_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// The profiles every new install starts with, shared with the extension.
pub fn starter_profiles() -> Vec<Profile> {
    serde_json::from_str(include_str!("../../../../packages/core/src/starter-profiles.json"))
        .expect("starter profiles are valid")
}

/// The running session. Like profiles, fields the app doesn't use (the extension's toggle
/// overrides and passes) are kept as they are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub profile_id: String,
    /// Epoch milliseconds.
    pub started_at: i64,
    /// The timer for this session, copied from its profile when it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer: Option<TimerSettings>,
    /// A copy of its profile, taken when it started, whose rules it runs by. Changes made on this
    /// computer reach it too; ones synced from other computers don't, so they can't change what's
    /// running. Sessions from before there was a copy use their profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Profile>,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Session {
    /// How hard it is to end the session at `now`: its strictness while the timer is running,
    /// and nothing once it's done or if there's no timer.
    pub fn lock(&self, now: i64) -> Option<Strictness> {
        let timer = self.timer.as_ref()?;
        let done_at = timer.done_at(self.started_at)?;
        (now < done_at).then_some(timer.strictness)
    }
}

/// Why a session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EndReason {
    /// The user ended it.
    Stopped,
    /// The user ended a Hard session early with an emergency pass.
    EmergencyPass,
    /// There was no keyboard or mouse input for a while.
    Idle,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            EndReason::Stopped => "stopped",
            EndReason::EmergencyPass => "emergencyPass",
            EndReason::Idle => "idle",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stopped" => Some(EndReason::Stopped),
            "emergencyPass" => Some(EndReason::EmergencyPass),
            "idle" => Some(EndReason::Idle),
            _ => None,
        }
    }
}

/// A one-line goal for a session (PLAN §7). It's kept from when the session starts until "Did
/// you do it?" is answered after it, or the next session starts. Only the app has it; it isn't
/// synced with the extension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionGoal {
    pub session_id: String,
    pub profile_id: String,
    pub text: String,
}

impl SessionGoal {
    /// A goal for this session, trimmed and cut to length, or none if it's empty.
    pub fn new(session: &Session, text: &str) -> Option<Self> {
        let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        truncate(&mut text, MAX_GOAL_LENGTH);
        (!text.is_empty()).then(|| Self {
            session_id: session.id.clone(),
            profile_id: session.profile_id.clone(),
            text,
        })
    }
}

/// A finished session, as kept in the session log. It holds everything the night sky will
/// need later, so stars and stardust can be worked out from past sessions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionRecord {
    pub id: String,
    pub profile_id: String,
    /// The profile's name when the session ended, in case it's renamed or deleted later.
    pub profile_name: String,
    /// Epoch milliseconds.
    pub started_at: i64,
    pub ended_at: i64,
    /// The session's timer length, if it had one.
    pub timer_minutes: Option<u32>,
    /// Time spent before the timer was done, or the whole session if it had no timer.
    pub focused_ms: i64,
    /// Time spent after the timer was done.
    pub overtime_ms: i64,
    /// Time away at the end, after the last keyboard or mouse input, which doesn't count
    /// towards focus or overtime. Only sessions that end for being idle have one.
    pub idle_tail_ms: i64,
    pub end_reason: EndReason,
    /// Whether a browser had to be closed during a Hard session for running without the
    /// extension, which no longer happens. Sessions logged as broken before then stay that way,
    /// and earn no rewards.
    pub broken: bool,
    /// The goal set when it started, if its profile asked for one and it wasn't skipped.
    pub goal: Option<String>,
    /// The answer to "Did you do it?", if it was answered.
    pub goal_met: Option<bool>,
}

impl SessionRecord {
    pub fn new(session: &Session, profile_name: &str, ended_at: i64, end_reason: EndReason) -> Self {
        Self::counted_until(session, profile_name, ended_at, ended_at, end_reason)
    }

    /// A session that ended for being idle: only the time up to the last input counts.
    pub fn idle(session: &Session, profile_name: &str, last_input: i64, ended_at: i64) -> Self {
        Self::counted_until(session, profile_name, last_input, ended_at, EndReason::Idle)
    }

    fn counted_until(
        session: &Session,
        profile_name: &str,
        counted: i64,
        ended_at: i64,
        end_reason: EndReason,
    ) -> Self {
        let ended_at = ended_at.max(session.started_at);
        let counted = counted.clamp(session.started_at, ended_at);
        let timer_minutes = session.timer.as_ref().map(|t| t.minutes).filter(|&m| m > 0);
        let done_at = session.timer.as_ref().and_then(|t| t.done_at(session.started_at));
        let split = done_at.map_or(counted, |done| done.min(counted));
        Self {
            id: session.id.clone(),
            profile_id: session.profile_id.clone(),
            profile_name: profile_name.to_owned(),
            started_at: session.started_at,
            ended_at,
            timer_minutes,
            focused_ms: split - session.started_at,
            overtime_ms: counted - split,
            idle_tail_ms: ended_at - counted,
            end_reason,
            broken: false,
            goal: None,
            goal_met: None,
        }
    }
}

/// One trip to a redirected site, as the extension reports it. The extension has its own
/// `GateVisit` type, so this one is `GateVisitRecord` in TypeScript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "GateVisitRecord")]
pub struct GateVisit {
    pub id: String,
    pub recipe_id: String,
    /// Epoch milliseconds.
    pub at: i64,
    /// The page the visit landed on, e.g. "gate" or "quote".
    pub destination: Option<String>,
    /// The one-tap answer to "why are you here?".
    pub why: Option<String>,
    /// What the user said they came for.
    pub intention: Option<String>,
    /// Whether they continued to the site with a pass.
    #[serde(default)]
    pub continued: bool,
}

/// The longest text kept from a gate visit. The extension already limits it; this is a backstop.
const MAX_VISIT_TEXT: usize = 500;

impl GateVisit {
    /// The same visit with its text fields cut down to a sensible length.
    pub fn trimmed(mut self) -> Self {
        for text in [&mut self.id, &mut self.recipe_id] {
            truncate(text, 100);
        }
        for text in [&mut self.destination, &mut self.why, &mut self.intention]
            .into_iter()
            .flatten()
        {
            truncate(text, MAX_VISIT_TEXT);
        }
        self
    }
}

pub(crate) fn truncate(text: &mut String, max_chars: usize) {
    if let Some((index, _)) = text.char_indices().nth(max_chars) {
        text.truncate(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MIN: i64 = 60_000;

    fn session(timer_minutes: Option<u32>) -> Session {
        Session {
            id: "s1".into(),
            profile_id: "writing".into(),
            started_at: 1_000_000,
            timer: timer_minutes.map(|minutes| TimerSettings {
                minutes,
                ..TimerSettings::default()
            }),
            rules: None,
            rest: Map::new(),
        }
    }

    #[test]
    fn keeps_fields_it_does_not_know() {
        let saved = json!({
            "id": "s1",
            "profileId": "writing",
            "startedAt": 5,
            "passes": { "reddit": 99 },
            "overrides": { "youtube": { "grayscale": false } }
        });
        let parsed: Session = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(parsed.timer, None);
        assert_eq!(serde_json::to_value(&parsed).unwrap(), saved);

        let profile = json!({ "id": "p", "name": "P", "sites": [{ "recipeId": "x", "level": "redirected" }] });
        let parsed: Profile = serde_json::from_value(profile.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), profile);
    }

    #[test]
    fn knows_which_profiles_tame_sites() {
        let profile = |sites: Value| -> Profile {
            serde_json::from_value(json!({ "id": "p", "name": "P", "sites": sites })).unwrap()
        };
        assert!(!profile(json!([])).tames_sites());
        assert!(!profile(json!([{ "recipeId": "reddit", "level": "allowed" }])).tames_sites());
        assert!(profile(json!([{ "recipeId": "reddit", "level": "tamed" }])).tames_sites());
        assert!(
            profile(json!([{ "recipeId": "youtube", "level": "allowed", "paths": [{ "path": "/shorts", "level": "redirected" }] }]))
                .tames_sites()
        );
        let no_sites: Profile = serde_json::from_value(json!({ "id": "p", "name": "P" })).unwrap();
        assert!(!no_sites.tames_sites());
        assert!(starter_profiles().iter().all(Profile::tames_sites));
    }

    #[test]
    fn sets_the_plan_and_keeps_the_rest_of_the_redirect_settings() {
        let mut profile: Profile = serde_json::from_value(json!({
            "id": "p",
            "name": "P",
            "redirect": { "gate": { "passMinutes": 10 }, "links": [] }
        }))
        .unwrap();
        assert_eq!(profile.plan(), "");

        profile.set_plan("  When I want Reddit, I'll write one paragraph first. ");
        assert_eq!(profile.plan(), "When I want Reddit, I'll write one paragraph first.");
        assert_eq!(profile.rest["redirect"]["gate"], json!({ "passMinutes": 10 }));
        assert_eq!(profile.rest["redirect"]["links"], json!([]));

        profile.set_plan(&"é".repeat(MAX_PLAN_LENGTH + 10));
        assert_eq!(profile.plan().chars().count(), MAX_PLAN_LENGTH);

        // A profile without redirect settings, or with ones that can't be read, gets them.
        for redirect in [json!(null), json!("x")] {
            let mut profile: Profile = serde_json::from_value(json!({ "id": "p", "name": "P" })).unwrap();
            if !redirect.is_null() {
                profile.rest.insert("redirect".into(), redirect);
            }
            profile.set_plan("Plan");
            assert_eq!(profile.rest["redirect"], json!({ "plan": "Plan" }));
        }
    }

    #[test]
    fn loads_the_starter_profiles() {
        let profiles = starter_profiles();
        let names: Vec<_> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Writing", "Deep Work", "Wind Down"]);
        assert_eq!(profiles[0].timer().minutes, 45);
    }

    #[test]
    fn splits_focus_from_overtime() {
        let record = SessionRecord::new(&session(Some(25)), "Writing", 1_000_000 + 40 * MIN, EndReason::Stopped);
        assert_eq!(record.timer_minutes, Some(25));
        assert_eq!(record.focused_ms, 25 * MIN);
        assert_eq!(record.overtime_ms, 15 * MIN);

        let early = SessionRecord::new(&session(Some(25)), "Writing", 1_000_000 + 10 * MIN, EndReason::Stopped);
        assert_eq!((early.focused_ms, early.overtime_ms), (10 * MIN, 0));
    }

    #[test]
    fn leaves_out_the_idle_tail() {
        let start = 1_000_000;
        // A 25-minute timer, last input at 40 minutes, ended for being idle at 55.
        let record = SessionRecord::idle(&session(Some(25)), "Writing", start + 40 * MIN, start + 55 * MIN);
        assert_eq!(record.end_reason, EndReason::Idle);
        assert_eq!(record.ended_at, start + 55 * MIN);
        assert_eq!(
            (record.focused_ms, record.overtime_ms, record.idle_tail_ms),
            (25 * MIN, 15 * MIN, 15 * MIN)
        );

        // Away before the timer was done: no overtime, and focus stops at the last input.
        let early = SessionRecord::idle(&session(Some(25)), "Writing", start + 10 * MIN, start + 25 * MIN);
        assert_eq!(
            (early.focused_ms, early.overtime_ms, early.idle_tail_ms),
            (10 * MIN, 0, 15 * MIN)
        );
    }

    #[test]
    fn counts_all_of_it_as_focus_without_a_timer() {
        for timer in [None, Some(0)] {
            let record = SessionRecord::new(&session(timer), "Writing", 1_000_000 + 40 * MIN, EndReason::Stopped);
            assert_eq!(record.timer_minutes, None);
            assert_eq!((record.focused_ms, record.overtime_ms), (40 * MIN, 0));
        }
    }

    #[test]
    fn is_locked_only_while_the_timer_runs() {
        let mut hard = session(Some(25));
        hard.timer.as_mut().unwrap().strictness = Strictness::Hard;
        assert_eq!(hard.lock(1_000_000 + 24 * MIN), Some(Strictness::Hard));
        assert_eq!(hard.lock(1_000_000 + 25 * MIN), None);
        assert_eq!(session(Some(0)).lock(1_000_000), None);
        assert_eq!(session(None).lock(1_000_000), None);
    }

    #[test]
    fn trims_long_visit_text() {
        let visit: GateVisit = serde_json::from_value(json!({
            "id": "v", "recipeId": "reddit", "at": 1, "intention": "é".repeat(600)
        }))
        .unwrap();
        assert!(!visit.continued);
        assert_eq!(visit.trimmed().intention.unwrap().chars().count(), MAX_VISIT_TEXT);
    }

    #[test]
    fn never_runs_backwards() {
        let record = SessionRecord::new(&session(Some(25)), "Writing", 0, EndReason::Stopped);
        assert_eq!(
            (record.ended_at, record.focused_ms, record.overtime_ms),
            (1_000_000, 0, 0)
        );
    }
}
