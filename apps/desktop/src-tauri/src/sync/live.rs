// The quiet connection to the sync server (PLAN §8, Sync, live sessions). While this computer
// syncs, it keeps one connection open, and the server says when another computer in the group
// has sent something, so a session started there starts here within seconds, and nothing polls.
// When it drops (the computer slept, the network went), it connects again, sooner once the
// computer wakes, and fetches whatever came meanwhile. A server from before live sessions doesn't
// have one, and sync goes on without it, at the moments in exchange.rs.
//
// The connection also carries short messages between the group's connected computers, sealed
// like everything else and never kept by the server. There's one so far: before a session ends
// for being idle, the computer about to end it asks the others whether anyone's been using
// theirs, so a session on several computers only ends once all of them have been idle.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use super::client::{Client, ClientError, LiveEvent};
use super::crypto::{GroupKey, Purpose};
use super::settings::SyncSettings;

/// How long to wait before connecting again, at first and at most, doubling in between.
const FIRST_RETRY: Duration = Duration::from_secs(5);
const LAST_RETRY: Duration = Duration::from_secs(5 * 60);
/// How long the computer about to end an idle session waits for the others to answer.
const ANSWERS_WAIT: Duration = Duration::from_secs(5);

/// The connection, and what it's for.
#[derive(Default)]
pub struct Listener(Mutex<Listening>);

#[derive(Default)]
struct Listening {
    /// The server and device the connection is for, and its task.
    task: Option<(String, JoinHandle<()>)>,
    /// Connects again now: the computer woke, so the connection may be gone without a word.
    again: Arc<Notify>,
    /// Questions waiting for answers: what the others said, by question.
    asked: HashMap<String, Vec<i64>>,
}

fn listening(app: &AppHandle) -> MutexGuard<'_, Listening> {
    app.state::<Listener>()
        .inner()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Connects, or stops, to match this computer's sync settings: when the app starts, after it
/// starts a group, joins, leaves or moves, and after each exchange, which connects again to a
/// server that had no live connection before it was updated.
pub fn watch(app: &AppHandle) {
    if app.try_state::<Listener>().is_none() {
        return;
    }
    let settings = crate::engine(app).sync_settings().ok().flatten();
    let key = settings.as_ref().map(|s| format!("{}\n{}", s.server, s.device_id));
    let mut listening = listening(app);
    let running = listening.task.as_ref().filter(|(_, task)| !task.inner().is_finished());
    if running.map(|(k, _)| k) == key.as_ref() {
        return;
    }
    if let Some((_, task)) = listening.task.take() {
        task.abort();
    }
    let (Some(settings), Some(key)) = (settings, key) else {
        return;
    };
    let again = Arc::clone(&listening.again);
    let task = tauri::async_runtime::spawn(listen(app.clone(), settings, again));
    listening.task = Some((key, task));
}

/// The computer woke from sleep: the connection is probably gone, so connect again now.
pub fn woke(app: &AppHandle) {
    if app.try_state::<Listener>().is_some() {
        listening(app).again.notify_one();
    }
}

async fn listen(app: AppHandle, settings: SyncSettings, again: Arc<Notify>) {
    let (Ok(client), Some(key)) = (Client::new(&settings.server), settings.group_key()) else {
        return;
    };
    let mut wait = FIRST_RETRY;
    loop {
        match client.listen(&settings.token).await {
            Ok(mut live) => loop {
                let event = tokio::select! {
                    event = live.next() => event,
                    () = again.notified() => break,
                };
                match event {
                    // Connected: fetch whatever came while it wasn't.
                    Ok(Some(LiveEvent::Ready)) => {
                        wait = FIRST_RETRY;
                        super::exchange::fetch(&app);
                    }
                    Ok(Some(LiveEvent::Batch)) => super::exchange::fetch(&app),
                    Ok(Some(LiveEvent::Message { data, .. })) => heard(&app, &client, &settings, &key, &data),
                    Ok(None) | Err(_) => break,
                }
            },
            // Removed from the group: a fetch finds out the same way and turns sync off here.
            Err(error) if error.is_signed_out() => {
                super::exchange::fetch(&app);
                return;
            }
            // A server from before live sessions.
            Err(ClientError::Refused {
                status: StatusCode::NOT_FOUND,
                ..
            }) => return,
            Err(_) => {}
        }
        // Waiting for the network or the server to come back, or for the computer to wake.
        let _ = tokio::time::timeout(wait, again.notified()).await;
        wait = (wait * 2).min(LAST_RETRY);
    }
}

