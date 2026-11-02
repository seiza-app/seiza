// Settings › Sync (PLAN §8, Sync): starting a sync group, joining one or adding a device with a
// pairing code, the group's devices, leaving, and moving to another server. Sending and fetching
// records is in exchange.rs.
//
// A pairing runs in the background for up to its five minutes, asking the server for the other
// computer's messages once a second. That's the only time sync asks over and over; it tells the
// window how it's going as "sync-pairing".

use std::future::Future;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use ts_rs::TS;

use super::client::{Client, ClientError};
use super::crypto::GroupKey;
use super::pairing::{Code, Host, Joiner, MAX_DEVICE_NAME, PairingError};
use super::settings::{SyncSettings, computer_name, server_address};
use crate::error::{Error, Result};

const PAIRING: Duration = Duration::from_secs(5 * 60);
const ASK_AGAIN: Duration = Duration::from_secs(1);

/// What Settings › Sync shows, sent as "sync" whenever it changes.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncStatus {
    /// The server's address, while this computer syncs.
    pub server: Option<String>,
    /// This computer's name in the group, or the one to suggest before it's in one.
    pub device_name: String,
    /// When this computer last sent or fetched (epoch ms).
    pub last_synced_at: Option<i64>,
    /// Why it last stopped syncing early, if the server or this app said why (being offline
    /// isn't shown, beyond when it last synced).
    pub problem: Option<String>,
    /// Whether this computer has sessions of its own, whose rare events and comets are drawn
    /// again when it joins a group and takes the group's sky.
    pub own_sessions: bool,
}

/// One of the group's devices, for the list in Settings › Sync.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncDevice {
    pub id: String,
    /// Its name, or none if it can't be read.
    pub name: Option<String>,
    /// Epoch milliseconds.
    pub added_at: i64,
    /// Epoch milliseconds.
    pub last_seen_at: i64,
    pub this_device: bool,
}

/// How a pairing is going, on either computer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(tag = "phase", rename_all = "camelCase")]
#[ts(export)]
pub enum PairingState {
    #[default]
    Idle,
    /// Add a device: the code to type on the new computer, until it runs out (epoch ms).
    Showing {
        code: String,
        #[serde(rename = "expiresAt")]
        expires_at: i64,
    },
    /// The code was typed right on the new computer: add it?
    Asking {
        #[serde(rename = "deviceName")]
        device_name: String,
    },
    Added {
        #[serde(rename = "deviceName")]
        device_name: String,
    },
    /// Joining: checking the code with the other computer.
    Checking,
    /// Joining: the code was right, and the other computer is asking its person to add this one.
    Waiting,
    Joined,
    Failed {
        problem: String,
    },
}

/// The pairing that's running, if one is.
#[derive(Default)]
pub struct Pairings(Mutex<Current>);

#[derive(Default)]
struct Current {
    /// Counts pairings, so one that's been stopped or replaced can't change what the window
    /// shows or end the one after it.
    generation: u64,
    state: PairingState,
    task: Option<JoinHandle<()>>,
    /// The pairing on the server, to end it if this one stops: the client, this side's key and
    /// the pairing's number.
    open: Option<(Client, String, String)>,
    /// The answer to "Add …?", while it's being asked.
    answer: Option<oneshot::Sender<bool>>,
}

fn current(app: &AppHandle) -> MutexGuard<'_, Current> {
    app.state::<Pairings>()
        .inner()
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn status(app: &AppHandle) -> Result<SyncStatus> {
    let (settings, own_sessions) = {
        let engine = crate::engine(app);
        (engine.sync_settings()?, engine.has_own_sessions()?)
    };
    Ok(SyncStatus {
        server: settings.as_ref().map(|s| s.server.clone()),
        device_name: settings.as_ref().map_or_else(computer_name, |s| s.device_name.clone()),
        problem: settings.as_ref().and_then(|_| super::exchange::problem(app)),
        last_synced_at: settings.and_then(|s| s.last_synced_at),
        own_sessions,
    })
}

/// Tells Settings › Sync how sync is going.
pub(super) fn emit_status(app: &AppHandle) {
    if let Ok(status) = status(app) {
        let _ = app.emit("sync", status);
    }
}

fn sync_changed(app: &AppHandle) {
    emit_status(app);
    crate::changed(app);
}

pub fn pairing(app: &AppHandle) -> PairingState {
    current(app).state.clone()
}

