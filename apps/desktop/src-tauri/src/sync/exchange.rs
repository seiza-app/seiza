// Sending this computer's changes to the sync server and fetching the others' (PLAN §8, Sync,
// build steps 5 and 8). There are no timers: a send goes out after something changes here (a
// session starts or ends, a goal is answered, a constellation or profile changes), and a fetch
// when the window opens, when a session starts or ends, and when the server's live connection
// says something new has arrived (live.rs). Changes go as records.rs's records, in batches of
// `{"v": 1, "records": [...]}` compressed with zlib and sealed by the group's key.
//
// A batch can also say which session is running in the group, as `"live"`, so a session started
// on one computer starts on the others that follow (Settings › Sync), and ending it on one ends
// it everywhere. The computer that ends it logs it; the others only stop it. Older versions of
// the app leave `"live"` out, as they would any field they don't know.
//
// One exchange runs at a time. Asking for one while it runs has it go again once it's done, and
// each waits a moment first, so changes made together go out together.

use std::borrow::Cow;
use std::io::{Read, Write};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::client::{Client, ClientError};
use super::crypto::{GroupKey, Purpose};
use super::settings::{GroupSession, RunningSession, SyncSettings};
use super::setup::{emit_status, signed_in};
use crate::error::Error;
use crate::records::Record;

/// The batches' format. A batch in a newer one waits on the server until this app is updated,
/// so a new kind of record that older apps can't save should come with a new version.
const VERSION: u32 = 1;
/// Records asked for at a time, so the first send from a computer with a long log goes in a
/// few hundred batches at most.
const PAGE: usize = 500;
/// The largest batch the server takes, sealed (apps/sync-server's `MAX_BATCH_BYTES`).
const MAX_BATCH_BYTES: usize = 1024 * 1024;
/// What sealing adds: a version byte, the nonce and the tag (crypto.rs).
const SEALING_BYTES: usize = 1 + 24 + 16;
/// The most a batch can unpack to, so a bad one can't fill the memory.
const MAX_UNPACKED_BYTES: u64 = 64 * 1024 * 1024;
/// How long a send or fetch waits before going out, so changes made together go together.
const SETTLE: Duration = Duration::from_secs(2);

/// Whether an exchange is running, what's been asked for since it started, and why the last one
/// stopped.
#[derive(Default)]
pub struct Exchanges(Mutex<Pending>);

#[derive(Default)]
struct Pending {
    running: bool,
    send: bool,
    fetch: bool,
    /// Something the server or this app said, for Settings › Sync. Being offline isn't one:
    /// "Last synced" says enough, and the next moment to sync tries again.
    problem: Option<String>,
}

fn pending(app: &AppHandle) -> MutexGuard<'_, Pending> {
    app.state::<Exchanges>()
        .inner()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Sends what's changed here since the last send, if anything has and this computer syncs.
pub fn send(app: &AppHandle) {
    soon(app, false);
}

/// Fetches what the group's other computers sent, then sends what's changed here.
pub fn fetch(app: &AppHandle) {
    soon(app, true);
}

/// Sends what's changed here straight away, for when the app is about to quit.
pub async fn send_now(app: &AppHandle) {
    exchange(app, false).await;
}

/// Why the last exchange stopped, if it was something more than being offline.
pub fn problem(app: &AppHandle) -> Option<String> {
    pending(app).problem.clone()
}

fn soon(app: &AppHandle, fetch: bool) {
    // Before the app has finished starting.
    if app.try_state::<Exchanges>().is_none() {
        return;
    }
    {
        let mut pending = pending(app);
        pending.send = true;
        pending.fetch |= fetch;
        if pending.running {
            return;
        }
        pending.running = true;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(SETTLE).await;
            let fetch = {
                let mut pending = pending(&app);
                if !pending.send && !pending.fetch {
                    pending.running = false;
                    return;
                }
                pending.send = false;
                std::mem::take(&mut pending.fetch)
            };
            exchange(&app, fetch).await;
        }
    });
}

/// Why an exchange stopped early.
enum Stop {
    /// Offline, the server busy, or sync changed here meanwhile: the next moment tries again.
    Passing,
    Problem(Error),
}

