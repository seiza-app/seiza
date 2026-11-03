// Live sessions' quiet connection (PLAN §8, Sync). A device keeps one `GET /v1/live` open, and the
// server tells it when another device in its group uploads a batch, so nothing polls, and passes
// on short messages between the group's devices (`POST /v1/live`), sealed by the app like
// everything else. Messages aren't kept: a device that isn't connected doesn't get them.
//
// It's a stream of server-sent events, plain HTTP that reverse proxies and Cloudflare Tunnels pass
// on as they are, with a comment every 40 seconds so none of them closes it for being quiet:
//
//   event: ready      once connected; the device fetches what it missed
//   event: batch      data {"seq": N}, or {} when it may have missed some
//   event: message    data {"deviceId": "…", "data": "<base64>"}

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use serde_json::json;
use tokio::sync::{broadcast, watch};

use crate::db::DeviceRef;
use crate::error::ApiError;

/// How often a quiet connection gets a comment. Under the 60 seconds after which nginx, for one,
/// closes a connection that's said nothing, and Cloudflare's 100.
pub const HEARTBEAT: Duration = Duration::from_secs(40);
/// How often a connection checks that its device is still in the group, which also keeps the
/// device's "last seen" current while it's connected.
pub const CHECK_EVERY: Duration = Duration::from_secs(5 * 60);
/// The most connections one group can have open, and the server as a whole.
const MAX_PER_GROUP: usize = 16;
const MAX_CONNECTIONS: usize = 5_000;
/// Events waiting for a slow connection before it's told it may have missed some.
const BACKLOG: usize = 64;

#[derive(Clone, Debug)]
pub enum Note {
    /// A device uploaded a batch.
    Batch { seq: i64, from: String },
    /// A device sent the others a message (base64).
    Message { from: String, data: String },
    /// A device connected again, so its older connection (one left behind by sleep, say) can go.
    Connected { device: String, connection: u64 },
    /// A device was removed from the group.
    Removed { device: String },
}

/// The open connections, by group.
pub struct Hub {
    groups: Mutex<HashMap<String, broadcast::Sender<Note>>>,
    open: AtomicUsize,
    next_connection: AtomicU64,
    closing: watch::Sender<bool>,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            groups: Mutex::default(),
            open: AtomicUsize::new(0),
            next_connection: AtomicU64::new(1),
            closing: watch::Sender::new(false),
        }
    }
}

impl Hub {
    fn groups(&self) -> std::sync::MutexGuard<'_, HashMap<String, broadcast::Sender<Note>>> {
        self.groups.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Tells the group's connections, if it has any.
    pub fn tell(&self, group_id: &str, note: Note) {
        let mut groups = self.groups();
        if let Some(sender) = groups.get(group_id)
            && sender.send(note).is_err()
        {
            // No one's listening any more.
            groups.remove(group_id);
        }
    }

    /// Ends every connection, so the server can shut down.
    pub fn close(&self) {
        self.closing.send_replace(true);
    }

    /// Opens a connection for a device, ending its older ones.
    fn connect(self: &Arc<Self>, device: &DeviceRef) -> Result<Connection, ApiError> {
        let receiver = {
            let mut groups = self.groups();
            let sender = groups
                .entry(device.group_id.clone())
                .or_insert_with(|| broadcast::channel(BACKLOG).0);
            if sender.receiver_count() >= MAX_PER_GROUP || self.open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                return Err(ApiError::TooManyRequests);
            }
            sender.subscribe()
        };
        self.open.fetch_add(1, Ordering::Relaxed);
        let id = self.next_connection.fetch_add(1, Ordering::Relaxed);
        self.tell(
            &device.group_id,
            Note::Connected {
                device: device.id.clone(),
                connection: id,
            },
        );
        Ok(Connection {
            hub: Arc::clone(self),
            id,
            receiver,
            closing: self.closing.subscribe(),
        })
    }
}

/// One device's open connection. Dropping it, when the device goes, counts it out.
struct Connection {
    hub: Arc<Hub>,
    id: u64,
    receiver: broadcast::Receiver<Note>,
    closing: watch::Receiver<bool>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.hub.open.fetch_sub(1, Ordering::Relaxed);
    }
}

/// What a connection does next.
enum Next {
    Send(Event),
    /// Nothing to send this time.
    Skip,
    End,
}