/// Shows how pairing `generation` is going, unless another has taken its place.
fn show(app: &AppHandle, generation: u64, state: PairingState) {
    let mut current = current(app);
    if current.generation == generation {
        current.state = state.clone();
        let _ = app.emit("sync-pairing", state);
    }
}

/// Stops the pairing that's running, if one is, and ends it on the server so its code can't be
/// used. Returns the next pairing's generation.
fn take_over(app: &AppHandle) -> u64 {
    let (task, open, generation) = {
        let mut current = current(app);
        current.generation += 1;
        current.answer = None;
        (current.task.take(), current.open.take(), current.generation)
    };
    if let Some(task) = task {
        task.abort();
    }
    if let Some((client, key, slot)) = open {
        tauri::async_runtime::spawn(async move {
            let _ = client.end_pairing(&key, &slot).await;
        });
    }
    generation
}

pub fn stop_pairing(app: &AppHandle) {
    let generation = take_over(app);
    show(app, generation, PairingState::Idle);
}

/// Notes where pairing `generation` is open on the server, so stopping it ends it there. If
/// another pairing has taken its place meanwhile, ends this one there instead.
fn claim(app: &AppHandle, generation: u64, open: (Client, String, String)) -> bool {
    {
        let mut current = current(app);
        if current.generation == generation {
            current.open = Some(open);
            return true;
        }
    }
    let (client, key, slot) = open;
    tauri::async_runtime::spawn(async move {
        let _ = client.end_pairing(&key, &slot).await;
    });
    false
}

/// Runs one side of pairing `generation` in the background. One that fails is ended on the
/// server, so its code gets one try.
fn run(app: &AppHandle, generation: u64, pairing: impl Future<Output = Result<PairingState>> + Send + 'static) {
    let handle = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let state = pairing.await.unwrap_or_else(|problem| PairingState::Failed {
            problem: problem.to_string(),
        });
        let open = {
            let mut current = current(&handle);
            if current.generation != generation {
                return;
            }
            current.task = None;
            current.answer = None;
            current.open.take()
        };
        if let (PairingState::Failed { .. }, Some((client, key, slot))) = (&state, open) {
            let _ = client.end_pairing(&key, &slot).await;
        }
        show(&handle, generation, state);
    });
    let mut current = current(app);
    if current.generation == generation {
        current.task = Some(task);
    } else {
        task.abort();
    }
}

fn ran_out() -> Error {
    Error::Sync("the pairing code ran out after five minutes; make a new one with Add a device".into())
}

/// Waits a moment before asking the server again, until the pairing's time is up.
async fn wait(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(ran_out());
    }
    tokio::time::sleep(ASK_AGAIN).await;
    Ok(())
}

/// A pairing the server no longer has: once one ends or runs out, the server knows neither side's
/// key for it.
fn ended(error: ClientError) -> Error {
    match error {
        ClientError::Refused {
            status: StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED,
            ..
        } => Error::Sync(
            "the pairing ended before it finished: the code ran out or was typed wrong, or the other computer stopped \
             it. Make a new code with Add a device and try again"
                .into(),
        ),
        other => other.into(),
    }
}

/// What one of a pairing's once-a-second questions got, or `None` to ask again after a passing
/// problem (offline for a moment, or the server busy), until the pairing's time is up.
fn polled<T>(answer: std::result::Result<T, ClientError>) -> Result<Option<T>> {
    match answer {
        Ok(answer) => Ok(Some(answer)),
        Err(ClientError::Offline(_)) => Ok(None),
        Err(ClientError::Refused { status, .. })
            if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() =>
        {
            Ok(None)
        }
        Err(other) => Err(ended(other)),
    }
}

fn pairing_problem(error: PairingError) -> Error {
    Error::Sync(error.to_string())
}

/// Checks that a Seiza sync server answers at the address, for a clearer problem than whatever
/// a wrong address would say to the first real request.
async fn reach(client: &Client) -> Result<()> {
    client.health().await.map_err(|error| match error {
        ClientError::Offline(_) => Error::Sync("couldn't reach a server at that address".into()),
        _ => Error::Sync("there's a server at that address, but it isn't a Seiza sync server".into()),
    })
}

/// The name this computer gives itself in the group.
fn device_name(typed: &str) -> String {
    let name: String = typed.trim().chars().take(MAX_DEVICE_NAME).collect();
    if name.is_empty() { computer_name() } else { name }
}

fn settings(app: &AppHandle) -> Result<SyncSettings> {
    crate::engine(app).sync_settings()?.ok_or(Error::NotSyncing)
}