impl From<Error> for Stop {
    fn from(error: Error) -> Self {
        Stop::Problem(error)
    }
}

type Result<T> = std::result::Result<T, Stop>;

async fn exchange(app: &AppHandle, fetch: bool) {
    let settings = match crate::engine(app).sync_settings() {
        Ok(Some(settings)) => settings,
        Ok(None) => return,
        Err(error) => return eprintln!("Seiza: couldn't read the sync settings: {error}"),
    };
    let problem = match run(app, &settings, fetch).await {
        Ok(()) => {
            super::live::watch(app);
            None
        }
        Err(Stop::Passing) => return,
        // Sync is off here now, and Settings › Sync says why.
        Err(Stop::Problem(Error::RemovedFromSync)) => None,
        Err(Stop::Problem(error)) => Some(error.to_string()),
    };
    let changed = {
        let mut pending = pending(app);
        std::mem::replace(&mut pending.problem, problem.clone()) != problem
    };
    if changed {
        emit_status(app);
    }
}

async fn run(app: &AppHandle, settings: &SyncSettings, fetch: bool) -> Result<()> {
    let key = settings.group_key().ok_or(Error::NotSyncing)?;
    let client = Client::new(&settings.server).map_err(Error::from)?;
    let mut settings = settings.clone();
    let newer = fetch && fetch_batches(app, &client, &mut settings, &key).await?;
    send_changes(app, &client, &mut settings, &key).await?;
    if newer {
        return Err(Stop::Problem(Error::Sync(
            "another computer has a newer version of Seiza, so some of what it synced waits here until this one \
             is updated"
                .into(),
        )));
    }
    Ok(())
}

/// What a request to the server got, sorted into what to do about it.
fn answered<T>(app: &AppHandle, settings: &SyncSettings, answer: std::result::Result<T, ClientError>) -> Result<T> {
    match answer {
        Err(ClientError::Offline(_)) => Err(Stop::Passing),
        Err(ClientError::Refused { status, .. })
            if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() =>
        {
            Err(Stop::Passing)
        }
        other => Ok(signed_in(app, settings, other)?),
    }
}

/// Saves how far sync got, onto this computer's settings as they are now, as long as it's still
/// in the same group as when the exchange started (it may have left or moved meanwhile).
fn advance(app: &AppHandle, settings: &mut SyncSettings, update: impl FnOnce(&mut SyncSettings)) -> Result<()> {
    let saved = {
        let engine = crate::engine(app);
        let Some(mut now) = engine.sync_settings()? else {
            return Err(Stop::Passing);
        };
        if (&now.group_id, &now.device_id, &now.token) != (&settings.group_id, &settings.device_id, &settings.token) {
            return Err(Stop::Passing);
        }
        update(&mut now);
        engine.set_sync_settings(Some(&now), crate::now_ms())?;
        now
    };
    *settings = saved;
    Ok(())
}

/// Fetches and saves the batches the group's other computers sent since the last fetch.
/// Returns whether it stopped at one from a newer version of Seiza.
async fn fetch_batches(app: &AppHandle, client: &Client, settings: &mut SyncSettings, key: &GroupKey) -> Result<bool> {
    let mut newer = false;
    loop {
        let page = client.batches_after(&settings.token, settings.fetched).await;
        let page = answered(app, settings, page)?;
        let mut records = Vec::new();
        let mut lives = Vec::new();
        let mut reached = settings.fetched;
        for batch in &page.batches {
            // This computer's own, sent back. Ones it sent under an earlier id (before it left and
            // joined again) are saved like any other's, and change nothing.
            if batch.device_id != settings.device_id {
                match open(key, &settings.group_id, &batch.data) {
                    Opened::Records(opened, live) => {
                        records.extend(opened);
                        lives.extend(live.map(|live| *live));
                    }
                    Opened::Newer => {
                        newer = true;
                        break;
                    }
                }
            }
            reached = batch.seq;
        }
        let now = crate::now_ms();
        let (applied, live, taken) = {
            let mut engine = crate::engine(app);
            // The log first, so a session that's already ended there isn't joined.
            let applied = engine.apply_records(&records, now)?;
            let (live, taken) = if settings.follow {
                take_lives(&mut engine, lives, now)?
            } else {
                // Not taken, but kept, so turning following on can join it (`join_live`).
                (lives.pop(), false)
            };
            (applied, live, taken)
        };
        // Told a page at a time, so what's saved shows even if a later page doesn't come.
        if applied > 0 || taken {
            crate::changed(app);
        }
        if applied > 0 {
            let _ = app.emit("synced", ());
        }
        advance(app, settings, |s| {
            s.fetched = reached;
            s.last_synced_at = Some(now);
            if live.is_some() {
                s.live = live;
            }
        })?;
        emit_status(app);
        if newer || !page.more || page.batches.is_empty() {
            return Ok(newer);
        }
    }
}