/// A message between the group's computers, sealed with `Purpose::Live`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
enum Message {
    /// "Has anyone used their computer lately?", about to end this session for being idle.
    Idle { session: String, ask: String },
    /// How long it's been since the last input on a computer running that session.
    Active { ask: String, idle_ms: i64 },
}

fn seal(key: &GroupKey, settings: &SyncSettings, message: &Message) -> Vec<u8> {
    let json = serde_json::to_vec(message).expect("messages serialize");
    key.seal(Purpose::Live, &settings.group_id, &json)
}

fn open(key: &GroupKey, settings: &SyncSettings, sealed: &[u8]) -> Option<Message> {
    let json = key.open(Purpose::Live, &settings.group_id, sealed).ok()?;
    serde_json::from_slice(&json).ok()
}

/// Answers another computer's question, or notes its answer to this one's.
fn heard(app: &AppHandle, client: &Client, settings: &SyncSettings, key: &GroupKey, sealed: &[u8]) {
    match open(key, settings, sealed) {
        Some(Message::Idle { session, ask }) => {
            let running = crate::engine(app).session().is_some_and(|s| s.id == session);
            if !running {
                return;
            }
            let (client, settings, key) = (client.clone(), settings.clone(), key.clone());
            tauri::async_runtime::spawn(async move {
                let Some(idle_ms) = crate::idle_reading().await else {
                    return;
                };
                let answer = seal(&key, &settings, &Message::Active { ask, idle_ms });
                let _ = client.tell(&settings.token, answer).await;
            });
        }
        Some(Message::Active { ask, idle_ms }) => {
            if let Some(answers) = listening(app).asked.get_mut(&ask) {
                answers.push(idle_ms);
            }
        }
        None => {}
    }
}

/// When the last keyboard or mouse input was on the group's other computers running this
/// session (epoch ms here), from asking them, or None if none of them answered: they're asleep,
/// offline, idle-less or don't follow the group's sessions, which all count as idle.
pub async fn last_input_elsewhere(app: &AppHandle, session_id: &str) -> Option<i64> {
    let settings = crate::engine(app).sync_settings().ok().flatten()?;
    if !settings.follow {
        return None;
    }
    let key = settings.group_key()?;
    let client = Client::new(&settings.server).ok()?;
    let ask = uuid::Uuid::new_v4().to_string();
    listening(app).asked.insert(ask.clone(), Vec::new());
    let question = Message::Idle {
        session: session_id.to_owned(),
        ask: ask.clone(),
    };
    let asked = client.tell(&settings.token, seal(&key, &settings, &question)).await;
    if asked.is_ok() {
        tokio::time::sleep(ANSWERS_WAIT).await;
    }
    let answers = listening(app).asked.remove(&ask).unwrap_or_default();
    let least = answers.into_iter().filter(|&ms| ms >= 0).min()?;
    Some(crate::now_ms() - least)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::client::Membership;

    #[test]
    fn messages_open_only_in_their_group() {
        let key = GroupKey::generate();
        let settings = |group: &str| {
            SyncSettings::new(
                "https://sync.example.com".into(),
                Membership {
                    group_id: group.into(),
                    device_id: "d".into(),
                    token: "t".into(),
                },
                &key,
                "Mac".into(),
            )
        };
        let question = Message::Idle {
            session: "s".into(),
            ask: "a".into(),
        };
        let sealed = seal(&key, &settings("g"), &question);
        assert_eq!(open(&key, &settings("g"), &sealed), Some(question));
        assert_eq!(open(&key, &settings("other"), &sealed), None);
        // Nor is a batch a message.
        let batch = key.seal(Purpose::Batch, "g", b"{}");
        assert_eq!(open(&key, &settings("g"), &batch), None);
    }

    #[test]
    fn messages_look_like_this() {
        let answer = Message::Active {
            ask: "a".into(),
            idle_ms: 5,
        };
        assert_eq!(
            serde_json::to_value(&answer).unwrap(),
            serde_json::json!({ "active": { "ask": "a", "idleMs": 5 } })
        );
    }
}