/// Saves this computer's place in a group it just started or joined, and marks everything
/// on it to be sent there. If it got into another group meanwhile (two clicks at once), the new
/// place is given up again.
async fn settle(app: &AppHandle, client: &Client, settings: SyncSettings, sky: Option<&str>) -> Result<()> {
    let now = crate::now_ms();
    let saved = {
        let engine = crate::engine(app);
        match engine.sync_settings() {
            Ok(None) => engine.set_sync_settings(Some(&settings), now),
            Ok(Some(_)) => Err(Error::AlreadySyncing),
            Err(error) => Err(error),
        }
    };
    if saved.is_err() {
        let _ = client.remove_device(&settings.token, &settings.device_id).await;
        return saved;
    }
    // In the group now, whatever happens next.
    let rest = {
        let mut engine = crate::engine(app);
        sky.map_or(Ok(()), |sky| engine.match_sky(sky, now))
            .and_then(|_| engine.resend_all_records())
    };
    sync_changed(app);
    if sky.is_some() {
        // The Sky draws its rare events and comets with the group's secret now.
        let _ = app.emit("synced", ());
    }
    super::exchange::fetch(app);
    rest
}

/// Turns sync off here after this computer's token stopped working, which means another one
/// removed it. Only if nothing else changed its settings meanwhile.
fn forget(app: &AppHandle, settings: &SyncSettings) -> Result<()> {
    {
        let engine = crate::engine(app);
        if engine.sync_settings()?.as_ref() == Some(settings) {
            engine.set_sync_settings(None, crate::now_ms())?;
        }
    }
    sync_changed(app);
    Ok(())
}

/// A request made with this computer's token, which stops sync here if the server no longer
/// knows it.
pub(super) fn signed_in<T>(
    app: &AppHandle,
    settings: &SyncSettings,
    result: std::result::Result<T, ClientError>,
) -> Result<T> {
    match result {
        Err(error) if error.is_signed_out() => {
            stop_pairing(app);
            forget(app, settings)?;
            Err(Error::RemovedFromSync)
        }
        other => Ok(other?),
    }
}

/// Start a new sync group, with this computer as its first device and a new key.
pub async fn start_group(app: &AppHandle, address: &str, name: &str, invite: Option<&str>) -> Result<()> {
    if crate::engine(app).sync_settings()?.is_some() {
        return Err(Error::AlreadySyncing);
    }
    let server = server_address(address)?;
    let name = device_name(name);
    let client = Client::new(&server)?;
    reach(&client).await?;
    let key = GroupKey::generate();
    let invite = invite.map(str::trim).filter(|invite| !invite.is_empty());
    let membership = client.create_group(&key.seal_label(&name), invite).await?;
    settle(app, &client, SyncSettings::new(server, membership, &key, name), None).await
}

/// Add a device: opens a pairing and shows its code, then waits in the background for the new
/// computer.
pub async fn add_device(app: &AppHandle) -> Result<PairingState> {
    let settings = settings(app)?;
    let key = settings.group_key().ok_or(Error::NotSyncing)?;
    let generation = take_over(app);
    let client = Client::new(&settings.server)?;
    let opened = client.open_pairing(&settings.token).await;
    let pairing = signed_in(app, &settings, opened)?;
    let open = (client.clone(), settings.token.clone(), pairing.slot.clone());
    if !claim(app, generation, open) {
        return Ok(self::pairing(app));
    }
    let code = Code::new(&pairing.slot);
    let showing = PairingState::Showing {
        code: code.to_string(),
        expires_at: pairing.expires_at,
    };
    show(app, generation, showing.clone());
    run(
        app,
        generation,
        host(app.clone(), generation, client, settings, key, code),
    );
    Ok(showing)
}