/// Brings the sessions other computers said are running or ended into this one, in the order
/// they were sent. Returns the last one, as the group's, and whether anything changed here.
fn take_lives(
    engine: &mut crate::engine::Engine,
    lives: Vec<GroupSession>,
    now: i64,
) -> std::result::Result<(Option<GroupSession>, bool), Error> {
    let mut changed = false;
    let mut last = None;
    for mut live in lives {
        changed |= match &mut live {
            GroupSession {
                running: Some(running), ..
            } => {
                running.session.rest.clear();
                engine.take_group_session(running.session.clone(), running.goal.clone(), now)?
            }
            GroupSession { ended: Some(id), .. } => engine.end_group_session(id, now)?,
            _ => false,
        };
        last = Some(live);
    }
    Ok((last, changed))
}

/// Joins the session the group last said was running, for a computer that's just started
/// following the group's sessions. Returns whether anything changed here.
pub(super) fn join_live(
    engine: &mut crate::engine::Engine,
    settings: &SyncSettings,
    now: i64,
) -> std::result::Result<bool, Error> {
    match settings.live.clone().filter(|live| live.running.is_some()) {
        Some(live) => Ok(take_lives(engine, vec![live], now)?.1),
        None => Ok(false),
    }
}

/// The session running here, to tell the group, or the one that ended here: once it's no longer
/// running, the last one the group heard of. None while it's on a computer that doesn't follow
/// the group's sessions, or there's nothing to say.
fn live_here(engine: &crate::engine::Engine, settings: &SyncSettings) -> Option<GroupSession> {
    if !settings.follow {
        return None;
    }
    match engine.session() {
        Some(session) => {
            let mut session = session.clone();
            // The browsers' own changes to it (site passes, say) stay on this computer.
            session.rest.clear();
            let goal = engine.session_goal().map(str::to_owned);
            Some(GroupSession {
                running: Some(RunningSession { session, goal }),
                ended: None,
            })
        }
        None => {
            let running = settings.live.as_ref()?.running.as_ref()?;
            Some(GroupSession {
                running: None,
                ended: Some(running.session.id.clone()),
            })
        }
    }
}

/// Sends everything that's changed here since the last send, and the session running here if
/// the group hasn't heard of it.
async fn send_changes(app: &AppHandle, client: &Client, settings: &mut SyncSettings, key: &GroupKey) -> Result<()> {
    loop {
        let (records, next, live) = {
            let engine = crate::engine(app);
            let (records, next) = engine.changed_records(settings.sent, PAGE)?;
            let live = live_here(&engine, settings).filter(|live| settings.live.as_ref() != Some(live));
            (records, next, live)
        };
        let sent_any = !records.is_empty() || live.is_some();
        for batch in batches(&records, live.as_ref())? {
            let sealed = key.seal(Purpose::Batch, &settings.group_id, &batch);
            let uploaded = client.upload_batch(&settings.token, sealed).await;
            answered(app, settings, uploaded)?;
        }
        if next != settings.sent || live.is_some() {
            let now = crate::now_ms();
            advance(app, settings, |s| {
                s.sent = next;
                if sent_any {
                    s.last_synced_at = Some(now);
                }
                if live.is_some() {
                    s.live = live;
                }
            })?;
            if sent_any {
                emit_status(app);
            }
        }
        if records.len() < PAGE {
            return Ok(());
        }
    }
}

