//! Keeping the browsers honest during a session. Each browser with the Seiza extension
//! starts a bridge that links it to the app (link.rs), and that link lasts exactly as long as
//! the extension runs. So a browser that's running without a link during a session has had
//! the extension turned off or removed, and its sites aren't being tamed.
//!
//! Only sessions whose profile tames sites are watched; a session spent in other apps doesn't
//! need the browser. What happens then is a warning, unless "close browsers" is on in
//! Settings (it's opt-in, so the app and extension can each be used alone). With it on, it
//! depends on the session's strictness:
//! - Gentle, or no timer running: a warning.
//! - Medium and Hard: a warning, then the browser is closed a minute later. The session
//!   carries on as it was; nothing counts against it.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::processes::Running;
use crate::timer::Strictness;

/// A browser that has linked to the app, so it's known to have the extension.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnownBrowser {
    pub name: String,
    /// The browser's executable, which is how it's recognized when it runs.
    pub path: String,
}

/// How long a browser can go without its link before it's a problem. The extension relinks
/// within a few seconds when it reloads or updates, or when the app or browser starts.
const GRACE_MS: i64 = 20_000;
/// How long after the warning a browser is closed.
const CLOSE_AFTER_MS: i64 = 60_000;

/// What to do about a browser running without the extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Warn { name: String, response: Response },
    Close { pid: u32, name: String },
}

/// How far a session goes when a browser runs without the extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    Warn,
    Close,
}

impl Response {
    /// The response for a session's strictness while its timer is running (`Session::lock`),
    /// and whether closing browsers is turned on.
    pub fn to(lock: Option<Strictness>, close_browsers: bool) -> Self {
        if !close_browsers {
            return Response::Warn;
        }
        match lock {
            None | Some(Strictness::Gentle) => Response::Warn,
            Some(Strictness::Medium | Strictness::Hard) => Response::Close,
        }
    }
}

/// When a browser was first seen without its link, and when it was last warned or closed.
struct Quiet {
    since: i64,
    acted_at: Option<i64>,
}

/// The browsers seen running without their link this session, by process id.
#[derive(Default)]
pub struct BrowserWatch {
    quiet: HashMap<u32, Quiet>,
}

impl BrowserWatch {
    /// When the next step for a browser without its link is due (epoch ms): its warning, or
    /// closing it. The session's watch wakes up then to check again.
    pub fn next_due(&self, response: Response) -> Option<i64> {
        self.quiet
            .values()
            .filter_map(|quiet| match quiet.acted_at {
                None => Some(quiet.since + GRACE_MS),
                Some(at) if response != Response::Warn => Some(at + CLOSE_AFTER_MS),
                Some(_) => None,
            })
            .min()
    }