/// The computer already in the group: sends its hello once the new one joins, checks the reply,
/// asks its person, then sends the welcome and lets the new one in.
async fn host(
    app: AppHandle,
    generation: u64,
    client: Client,
    settings: SyncSettings,
    key: GroupKey,
    code: Code,
) -> Result<PairingState> {
    let deadline = Instant::now() + PAIRING;
    let (token, slot) = (settings.token.as_str(), code.slot());
    loop {
        if polled(client.messages(token, slot, 0).await)?.is_some_and(|inbox| inbox.joined) {
            break;
        }
        wait(deadline).await?;
    }
    let (host, hello) = Host::start(&code, &settings.group_id);
    client.send_message(token, slot, hello).await.map_err(ended)?;
    let reply = loop {
        let inbox = polled(client.messages(token, slot, 0).await)?;
        if let Some(reply) = inbox.and_then(|inbox| inbox.messages.into_iter().next()) {
            break reply;
        }
        wait(deadline).await?;
    };
    let checked = host.check(&reply).map_err(|error| match error {
        PairingError::WrongCode => Error::Sync(
            "the code was typed wrong on the new computer, so it can't be used again. Make a new one with Add a \
             device"
                .into(),
        ),
        other => pairing_problem(other),
    })?;

    let (answer, answered) = oneshot::channel();
    {
        let mut current = current(&app);
        if current.generation != generation {
            return Ok(PairingState::Idle);
        }
        current.answer = Some(answer);
    }
    show(
        &app,
        generation,
        PairingState::Asking {
            device_name: checked.device_name.clone(),
        },
    );
    let left = deadline.saturating_duration_since(Instant::now());
    match tokio::time::timeout(left, answered).await {
        Ok(Ok(true)) => {}
        Ok(_) => {
            let _ = client.end_pairing(token, slot).await;
            return Ok(PairingState::Idle);
        }
        Err(_) => return Err(ran_out()),
    }

    let sky = crate::engine(&app).sky_state(crate::now_ms())?.secret;
    let welcome = checked.welcome(&key, &sky);
    client.send_message(token, slot, welcome).await.map_err(ended)?;
    let label = key.seal_label(&checked.device_name);
    client.admit(token, slot, &label).await.map_err(ended)?;
    Ok(PairingState::Added {
        device_name: checked.device_name,
    })
}

/// The answer to "Add …?".
pub fn answer_device(app: &AppHandle, add: bool) {
    if let Some(answer) = current(app).answer.take() {
        let _ = answer.send(add);
    }
}

/// Join with a pairing code from a computer already in a group, on the same server.
pub async fn join(app: &AppHandle, address: &str, code: &str, name: &str) -> Result<PairingState> {
    if crate::engine(app).sync_settings()?.is_some() {
        return Err(Error::AlreadySyncing);
    }
    let code = Code::parse(code)?;
    let server = server_address(address)?;
    let name = device_name(name);
    let client = Client::new(&server)?;
    reach(&client).await?;
    let generation = take_over(app);
    let joined = client.join_pairing(code.slot()).await.map_err(|error| match error {
        ClientError::Refused {
            status: StatusCode::NOT_FOUND,
            ..
        } => Error::Sync(
            "this server has no pairing with that number. Check the code, or make a new one with Add a device".into(),
        ),
        other => other.into(),
    })?;
    let open = (client.clone(), joined.joiner_key.clone(), code.slot().to_owned());
    if !claim(app, generation, open) {
        return Ok(pairing(app));
    }
    show(app, generation, PairingState::Checking);
    let joining = joiner(app.clone(), generation, client, server, code, joined.joiner_key, name);
    run(app, generation, joining);
    Ok(PairingState::Checking)
}

/// The new computer: answers the hello, waits for its person on the other computer to say Add,
/// then takes the group's key and sky and collects its own token.
async fn joiner(
    app: AppHandle,
    generation: u64,
    client: Client,
    server: String,
    code: Code,
    key: String,
    name: String,
) -> Result<PairingState> {
    let deadline = Instant::now() + PAIRING;
    let slot = code.slot();
    let hello = loop {
        let inbox = polled(client.messages(&key, slot, 0).await)?;
        if let Some(hello) = inbox.and_then(|inbox| inbox.messages.into_iter().next()) {
            break hello;
        }
        wait(deadline).await?;
    };
    let (joiner, reply) = Joiner::reply(&code, &hello, &name).map_err(pairing_problem)?;
    let group_id = joiner.group_id().to_owned();
    client.send_message(&key, slot, reply).await.map_err(ended)?;
    show(&app, generation, PairingState::Waiting);

    let welcome = loop {
        let inbox = polled(client.messages(&key, slot, 1).await)?;
        if let Some(welcome) = inbox.and_then(|inbox| inbox.messages.into_iter().next()) {
            break welcome;
        }
        wait(deadline).await?;
    };
    let welcomed = joiner.finish(&welcome).map_err(pairing_problem)?;
    let membership = loop {
        if let Some(membership) = polled(client.collect_device(&key, slot).await)?.flatten() {
            break membership;
        }
        wait(deadline).await?;
    };
    let settings = SyncSettings::new(server, membership, &welcomed.key, name);
    // The host's proof showed the group id in its hello is its own, so the server has to agree.
    if settings.group_id != group_id {
        let _ = client.remove_device(&settings.token, &settings.device_id).await;
        return Err(pairing_problem(PairingError::Garbled));
    }
    settle(&app, &client, settings, Some(&welcomed.sky)).await?;
    Ok(PairingState::Joined)
}