#[derive(Serialize)]
struct Outgoing<'a> {
    v: u32,
    records: &'a [Record],
    #[serde(skip_serializing_if = "Option::is_none")]
    live: Option<&'a GroupSession>,
}

#[derive(Deserialize)]
struct Incoming {
    v: u32,
    #[serde(default)]
    records: Vec<serde_json::Value>,
    #[serde(default)]
    live: Option<serde_json::Value>,
}

/// Records as batches to seal, each small enough for the server, with the group's session in
/// the last, after the records it may need (an ended session's log, say). A single record too
/// big for one on its own (it would take a profile with tens of thousands of sites) isn't sent.
fn batches(records: &[Record], live: Option<&GroupSession>) -> std::result::Result<Vec<Vec<u8>>, Error> {
    if records.is_empty() && live.is_none() {
        return Ok(Vec::new());
    }
    let packed = compress(&serde_json::to_vec(&Outgoing {
        v: VERSION,
        records,
        live,
    })?);
    if packed.len() + SEALING_BYTES <= MAX_BATCH_BYTES {
        return Ok(vec![packed]);
    }
    if records.len() <= 1 {
        eprintln!("Seiza: a record is too big to sync, so it stays on this computer");
        // The session still goes, without it.
        return match live {
            Some(_) if !records.is_empty() => batches(&[], live),
            _ => Ok(Vec::new()),
        };
    }
    let (first, rest) = records.split_at(records.len() / 2);
    let mut halves = batches(first, None)?;
    halves.extend(batches(rest, live)?);
    Ok(halves)
}

/// JSON's field names repeat in every record, so compressing shrinks a batch several times over.
fn compress(json: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json).expect("writing to memory");
    encoder.finish().expect("writing to memory")
}

/// The first byte of what `compress` makes: zlib's header, for deflate with its usual window.
const ZLIB: u8 = 0x78;

/// A compressed batch's JSON, unless it's damaged or unpacks too big.
fn decompress(packed: &[u8]) -> Option<Vec<u8>> {
    let mut json = Vec::new();
    ZlibDecoder::new(packed)
        .take(MAX_UNPACKED_BYTES + 1)
        .read_to_end(&mut json)
        .ok()?;
    (json.len() as u64 <= MAX_UNPACKED_BYTES).then_some(json)
}

#[derive(Debug, PartialEq)]
enum Opened {
    /// Its records, and the group's session if it said.
    Records(Vec<Record>, Option<Box<GroupSession>>),
    /// From a newer version of Seiza: it waits on the server until this one is updated.
    Newer,
}