    /// Takes the known browsers that are running and the process ids of the browsers that are
    /// linked, and returns what to do now.
    pub fn check(&mut self, now: i64, running: &[Running], linked: &HashSet<u32>, response: Response) -> Vec<Action> {
        let unlinked: Vec<&Running> = running.iter().filter(|b| !linked.contains(&b.pid)).collect();
        // A browser that linked again, or quit, starts over.
        self.quiet.retain(|pid, _| unlinked.iter().any(|b| b.pid == *pid));

        let mut actions = Vec::new();
        for browser in unlinked {
            let quiet = self.quiet.entry(browser.pid).or_insert(Quiet {
                since: now,
                acted_at: None,
            });
            match quiet.acted_at {
                None if now - quiet.since >= GRACE_MS => {
                    quiet.acted_at = Some(now);
                    actions.push(Action::Warn {
                        name: browser.name.clone(),
                        response,
                    });
                }
                // If it's still running a minute after being asked to quit, it's asked again.
                Some(at) if response != Response::Warn && now - at >= CLOSE_AFTER_MS => {
                    quiet.acted_at = Some(now);
                    actions.push(Action::Close {
                        pid: browser.pid,
                        name: browser.name.clone(),
                    });
                }
                _ => {}
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: i64 = 1_000;
    /// How often these tests check, as Linux does during a session.
    const CHECK_EVERY_MS: i64 = 10_000;

    fn chrome() -> Running {
        Running {
            pid: 7,
            path: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
            name: "Google Chrome".into(),
        }
    }

    fn linked(pids: &[u32]) -> HashSet<u32> {
        pids.iter().copied().collect()
    }

    /// Checks every 10 seconds from 0 to `until`, and returns each action with when it happened.
    fn run(
        watch: &mut BrowserWatch,
        until: i64,
        linked: &HashSet<u32>,
        lock: Option<Strictness>,
    ) -> Vec<(i64, Action)> {
        (0..=until / CHECK_EVERY_MS)
            .map(|i| i * CHECK_EVERY_MS)
            .flat_map(|now| {
                watch
                    .check(now, &[chrome()], linked, Response::to(lock, true))
                    .into_iter()
                    .map(move |a| (now, a))
            })
            .collect()
    }

    #[test]
    fn leaves_linked_browsers_alone() {
        let mut watch = BrowserWatch::default();
        assert_eq!(
            run(&mut watch, 300 * SEC, &linked(&[7]), Some(Strictness::Hard)),
            vec![]
        );
    }

    #[test]
    fn gentle_only_warns() {
        let mut watch = BrowserWatch::default();
        let actions = run(&mut watch, 300 * SEC, &linked(&[]), Some(Strictness::Gentle));
        assert_eq!(
            actions,
            vec![(
                20 * SEC,
                Action::Warn {
                    name: "Google Chrome".into(),
                    response: Response::Warn
                }
            )]
        );
        // So does a session with no timer running.
        let mut watch = BrowserWatch::default();
        assert_eq!(run(&mut watch, 300 * SEC, &linked(&[]), None).len(), 1);
    }

    #[test]
    fn medium_closes_the_browser_a_minute_after_warning() {
        let mut watch = BrowserWatch::default();
        let actions = run(&mut watch, 90 * SEC, &linked(&[]), Some(Strictness::Medium));
        assert_eq!(
            actions,
            vec![
                (
                    20 * SEC,
                    Action::Warn {
                        name: "Google Chrome".into(),
                        response: Response::Close
                    }
                ),
                (
                    80 * SEC,
                    Action::Close {
                        pid: 7,
                        name: "Google Chrome".into(),
                    }
                ),
            ]
        );
    }

    #[test]
    fn hard_closes_the_browser_like_medium() {
        let mut watch = BrowserWatch::default();
        let hard = run(&mut watch, 90 * SEC, &linked(&[]), Some(Strictness::Hard));
        let mut watch = BrowserWatch::default();
        let medium = run(&mut watch, 90 * SEC, &linked(&[]), Some(Strictness::Medium));
        assert_eq!(hard, medium);
    }

    #[test]
    fn only_warns_unless_closing_is_turned_on() {
        let mut watch = BrowserWatch::default();
        let none = linked(&[]);
        let response = Response::to(Some(Strictness::Hard), false);
        assert_eq!(response, Response::Warn);
        let actions: Vec<_> = (0..=30)
            .flat_map(|i| watch.check(i * CHECK_EVERY_MS, &[chrome()], &none, response))
            .collect();
        assert!(matches!(actions.as_slice(), [Action::Warn { .. }]));
    }

    #[test]
    fn says_when_the_next_step_is_due() {
        let mut watch = BrowserWatch::default();
        let none = linked(&[]);
        let medium = Response::to(Some(Strictness::Medium), true);
        assert_eq!(watch.next_due(medium), None);
        watch.check(5 * SEC, &[chrome()], &none, medium);
        assert_eq!(watch.next_due(medium), Some(25 * SEC));
        // Checking exactly then warns, and the close is due a minute later.
        assert_eq!(watch.check(25 * SEC, &[chrome()], &none, medium).len(), 1);
        assert_eq!(watch.next_due(medium), Some(85 * SEC));
        // With only a warning to give, nothing more is due once it's given.
        assert_eq!(watch.next_due(Response::Warn), None);
        // Linked again: nothing due.
        watch.check(30 * SEC, &[chrome()], &linked(&[7]), medium);
        assert_eq!(watch.next_due(medium), None);
    }

    #[test]
    fn linking_again_in_time_starts_over() {
        let mut watch = BrowserWatch::default();
        let none = linked(&[]);
        let lock = Some(Strictness::Medium);
        assert_eq!(watch.check(0, &[chrome()], &none, Response::to(lock, true)), vec![]);
        assert_eq!(
            watch
                .check(20 * SEC, &[chrome()], &none, Response::to(lock, true))
                .len(),
            1
        );
        // The extension is turned back on before the browser closes.
        assert_eq!(
            watch.check(30 * SEC, &[chrome()], &linked(&[7]), Response::to(lock, true)),
            vec![]
        );
        // Turned off again later: a fresh warning, not an instant close.
        assert_eq!(
            watch.check(100 * SEC, &[chrome()], &none, Response::to(lock, true)),
            vec![]
        );
        assert!(matches!(
            watch
                .check(120 * SEC, &[chrome()], &none, Response::to(lock, true))
                .as_slice(),
            [Action::Warn { .. }]
        ));
    }

    #[test]
    fn does_not_close_once_the_timer_is_done() {
        let mut watch = BrowserWatch::default();
        let none = linked(&[]);
        watch.check(0, &[chrome()], &none, Response::to(Some(Strictness::Hard), true));
        assert_eq!(
            watch
                .check(20 * SEC, &[chrome()], &none, Response::to(Some(Strictness::Hard), true))
                .len(),
            1
        );
        // The timer runs out before the minute is up, so ending is free and nothing closes.
        assert_eq!(
            watch.check(90 * SEC, &[chrome()], &none, Response::to(None, true)),
            vec![]
        );
    }
}