/// The group's devices, oldest first.
pub async fn devices(app: &AppHandle) -> Result<Vec<SyncDevice>> {
    let settings = settings(app)?;
    let key = settings.group_key().ok_or(Error::NotSyncing)?;
    let listed = Client::new(&settings.server)?.devices(&settings.token).await;
    let listed = signed_in(app, &settings, listed)?;
    Ok(listed
        .devices
        .into_iter()
        .map(|device| SyncDevice {
            name: key.open_label(&device.label).ok(),
            this_device: device.id == listed.you,
            id: device.id,
            added_at: device.created_at,
            last_seen_at: device.last_seen_at,
        })
        .collect())
}

/// Removes another device from the group, say a lost laptop. Removing this one leaves sync.
pub async fn remove_device(app: &AppHandle, id: &str) -> Result<()> {
    let settings = settings(app)?;
    if id == settings.device_id {
        return leave(app).await.map(|_| ());
    }
    let removed = Client::new(&settings.server)?.remove_device(&settings.token, id).await;
    signed_in(app, &settings, removed)
}

/// Leave sync: this computer stops syncing and keeps everything on it. Returns whether the
/// server heard; if it didn't, it lists this computer until another one removes it.
pub async fn leave(app: &AppHandle) -> Result<bool> {
    let settings = settings(app)?;
    stop_pairing(app);
    let removed = Client::new(&settings.server)?
        .remove_device(&settings.token, &settings.device_id)
        .await;
    let heard = match removed {
        Ok(()) => true,
        Err(error) => error.is_signed_out(),
    };
    forget(app, &settings)?;
    Ok(heard)
}

/// Move to another server: starts a new group there with a new key, marks everything here to
/// be sent to it, and leaves the old group. The other computers join the new one by pairing.
pub async fn move_server(app: &AppHandle, address: &str, invite: Option<&str>) -> Result<()> {
    let old = settings(app)?;
    let server = server_address(address)?;
    if server == old.server {
        return Err(Error::Sync("that's the server this computer already uses".into()));
    }
    stop_pairing(app);
    let client = Client::new(&server)?;
    reach(&client).await?;
    let key = GroupKey::generate();
    let invite = invite.map(str::trim).filter(|invite| !invite.is_empty());
    let membership = client.create_group(&key.seal_label(&old.device_name), invite).await?;
    let new = SyncSettings {
        follow: old.follow,
        ..SyncSettings::new(server, membership, &key, old.device_name.clone())
    };
    let saved = {
        let engine = crate::engine(app);
        match engine.sync_settings() {
            Ok(Some(now)) if now == old => engine.set_sync_settings(Some(&new), crate::now_ms()),
            Ok(_) => Err(Error::Sync("sync changed on this computer meanwhile; try again".into())),
            Err(error) => Err(error),
        }
    };
    if saved.is_err() {
        let _ = client.remove_device(&new.token, &new.device_id).await;
        return saved;
    }
    // On the new server now, whatever happens next.
    let resent = crate::engine(app).resend_all_records();
    sync_changed(app);
    super::exchange::fetch(app);
    // Leaves the old group if its server still answers. If it doesn't, there's nothing to tidy.
    if let Ok(old_client) = Client::new(&old.server) {
        let _ = old_client.remove_device(&old.token, &old.device_id).await;
    }
    resent
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(status: StatusCode) -> std::result::Result<(), ClientError> {
        Err(ClientError::Refused {
            status,
            message: "unknown device".into(),
        })
    }

    #[test]
    fn a_busy_server_is_asked_again_and_a_gone_pairing_ends_it() {
        assert_eq!(polled(Ok(3)).unwrap(), Some(3));
        for status in [StatusCode::TOO_MANY_REQUESTS, StatusCode::BAD_GATEWAY] {
            assert_eq!(polled(refused(status)).unwrap(), None);
        }
        // Once a pairing ends or runs out, the server knows neither side's key for it.
        for status in [StatusCode::UNAUTHORIZED, StatusCode::NOT_FOUND] {
            let problem = polled(refused(status)).unwrap_err().to_string();
            assert!(problem.starts_with("the pairing ended before it finished"), "{problem}");
        }
        let problem = polled(refused(StatusCode::CONFLICT)).unwrap_err().to_string();
        assert_eq!(problem, "the sync server said: unknown device");
    }
}