/// What a batch from another computer holds. One that won't open (not sealed by this group's
/// key, or changed on the way) holds nothing, as does a record this version doesn't know, or a
/// group's session it can't read.
fn open(key: &GroupKey, group_id: &str, sealed: &[u8]) -> Opened {
    let Ok(plain) = key.open(Purpose::Batch, group_id, sealed) else {
        return Opened::Records(Vec::new(), None);
    };
    // The first byte says how it's packed: plain JSON (batches from before compression) or
    // zlib. Anything else is a packing from a newer version, so it waits for an update rather
    // than being skipped.
    let json = match plain.first() {
        None => return Opened::Records(Vec::new(), None),
        Some(b'{') => Cow::Borrowed(&plain[..]),
        Some(&ZLIB) => match decompress(&plain) {
            Some(json) => Cow::Owned(json),
            None => return Opened::Records(Vec::new(), None),
        },
        Some(_) => return Opened::Newer,
    };
    match serde_json::from_slice::<Incoming>(&json) {
        Ok(batch) if batch.v > VERSION => Opened::Newer,
        Ok(batch) => Opened::Records(
            batch
                .records
                .into_iter()
                .filter_map(|record| serde_json::from_value(record).ok())
                .collect(),
            batch
                .live
                .and_then(|live| serde_json::from_value(live).ok())
                .map(Box::new),
        ),
        Err(_) => Opened::Records(Vec::new(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::records::SyncedName;
    use crate::session::EndReason;
    use crate::store::Store;

    fn name(id: &str, name: &str) -> Record {
        Record::ConstellationName(SyncedName {
            id: id.into(),
            name: Some(name.into()),
            updated_at: 1,
        })
    }

    #[test]
    fn batches_open_on_the_group_s_other_computers() {
        let key = GroupKey::generate();
        let records = vec![name("a", "Orion"), name("b", "Lyra")];
        let sealed: Vec<_> = batches(&records, None)
            .unwrap()
            .iter()
            .map(|batch| key.seal(Purpose::Batch, "group", batch))
            .collect();
        assert_eq!(sealed.len(), 1);
        assert_eq!(open(&key, "group", &sealed[0]), Opened::Records(records, None));
        // Another group's, or another key's, hold nothing.
        assert_eq!(open(&key, "other", &sealed[0]), Opened::Records(vec![], None));
        assert_eq!(
            open(&GroupKey::generate(), "group", &sealed[0]),
            Opened::Records(vec![], None)
        );
    }

    #[test]
    fn what_this_version_can_t_read_is_left_out_or_waits() {
        let key = GroupKey::generate();
        let sealed = |json: &str| key.seal(Purpose::Batch, "group", json.as_bytes());
        // A kind of record it doesn't know is left out, and the rest are saved.
        let mixed = r#"{"v":1,"records":[{"kind":"satellite","id":"s"},
            {"kind":"constellationName","id":"a","name":"Orion","updatedAt":1}]}"#;
        assert_eq!(
            open(&key, "group", &sealed(mixed)),
            Opened::Records(vec![name("a", "Orion")], None)
        );
        // A newer format waits for an update.
        assert_eq!(open(&key, "group", &sealed(r#"{"v":2,"stuff":[]}"#)), Opened::Newer);
        assert_eq!(open(&key, "group", &sealed("{not json")), Opened::Records(vec![], None));
        // So does a packing it doesn't know, and a damaged zlib one is skipped.
        assert_eq!(open(&key, "group", &sealed("\u{1}new")), Opened::Newer);
        assert_eq!(open(&key, "group", &sealed("x damaged")), Opened::Records(vec![], None));
    }

    /// Text that hardly compresses.
    fn noise(len: usize) -> String {
        let mut text = String::new();
        while text.len() < len {
            text.push_str(&uuid::Uuid::new_v4().simple().to_string());
        }
        text.truncate(len);
        text
    }

    #[test]
    fn batches_are_compressed_and_older_plain_ones_still_open() {
        let records: Vec<_> = (0..200).map(|i| name(&i.to_string(), "Orion")).collect();
        let json = serde_json::to_vec(&Outgoing {
            v: VERSION,
            records: &records,
            live: None,
        })
        .unwrap();
        let packed = batches(&records, None).unwrap();
        assert_eq!(packed[0][0], ZLIB);
        assert!(packed[0].len() * 4 < json.len());

        let key = GroupKey::generate();
        let plain = key.seal(Purpose::Batch, "group", &json);
        assert_eq!(open(&key, "group", &plain), Opened::Records(records, None));
    }

    fn settings() -> SyncSettings {
        SyncSettings::new(
            "https://sync.example.com".into(),
            crate::sync::client::Membership {
                group_id: "g".into(),
                device_id: "d".into(),
                token: "t".into(),
            },
            &GroupKey::generate(),
            "Mac".into(),
        )
    }

    fn engine() -> Engine {
        Engine::load(Store::open_in_memory().unwrap(), 0).unwrap()
    }

    #[test]
    fn the_session_running_here_goes_to_the_group_and_its_end_after_it() {
        let mut engine = engine();
        let mut settings = settings();
        assert_eq!(live_here(&engine, &settings), None);

        engine.start_session("writing", 1).unwrap();
        engine.set_session_goal("Draft the intro", 1).unwrap();
        let running = live_here(&engine, &settings).unwrap();
        let sent = running.running.as_ref().unwrap();
        assert_eq!(sent.goal.as_deref(), Some("Draft the intro"));
        assert!(sent.session.rest.is_empty());
        settings.live = Some(running.clone());

        engine.end_session(2, EndReason::Stopped).unwrap();
        let ended = live_here(&engine, &settings).unwrap();
        assert_eq!(ended.ended, Some(sent.session.id.clone()));
        // Once the group's heard, there's nothing more to say.
        settings.live = Some(ended);
        assert_eq!(live_here(&engine, &settings), None);

        // A computer that doesn't follow the group's sessions keeps its own to itself.
        settings.follow = false;
        engine.start_session("writing", 3).unwrap();
        assert_eq!(live_here(&engine, &settings), None);
    }

    #[test]
    fn sessions_from_the_group_are_taken_in_order() {
        let mut there = engine();
        let mut here = engine();
        there.start_session("writing", 1).unwrap();
        let first = live_here(&there, &settings()).unwrap();
        let id = first.running.as_ref().unwrap().session.id.clone();
        let ended = GroupSession {
            running: None,
            ended: Some(id.clone()),
        };
        let (last, changed) = take_lives(&mut here, vec![first.clone()], 2).unwrap();
        assert!(changed);
        assert_eq!(last, Some(first.clone()));
        assert_eq!(here.session().unwrap().id, id);
        let (last, changed) = take_lives(&mut here, vec![first, ended.clone()], 3).unwrap();
        assert!(changed);
        assert_eq!(last, Some(ended));
        assert!(here.session().is_none());
    }

    #[test]
    fn following_again_joins_the_session_the_group_last_said_was_running() {
        let mut there = engine();
        let mut here = engine();
        let mut settings = settings();
        settings.follow = false;
        there.start_session("writing", 1).unwrap();
        let running = live_here(&there, &self::settings()).unwrap();
        let id = running.running.as_ref().unwrap().session.id.clone();
        settings.live = Some(GroupSession {
            running: None,
            ended: Some("older".into()),
        });
        assert!(!join_live(&mut here, &settings, 2).unwrap());
        settings.live = Some(running);
        assert!(join_live(&mut here, &settings, 2).unwrap());
        assert_eq!(here.session().unwrap().id, id);
    }

    #[test]
    fn the_group_s_session_travels_with_the_records() {
        let key = GroupKey::generate();
        let mut engine = engine();
        engine.start_session("writing", 1).unwrap();
        let live = live_here(&engine, &settings()).unwrap();
        let records = vec![name("a", "Orion")];
        let packed = batches(&records, Some(&live)).unwrap();
        assert_eq!(
            open(&key, "g", &key.seal(Purpose::Batch, "g", &packed[0])),
            Opened::Records(records, Some(Box::new(live.clone())))
        );
        // On its own, and in the last of a split batch.
        let alone = batches(&[], Some(&live)).unwrap();
        assert_eq!(
            open(&key, "g", &key.seal(Purpose::Batch, "g", &alone[0])),
            Opened::Records(vec![], Some(Box::new(live.clone())))
        );
        let big: Vec<_> = (0..7).map(|i| name(&i.to_string(), &noise(600_000))).collect();
        let split = batches(&big, Some(&live)).unwrap();
        let lives: Vec<_> = split
            .iter()
            .map(|b| match open(&key, "g", &key.seal(Purpose::Batch, "g", b)) {
                Opened::Records(_, live) => live,
                Opened::Newer => unreachable!(),
            })
            .collect();
        assert_eq!(lives.last().unwrap(), &Some(Box::new(live)));
        assert!(lives[..lives.len() - 1].iter().all(Option::is_none));
    }

    #[test]
    fn big_changes_are_split_to_fit_the_server() {
        let records: Vec<_> = (0..7).map(|i| name(&i.to_string(), &noise(600_000))).collect();
        let batches = batches(&records, None).unwrap();
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|b| b.len() + SEALING_BYTES <= MAX_BATCH_BYTES));
        let key = GroupKey::generate();
        let back: Vec<_> = batches
            .iter()
            .flat_map(|b| match open(&key, "g", &key.seal(Purpose::Batch, "g", b)) {
                Opened::Records(records, _) => records,
                Opened::Newer => unreachable!(),
            })
            .collect();
        assert_eq!(back, records);
        // One too big to send on its own stays here.
        assert!(
            super::batches(&[name("big", &noise(3 * MAX_BATCH_BYTES))], None)
                .unwrap()
                .is_empty()
        );
    }
}
