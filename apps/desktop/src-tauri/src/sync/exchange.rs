// Sending this computer's changes to the sync server and fetching the others' (PLAN §8, Sync,
// build step 5). There are no timers: a send goes out after something changes here (a session
// ends, a goal is answered, a constellation or profile changes), and a fetch when the window
// opens and when a session starts or ends. Changes go as records.rs's records, in batches of
// `{"v": 1, "records": [...]}` compressed with zlib and sealed by the group's key.
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
use super::settings::SyncSettings;
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
        Ok(()) => None,
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
        let mut reached = settings.fetched;
        for batch in &page.batches {
            // This computer's own, sent back. Ones it sent under an earlier id (before it left and
            // joined again) are saved like any other's, and change nothing.
            if batch.device_id != settings.device_id {
                match open(key, &settings.group_id, &batch.data) {
                    Opened::Records(opened) => records.extend(opened),
                    Opened::Newer => {
                        newer = true;
                        break;
                    }
                }
            }
            reached = batch.seq;
        }
        let now = crate::now_ms();
        let applied = crate::engine(app).apply_records(&records, now)?;
        // Told a page at a time, so what's saved shows even if a later page doesn't come.
        if applied > 0 {
            crate::changed(app);
            let _ = app.emit("synced", ());
        }
        advance(app, settings, |s| {
            s.fetched = reached;
            s.last_synced_at = Some(now);
        })?;
        emit_status(app);
        if newer || !page.more || page.batches.is_empty() {
            return Ok(newer);
        }
    }
}

/// Sends everything that's changed here since the last send.
async fn send_changes(app: &AppHandle, client: &Client, settings: &mut SyncSettings, key: &GroupKey) -> Result<()> {
    loop {
        let (records, next) = crate::engine(app).changed_records(settings.sent, PAGE)?;
        let sent_any = !records.is_empty();
        for batch in batches(&records)? {
            let sealed = key.seal(Purpose::Batch, &settings.group_id, &batch);
            let uploaded = client.upload_batch(&settings.token, sealed).await;
            answered(app, settings, uploaded)?;
        }
        if next != settings.sent {
            let now = crate::now_ms();
            advance(app, settings, |s| {
                s.sent = next;
                if sent_any {
                    s.last_synced_at = Some(now);
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
}

#[derive(Deserialize)]
struct Incoming {
    v: u32,
    #[serde(default)]
    records: Vec<serde_json::Value>,
}

/// Records as batches to seal, each small enough for the server. A single record too big for
/// one on its own (it would take a profile with tens of thousands of sites) isn't sent.
fn batches(records: &[Record]) -> std::result::Result<Vec<Vec<u8>>, Error> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let packed = compress(&serde_json::to_vec(&Outgoing { v: VERSION, records })?);
    if packed.len() + SEALING_BYTES <= MAX_BATCH_BYTES {
        return Ok(vec![packed]);
    }
    if records.len() == 1 {
        eprintln!("Seiza: a record is too big to sync, so it stays on this computer");
        return Ok(Vec::new());
    }
    let (first, rest) = records.split_at(records.len() / 2);
    let mut halves = batches(first)?;
    halves.extend(batches(rest)?);
    Ok(halves)
}

/// JSON's field names repeat in every record, so compressing shrinks a batch several times over.
fn compress(json: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json).expect("writing to memory");
    encoder.finish().expect("writing to memory")
}

/// A batch's JSON. zlib's header never starts with `{`, so batches sent before they were
/// compressed still open.
fn decompress(packed: &[u8]) -> Option<Cow<'_, [u8]>> {
    if packed.first() == Some(&b'{') {
        return Some(Cow::Borrowed(packed));
    }
    let mut json = Vec::new();
    ZlibDecoder::new(packed)
        .take(MAX_UNPACKED_BYTES + 1)
        .read_to_end(&mut json)
        .ok()?;
    (json.len() as u64 <= MAX_UNPACKED_BYTES).then_some(Cow::Owned(json))
}

#[derive(Debug, PartialEq)]
enum Opened {
    Records(Vec<Record>),
    /// From a newer version of Seiza: it waits on the server until this one is updated.
    Newer,
}

/// What a batch from another computer holds. One that won't open (not sealed by this group's
/// key, or changed on the way) holds nothing, as does a record this version doesn't know.
fn open(key: &GroupKey, group_id: &str, sealed: &[u8]) -> Opened {
    let Some(plain) = key
        .open(Purpose::Batch, group_id, sealed)
        .ok()
        .and_then(|p| decompress(&p).map(Cow::into_owned))
    else {
        return Opened::Records(Vec::new());
    };
    match serde_json::from_slice::<Incoming>(&plain) {
        Ok(batch) if batch.v > VERSION => Opened::Newer,
        Ok(batch) => Opened::Records(
            batch
                .records
                .into_iter()
                .filter_map(|record| serde_json::from_value(record).ok())
                .collect(),
        ),
        Err(_) => Opened::Records(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::SyncedName;

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
        let sealed: Vec<_> = batches(&records)
            .unwrap()
            .iter()
            .map(|batch| key.seal(Purpose::Batch, "group", batch))
            .collect();
        assert_eq!(sealed.len(), 1);
        assert_eq!(open(&key, "group", &sealed[0]), Opened::Records(records));
        // Another group's, or another key's, hold nothing.
        assert_eq!(open(&key, "other", &sealed[0]), Opened::Records(vec![]));
        assert_eq!(
            open(&GroupKey::generate(), "group", &sealed[0]),
            Opened::Records(vec![])
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
            Opened::Records(vec![name("a", "Orion")])
        );
        // A newer format waits for an update.
        assert_eq!(open(&key, "group", &sealed(r#"{"v":2,"stuff":[]}"#)), Opened::Newer);
        assert_eq!(open(&key, "group", &sealed("not json")), Opened::Records(vec![]));
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
        })
        .unwrap();
        let packed = batches(&records).unwrap();
        assert!(packed[0].len() * 4 < json.len());

        let key = GroupKey::generate();
        let plain = key.seal(Purpose::Batch, "group", &json);
        assert_eq!(open(&key, "group", &plain), Opened::Records(records));
    }

    #[test]
    fn big_changes_are_split_to_fit_the_server() {
        let records: Vec<_> = (0..7).map(|i| name(&i.to_string(), &noise(600_000))).collect();
        let batches = batches(&records).unwrap();
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|b| b.len() + SEALING_BYTES <= MAX_BATCH_BYTES));
        let key = GroupKey::generate();
        let back: Vec<_> = batches
            .iter()
            .flat_map(|b| match open(&key, "g", &key.seal(Purpose::Batch, "g", b)) {
                Opened::Records(records) => records,
                Opened::Newer => unreachable!(),
            })
            .collect();
        assert_eq!(back, records);
        // One too big to send on its own stays here.
        assert!(
            super::batches(&[name("big", &noise(3 * MAX_BATCH_BYTES))])
                .unwrap()
                .is_empty()
        );
    }
}