/// What a connection does with a note to its group.
fn next(note: Result<Note, broadcast::error::RecvError>, device: &str, connection: u64) -> Next {
    match note {
        Ok(Note::Batch { seq, from }) if from != device => Next::Send(event("batch", json!({ "seq": seq }))),
        Ok(Note::Message { from, data }) if from != device => {
            Next::Send(event("message", json!({ "deviceId": from, "data": data })))
        }
        Ok(Note::Connected {
            device: d,
            connection: c,
        }) if d == device && c != connection => Next::End,
        Ok(Note::Removed { device: d }) if d == device => Next::End,
        Ok(_) => Next::Skip,
        // It fell behind, so it fetches to catch up.
        Err(broadcast::error::RecvError::Lagged(_)) => Next::Send(event("batch", json!({}))),
        Err(broadcast::error::RecvError::Closed) => Next::End,
    }
}

fn event(name: &str, data: serde_json::Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

/// Where a device's stream is up to.
struct Listening<F> {
    connection: Connection,
    device: String,
    ready_sent: bool,
    check_at: tokio::time::Instant,
    still_in: F,
}

/// The stream of events for a device. `still_in` checks that the device is still in its group
/// (and notes that it was seen), every `CHECK_EVERY`.
pub fn listen<F, Fut>(
    hub: &Arc<Hub>,
    device: DeviceRef,
    still_in: F,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + use<F, Fut>>, ApiError>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = bool> + Send,
{
    let start = Listening {
        connection: hub.connect(&device)?,
        device: device.id,
        ready_sent: false,
        check_at: tokio::time::Instant::now() + CHECK_EVERY,
        still_in,
    };
    let stream = futures_util::stream::unfold(start, |mut at| async move {
        if !at.ready_sent {
            at.ready_sent = true;
            return Some((Ok(event("ready", json!({}))), at));
        }
        loop {
            if *at.connection.closing.borrow() {
                return None;
            }
            let next = tokio::select! {
                note = at.connection.receiver.recv() => next(note, &at.device, at.connection.id),
                _ = at.connection.closing.changed() => Next::End,
                () = tokio::time::sleep_until(at.check_at) => {
                    at.check_at = tokio::time::Instant::now() + CHECK_EVERY;
                    if (at.still_in)().await { Next::Skip } else { Next::End }
                }
            };
            match next {
                Next::Send(event) => return Some((Ok(event), at)),
                Next::Skip => continue,
                Next::End => return None,
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(HEARTBEAT)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> DeviceRef {
        DeviceRef {
            id: id.into(),
            group_id: "g".into(),
        }
    }

    #[test]
    fn a_device_hears_the_others_but_not_itself() {
        let batch = |from: &str| {
            Ok(Note::Batch {
                seq: 3,
                from: from.into(),
            })
        };
        assert!(matches!(next(batch("b"), "a", 1), Next::Send(_)));
        assert!(matches!(next(batch("a"), "a", 1), Next::Skip));
        let message = Ok(Note::Message {
            from: "a".into(),
            data: "x".into(),
        });
        assert!(matches!(next(message, "a", 1), Next::Skip));
    }

    #[test]
    fn a_newer_connection_or_removal_ends_one() {
        let connected = |connection| {
            Ok(Note::Connected {
                device: "a".into(),
                connection,
            })
        };
        assert!(matches!(next(connected(1), "a", 1), Next::Skip));
        assert!(matches!(next(connected(2), "a", 1), Next::End));
        assert!(matches!(next(connected(2), "b", 1), Next::Skip));
        let removed = |d: &str| Ok(Note::Removed { device: d.into() });
        assert!(matches!(next(removed("a"), "a", 1), Next::End));
        assert!(matches!(next(removed("b"), "a", 1), Next::Skip));
        let lagged = Err(broadcast::error::RecvError::Lagged(5));
        assert!(matches!(next(lagged, "a", 1), Next::Send(_)));
    }

    #[test]
    fn groups_have_a_cap_and_connections_count_out_when_they_go() {
        let hub = Arc::new(Hub::default());
        let mut open: Vec<_> = (0..MAX_PER_GROUP)
            .map(|i| hub.connect(&device(&format!("d{i}"))).ok().unwrap())
            .collect();
        assert!(hub.connect(&device("one more")).is_err());
        open.pop();
        assert!(hub.connect(&device("one more")).is_ok());
        drop(open);
        assert_eq!(hub.open.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn telling_a_group_no_one_listens_to_forgets_it() {
        let hub = Arc::new(Hub::default());
        drop(hub.connect(&device("a")).ok().unwrap());
        hub.tell("g", Note::Removed { device: "a".into() });
        assert!(hub.groups().is_empty());
    }
}
