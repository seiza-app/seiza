// The HTTP API, under /v1. Every request there needs the `Seiza-Protocol: 1` header, which isn't
// a secret: it lets the server drop scanners and stray traffic before doing any work. Devices
// sign in with `Authorization: Bearer <token>`, and a new device in a pairing with the key it got
// when it joined. Encrypted data travels as base64 in JSON, except uploaded batches, which are
// the request's raw body.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::db::{self, AddBatch, CreateGroup, DeviceRef, NewDevice};
use crate::error::ApiError;
use crate::limits::{Kind, Limits};
use crate::live::{self, Hub, Note};
use crate::pairing::{Pairings, Side};
use crate::tokens;

/// The protocol version this server speaks.
const PROTOCOL: &str = "1";
/// The largest batch a device can upload at once, in bytes.
const MAX_BATCH_BYTES: usize = 1024 * 1024;
/// The largest of any other request body.
const MAX_BODY_BYTES: usize = 16 * 1024;
/// The largest message between a group's devices (live.rs), sealed.
const MAX_LIVE_MESSAGE_BYTES: usize = 4 * 1024;
/// The largest device label (encrypted by the app, so a little bigger than the name).
const MAX_LABEL_BYTES: usize = 512;
/// A page of batches holds at most this many, or about this many bytes.
const PAGE_BATCHES: usize = 500;
const PAGE_BYTES: usize = 4 * 1024 * 1024;
/// How often expired pairings are swept for new devices that never collected their token.
const SWEEP_EVERY: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct App(Arc<Inner>);

struct Inner {
    db: Mutex<Connection>,
    config: Config,
    limits: Limits,
    pairings: Pairings,
    live: Arc<Hub>,
    clock: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl App {
    pub fn new(db: Connection, config: Config) -> Self {
        Self::with_clock(db, config, tokens::now_ms)
    }

    pub fn with_clock(db: Connection, config: Config, clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self(Arc::new(Inner {
            db: Mutex::new(db),
            config,
            limits: Limits::default(),
            pairings: Pairings::default(),
            live: Arc::default(),
            clock: Box::new(clock),
        }))
    }

    fn now(&self) -> i64 {
        (self.0.clock)()
    }

    /// Ends the live connections, for shutting down.
    pub fn close_live(&self) {
        self.0.live.close();
    }

    /// Runs a database call on a thread that's allowed to wait for SQLite.
    async fn db<T: Send + 'static>(
        &self,
        call: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, ApiError> {
        let app = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut db = app.0.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            call(&mut db)
        })
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(ApiError::from)
    }

    /// The client's address: from the proxy's header when one is set up, else the connection's.
    fn client_ip(&self, headers: &HeaderMap, connection: SocketAddr) -> IpAddr {
        self.0
            .config
            .client_ip_header
            .as_ref()
            .and_then(|name| headers.get(name))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(connection.ip())
    }

    fn limit(&self, ip: IpAddr, kind: Kind) -> Result<(), ApiError> {
        if self.0.limits.allow(ip, kind, self.now()) {
            Ok(())
        } else {
            Err(ApiError::TooManyRequests)
        }
    }
}

pub fn router(app: App) -> Router {
    let v1 = Router::new()
        .route("/groups", post(create_group))
        .route("/devices", get(list_devices))
        .route("/devices/{id}", delete(remove_device))
        .route(
            "/batches",
            post(add_batch)
                .get(list_batches)
                .layer(DefaultBodyLimit::max(MAX_BATCH_BYTES)),
        )
        .route("/live", get(listen).post(tell))
        .route("/pairings", post(open_pairing))
        .route("/pairings/{slot}", delete(end_pairing))
        .route("/pairings/{slot}/join", post(join_pairing))
        .route("/pairings/{slot}/messages", post(send_message).get(receive_messages))
        .route("/pairings/{slot}/admit", post(admit))
        .route("/pairings/{slot}/device", get(collect_device))
        .layer(middleware::from_fn_with_state(app.clone(), gate));
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/v1", v1)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(app)
}

/// The client's address, for rate limits.
#[derive(Clone, Copy)]
struct ClientIp(IpAddr);

/// Checks the protocol header and the overall rate limit before anything else.
async fn gate(
    State(app): State<App>,
    ConnectInfo(connection): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let protocol = request
        .headers()
        .get("seiza-protocol")
        .and_then(|value| value.to_str().ok());
    if protocol != Some(PROTOCOL) {
        return ApiError::BadRequest("this is a Seiza sync server; requests need the header Seiza-Protocol: 1")
            .into_response();
    }
    let ip = app.client_ip(request.headers(), connection);
    if let Err(error) = app.limit(ip, Kind::Request) {
        return error.into_response();
    }
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// The device a request comes from, by its token.
struct Authed(DeviceRef);

impl FromRequestParts<App> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Self::Rejection> {
        let token = bearer(&parts.headers).ok_or(ApiError::Unauthorized)?;
        let now = app.now();
        app.db(move |db| db::device_for_token(db, &token, now))
            .await?
            .map(Authed)
            .ok_or(ApiError::Unauthorized)
    }
}

fn decode(data: &str, max: usize) -> Result<Vec<u8>, ApiError> {
    let bytes = STANDARD.decode(data).map_err(|_| ApiError::BadRequest("not base64"))?;
    if bytes.is_empty() || bytes.len() > max {
        return Err(ApiError::BadRequest("empty or too long"));
    }
    Ok(bytes)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewDeviceResponse {
    group_id: String,
    device_id: String,
    token: String,
}

impl From<NewDevice> for NewDeviceResponse {
    fn from(device: NewDevice) -> Self {
        Self {
            group_id: device.group_id,
            device_id: device.device_id,
            token: device.token,
        }
    }
}

#[derive(Deserialize)]
struct CreateGroupRequest {
    /// The first device's label, encrypted, in base64.
    label: String,
    invite: Option<String>,
}

async fn create_group(
    State(app): State<App>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Json(request): Json<CreateGroupRequest>,
) -> Result<Json<NewDeviceResponse>, ApiError> {
    app.limit(ip, Kind::NewGroup)?;
    let label = decode(&request.label, MAX_LABEL_BYTES)?;
    let (new_groups, now) = (app.0.config.new_groups, app.now());
    let outcome = app
        .db(move |db| db::create_group(db, &label, new_groups, request.invite.as_deref(), now))
        .await?;
    match outcome {
        CreateGroup::Created(device) => Ok(Json(device.into())),
        CreateGroup::Closed => Err(ApiError::Forbidden("this server isn't taking new sync groups")),
        CreateGroup::BadInvite => Err(ApiError::Forbidden("a new sync group here needs a valid invite code")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceResponse {
    id: String,
    label: String,
    created_at: i64,
    last_seen_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DevicesResponse {
    devices: Vec<DeviceResponse>,
    /// The id of the device asking.
    you: String,
}

async fn list_devices(State(app): State<App>, Authed(device): Authed) -> Result<Json<DevicesResponse>, ApiError> {
    let group_id = device.group_id.clone();
    let rows = app.db(move |db| db::devices(db, &group_id)).await?;
    Ok(Json(DevicesResponse {
        devices: rows
            .into_iter()
            .map(|row| DeviceResponse {
                id: row.id,
                label: STANDARD.encode(row.label),
                created_at: row.created_at,
                last_seen_at: row.last_seen_at,
            })
            .collect(),
        you: device.id,
    }))
}

/// Removes a device from the group (Remove, or Leave sync when it's the device asking).
async fn remove_device(
    State(app): State<App>,
    Authed(device): Authed,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let group_id = device.group_id.clone();
    let removing = id.clone();
    let removed = app.db(move |db| db::remove_device(db, &group_id, &removing)).await?;
    if removed {
        app.0.live.tell(&device.group_id, Note::Removed { device: id });
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Serialize)]
struct SeqResponse {
    seq: i64,
}

async fn add_batch(State(app): State<App>, Authed(device): Authed, body: Bytes) -> Result<Json<SeqResponse>, ApiError> {
    if body.is_empty() {
        return Err(ApiError::BadRequest("a batch can't be empty"));
    }
    let (max, now) = (app.0.config.group_max_bytes, app.now());
    let adding = device.clone();
    match app.db(move |db| db::add_batch(db, &adding, &body, max, now)).await? {
        AddBatch::Added { seq } => {
            app.0.live.tell(&device.group_id, Note::Batch { seq, from: device.id });
            Ok(Json(SeqResponse { seq }))
        }
        AddBatch::Full => Err(ApiError::TooLarge("this sync group is full")),
    }
}

#[derive(Deserialize)]
struct AfterQuery {
    #[serde(default)]
    after: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchResponse {
    seq: i64,
    device_id: String,
    data: String,
    created_at: i64,
}

#[derive(Serialize)]
struct BatchesResponse {
    batches: Vec<BatchResponse>,
    /// True if there are more after these.
    more: bool,
}

async fn list_batches(
    State(app): State<App>,
    Authed(device): Authed,
    Query(query): Query<AfterQuery>,
) -> Result<Json<BatchesResponse>, ApiError> {
    let (rows, more) = app
        .db(move |db| db::batches_after(db, &device.group_id, query.after, PAGE_BATCHES, PAGE_BYTES))
        .await?;
    Ok(Json(BatchesResponse {
        batches: rows
            .into_iter()
            .map(|row| BatchResponse {
                seq: row.seq,
                device_id: row.device_id,
                data: STANDARD.encode(row.data),
                created_at: row.created_at,
            })
            .collect(),
        more,
    }))
}

/// A device's live connection (live.rs).
async fn listen(State(app): State<App>, headers: HeaderMap, Authed(device): Authed) -> Result<Response, ApiError> {
    let token = bearer(&headers).ok_or(ApiError::Unauthorized)?;
    let checking = app.clone();
    let events = live::listen(&app.0.live, device, move || {
        let (app, token) = (checking.clone(), token.clone());
        async move {
            let now = app.now();
            // A database error leaves it connected, to be checked again later.
            app.db(move |db| db::device_for_token(db, &token, now))
                .await
                .map_or(true, |device| device.is_some())
        }
    })?;
    // nginx would otherwise hold events back until it had a bufferful.
    Ok(([("x-accel-buffering", "no")], events).into_response())
}

/// Passes a sealed message on to the group's other connected devices. It isn't kept.
async fn tell(State(app): State<App>, Authed(device): Authed, body: Bytes) -> Result<StatusCode, ApiError> {
    if body.is_empty() || body.len() > MAX_LIVE_MESSAGE_BYTES {
        return Err(ApiError::BadRequest("empty or too long"));
    }
    let note = Note::Message {
        from: device.id,
        data: STANDARD.encode(&body),
    };
    app.0.live.tell(&device.group_id, note);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingResponse {
    slot: String,
    expires_at: i64,
}

async fn open_pairing(State(app): State<App>, Authed(device): Authed) -> Result<Json<PairingResponse>, ApiError> {
    let (slot, expires_at) = app.0.pairings.open(&device.group_id, &device.id, app.now())?;
    // The group's earlier pairing may have admitted a device that never collected its token.
    forget_uncollected(&app).await?;
    Ok(Json(PairingResponse { slot, expires_at }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinResponse {
    joiner_key: String,
    expires_at: i64,
}

async fn join_pairing(
    State(app): State<App>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    Path(slot): Path<String>,
) -> Result<Json<JoinResponse>, ApiError> {
    app.limit(ip, Kind::Join)?;
    let (joiner_key, expires_at) = app.0.pairings.join(&slot, app.now())?;
    Ok(Json(JoinResponse { joiner_key, expires_at }))
}

/// Which side of a pairing a request comes from: the new device by its joiner key, or the device
/// that opened it by its token.
async fn side(app: &App, headers: &HeaderMap, slot: &str) -> Result<Side, ApiError> {
    let key = bearer(headers).ok_or(ApiError::Unauthorized)?;
    let now = app.now();
    if app.0.pairings.is_joiner(slot, &key, now) {
        return Ok(Side::Joiner);
    }
    let device = app.db(move |db| db::device_for_token(db, &key, now)).await?;
    match device {
        Some(device) if app.0.pairings.is_host(slot, &device.id, now) => Ok(Side::Host),
        _ => Err(ApiError::Unauthorized),
    }
}

async fn send_message(
    State(app): State<App>,
    headers: HeaderMap,
    Path(slot): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let side = side(&app, &headers, &slot).await?;
    if body.is_empty() {
        return Err(ApiError::BadRequest("a message can't be empty"));
    }
    app.0.pairings.send(&slot, side, body.to_vec(), app.now())?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MessagesQuery {
    #[serde(default)]
    after: usize,
}

#[derive(Serialize)]
struct MessagesResponse {
    messages: Vec<String>,
    joined: bool,
}

async fn receive_messages(
    State(app): State<App>,
    headers: HeaderMap,
    Path(slot): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<MessagesResponse>, ApiError> {
    let side = side(&app, &headers, &slot).await?;
    let inbox = app.0.pairings.receive(&slot, side, query.after, app.now())?;
    Ok(Json(MessagesResponse {
        messages: inbox
            .messages
            .into_iter()
            .map(|message| STANDARD.encode(message))
            .collect(),
        joined: inbox.joined,
    }))
}

#[derive(Deserialize)]
struct AdmitRequest {
    /// The new device's label, encrypted, in base64.
    label: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AdmitResponse {
    device_id: String,
}

/// The device that opened the pairing lets the new one in, once its person has said Add.
async fn admit(
    State(app): State<App>,
    Authed(host): Authed,
    Path(slot): Path<String>,
    Json(request): Json<AdmitRequest>,
) -> Result<Json<AdmitResponse>, ApiError> {
    let now = app.now();
    if !app.0.pairings.is_host(&slot, &host.id, now) {
        return Err(ApiError::Unauthorized);
    }
    let label = decode(&request.label, MAX_LABEL_BYTES)?;
    let group_id = app.0.pairings.group_to_admit(&slot, now)?;
    let device = app
        .db(move |db| {
            if db::group_exists(db, &group_id)? {
                db::add_device(db, &group_id, &label, now).map(Some)
            } else {
                Ok(None)
            }
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    let device_id = device.device_id.clone();
    if let Err(device) = app.0.pairings.admit(&slot, device, app.now()) {
        // The pairing ended meanwhile, so no one can collect the token: take the device out again.
        forget(&app, device).await?;
        return Err(ApiError::NotFound);
    }
    Ok(Json(AdmitResponse { device_id }))
}

/// Removes a device that was admitted but never collected its token.
async fn forget(app: &App, device: NewDevice) -> Result<(), ApiError> {
    app.db(move |db| db::remove_device(db, &device.group_id, &device.device_id))
        .await
        .map(|_| ())
}

/// Removes the devices admitted by pairings that went (expired or replaced) before they were
/// collected.
async fn forget_uncollected(app: &App) -> Result<(), ApiError> {
    for device in app.0.pairings.sweep(app.now()) {
        forget(app, device).await?;
    }
    Ok(())
}

/// Sweeps expired pairings every minute, since one can expire with nothing else touching the
/// pairings for a long while, which would leave its uncollected device in the group.
pub async fn sweep_pairings(app: App) {
    let mut every = tokio::time::interval(SWEEP_EVERY);
    loop {
        every.tick().await;
        // A database error is already logged, and the next sweep has nothing left to retry.
        let _ = forget_uncollected(&app).await;
    }
}

/// The new device collects its token once it's been admitted: 200 with it, or 204 while it waits.
async fn collect_device(State(app): State<App>, headers: HeaderMap, Path(slot): Path<String>) -> Response {
    let result = async {
        if side(&app, &headers, &slot).await? != Side::Joiner {
            return Err(ApiError::Unauthorized);
        }
        app.0.pairings.collect(&slot, app.now())
    }
    .await;
    match result {
        Ok(Some(device)) => Json(NewDeviceResponse::from(device)).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error.into_response(),
    }
}

/// Either side ends the pairing, e.g. when the code was wrong, so it can't be tried again.
async fn end_pairing(
    State(app): State<App>,
    headers: HeaderMap,
    Path(slot): Path<String>,
) -> Result<StatusCode, ApiError> {
    side(&app, &headers, &slot).await?;
    if let Some(device) = app.0.pairings.end(&slot, app.now()) {
        forget(&app, device).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::Method;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::config::NewGroups;
    use crate::pairing::PAIRING_MS;

    struct Server {
        app: App,
        router: Router,
        clock: Arc<AtomicI64>,
    }

    struct Call<'a> {
        method: Method,
        path: &'a str,
        token: Option<&'a str>,
        body: Body,
        content_type: Option<&'static str>,
        protocol: bool,
        ip: Option<&'a str>,
    }

    impl<'a> Call<'a> {
        fn new(method: Method, path: &'a str) -> Self {
            Self {
                method,
                path,
                token: None,
                body: Body::empty(),
                content_type: None,
                protocol: true,
                ip: None,
            }
        }

        fn token(mut self, token: &'a str) -> Self {
            self.token = Some(token);
            self
        }

        fn json(mut self, value: Value) -> Self {
            self.body = Body::from(value.to_string());
            self.content_type = Some("application/json");
            self
        }

        fn bytes(mut self, bytes: &[u8]) -> Self {
            self.body = Body::from(bytes.to_vec());
            self.content_type = Some("application/octet-stream");
            self
        }
    }

    /// A live connection's events, as they come.
    struct Events {
        body: Body,
        text: String,
    }

    impl Events {
        /// The next event's name and data, or None if the stream ended or nothing came for a
        /// moment. Comments (the heartbeat) are skipped.
        async fn next(&mut self) -> Option<(String, Value)> {
            loop {
                if let Some(end) = self.text.find("\n\n") {
                    let block: String = self.text.drain(..end + 2).collect();
                    let field = |name: &str| {
                        block
                            .lines()
                            .find_map(|line| line.strip_prefix(name).map(str::to_owned))
                    };
                    let Some(name) = field("event: ") else { continue };
                    let data = field("data: ").and_then(|d| serde_json::from_str(&d).ok());
                    return Some((name, data.unwrap_or(Value::Null)));
                }
                let frame = tokio::time::timeout(Duration::from_millis(200), self.body.frame()).await;
                match frame {
                    Ok(Some(Ok(frame))) => {
                        if let Ok(data) = frame.into_data() {
                            self.text.push_str(&String::from_utf8_lossy(&data));
                        }
                    }
                    _ => return None,
                }
            }
        }
    }

    fn label(text: &str) -> String {
        STANDARD.encode(text)
    }

    impl Server {
        fn new(new_groups: NewGroups) -> Self {
            Self::with(new_groups, |_| {})
        }

        fn with(new_groups: NewGroups, change: impl FnOnce(&mut Config)) -> Self {
            let mut config = Config::from_lookup(|_| None).unwrap();
            config.new_groups = new_groups;
            change(&mut config);
            let clock = Arc::new(AtomicI64::new(1_000));
            let reading = clock.clone();
            let app = App::with_clock(db::open_in_memory().unwrap(), config, move || {
                reading.load(Ordering::SeqCst)
            });
            let router = router(app.clone()).layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));
            Self { app, router, clock }
        }

        fn advance(&self, ms: i64) {
            self.clock.fetch_add(ms, Ordering::SeqCst);
        }

        async fn send(&self, call: Call<'_>) -> (StatusCode, Value) {
            let mut request = axum::http::Request::builder().method(call.method).uri(call.path);
            if call.protocol {
                request = request.header("Seiza-Protocol", "1");
            }
            if let Some(token) = call.token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            if let Some(content_type) = call.content_type {
                request = request.header(header::CONTENT_TYPE, content_type);
            }
            if let Some(ip) = call.ip {
                request = request.header("CF-Connecting-IP", ip);
            }
            let response = self
                .router
                .clone()
                .oneshot(request.body(call.body).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        }

        /// Starts a group and returns the first device's (group id, device id, token).
        async fn first_device(&self) -> (String, String, String) {
            let (status, body) = self
                .send(Call::new(Method::POST, "/v1/groups").json(json!({ "label": label("laptop") })))
                .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            (
                body["groupId"].as_str().unwrap().into(),
                body["deviceId"].as_str().unwrap().into(),
                body["token"].as_str().unwrap().into(),
            )
        }

        /// Pairs a new device into the group through the host, and returns its token.
        async fn pair(&self, host_token: &str) -> String {
            let (_, opened) = self
                .send(Call::new(Method::POST, "/v1/pairings").token(host_token))
                .await;
            let slot = opened["slot"].as_str().unwrap().to_string();
            let (_, joined) = self
                .send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
                .await;
            let key = joined["joinerKey"].as_str().unwrap().to_string();
            let (status, _) = self
                .send(
                    Call::new(Method::POST, &format!("/v1/pairings/{slot}/admit"))
                        .token(host_token)
                        .json(json!({ "label": label("desktop") })),
                )
                .await;
            assert_eq!(status, StatusCode::OK);
            let (_, device) = self
                .send(Call::new(Method::GET, &format!("/v1/pairings/{slot}/device")).token(&key))
                .await;
            device["token"].as_str().unwrap().to_string()
        }

        /// Opens a pairing and has a new device join it and be admitted, without collecting its
        /// token.
        async fn admit_without_collecting(&self, host_token: &str) {
            let (_, opened) = self
                .send(Call::new(Method::POST, "/v1/pairings").token(host_token))
                .await;
            let slot = opened["slot"].as_str().unwrap().to_string();
            self.send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
                .await;
            let (status, _) = self
                .send(
                    Call::new(Method::POST, &format!("/v1/pairings/{slot}/admit"))
                        .token(host_token)
                        .json(json!({ "label": label("desktop") })),
                )
                .await;
            assert_eq!(status, StatusCode::OK);
        }

        /// Opens a device's live connection.
        async fn listen(&self, token: &str) -> Events {
            let request = axum::http::Request::builder()
                .uri("/v1/live")
                .header("Seiza-Protocol", "1")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let response = self.router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["content-type"], "text/event-stream");
            Events {
                body: response.into_body(),
                text: String::new(),
            }
        }

        async fn device_count(&self, token: &str) -> usize {
            let (_, devices) = self.send(Call::new(Method::GET, "/v1/devices").token(token)).await;
            devices["devices"].as_array().unwrap().len()
        }
    }

    #[tokio::test]
    async fn health_needs_no_header_but_the_api_does() {
        let server = Server::new(NewGroups::Open);
        let mut call = Call::new(Method::GET, "/health");
        call.protocol = false;
        assert_eq!(server.send(call).await.0, StatusCode::OK);

        let mut call = Call::new(Method::POST, "/v1/groups").json(json!({ "label": label("laptop") }));
        call.protocol = false;
        let (status, body) = server.send(call).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("Seiza-Protocol"));
    }

    #[tokio::test]
    async fn the_first_group_closes_the_server_by_default() {
        let server = Server::new(NewGroups::FirstOnly);
        server.first_device().await;
        let (status, body) = server
            .send(Call::new(Method::POST, "/v1/groups").json(json!({ "label": label("someone") })))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "this server isn't taking new sync groups");
    }

    #[tokio::test]
    async fn requests_without_a_known_token_are_turned_away() {
        let server = Server::new(NewGroups::Open);
        assert_eq!(
            server.send(Call::new(Method::GET, "/v1/devices")).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .send(Call::new(Method::GET, "/v1/devices").token("made-up"))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn batches_go_up_and_come_back_in_order() {
        let server = Server::new(NewGroups::Open);
        let (_, laptop, token) = server.first_device().await;
        for data in [&b"sealed one"[..], b"sealed two"] {
            let (status, _) = server
                .send(Call::new(Method::POST, "/v1/batches").token(&token).bytes(data))
                .await;
            assert_eq!(status, StatusCode::OK);
        }
        let desktop = server.pair(&token).await;
        let (status, body) = server
            .send(Call::new(Method::GET, "/v1/batches?after=1").token(&desktop))
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["more"], false);
        assert_eq!(body["batches"][0]["seq"], 2);
        assert_eq!(body["batches"][0]["deviceId"], laptop.as_str());
        assert_eq!(body["batches"][0]["data"], STANDARD.encode("sealed two"));
    }

    #[tokio::test]
    async fn empty_too_big_and_over_the_cap_batches_are_refused() {
        let server = Server::with(NewGroups::Open, |config| config.group_max_bytes = 1_500_000);
        let (_, _, token) = server.first_device().await;
        let upload = |size: usize| {
            Call::new(Method::POST, "/v1/batches")
                .token(&token)
                .bytes(&vec![7; size])
        };
        assert_eq!(server.send(upload(0)).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            server.send(upload(MAX_BATCH_BYTES + 1)).await.0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(server.send(upload(1_000_000)).await.0, StatusCode::OK);
        let (status, body) = server.send(upload(600_000)).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(body["error"], "this sync group is full");
    }

    #[tokio::test]
    async fn pairing_relays_messages_then_admits_the_new_device() {
        let server = Server::new(NewGroups::Open);
        let (group, _, host) = server.first_device().await;
        let (_, opened) = server.send(Call::new(Method::POST, "/v1/pairings").token(&host)).await;
        let slot = opened["slot"].as_str().unwrap().to_string();
        assert_eq!(opened["expiresAt"], 1_000 + PAIRING_MS);

        let (_, joined) = server
            .send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
            .await;
        let key = joined["joinerKey"].as_str().unwrap().to_string();
        let messages = format!("/v1/pairings/{slot}/messages");
        let (status, _) = server
            .send(
                Call::new(Method::POST, &messages)
                    .token(&key)
                    .bytes(b"pake from the new device"),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, inbox) = server.send(Call::new(Method::GET, &messages).token(&host)).await;
        assert_eq!(inbox["joined"], true);
        assert_eq!(inbox["messages"][0], STANDARD.encode("pake from the new device"));
        server
            .send(
                Call::new(Method::POST, &messages)
                    .token(&host)
                    .bytes(b"sealed group key"),
            )
            .await;
        let (_, inbox) = server.send(Call::new(Method::GET, &messages).token(&key)).await;
        assert_eq!(inbox["messages"][0], STANDARD.encode("sealed group key"));

        let collect = format!("/v1/pairings/{slot}/device");
        assert_eq!(
            server.send(Call::new(Method::GET, &collect).token(&key)).await.0,
            StatusCode::NO_CONTENT
        );
        // Only the device that opened the pairing can admit, not the new one.
        let admit = format!("/v1/pairings/{slot}/admit");
        let admit_call = |token| {
            Call::new(Method::POST, &admit)
                .token(token)
                .json(json!({ "label": label("desktop") }))
        };
        assert_eq!(server.send(admit_call(&key)).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(server.send(admit_call(&host)).await.0, StatusCode::OK);
        // And only the new device can collect.
        assert_eq!(
            server.send(Call::new(Method::GET, &collect).token(&host)).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (status, device) = server.send(Call::new(Method::GET, &collect).token(&key)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(device["groupId"], group.as_str());

        let new_token = device["token"].as_str().unwrap();
        let (_, devices) = server
            .send(Call::new(Method::GET, "/v1/devices").token(new_token))
            .await;
        assert_eq!(devices["devices"].as_array().unwrap().len(), 2);
        assert_eq!(devices["devices"][1]["label"], label("desktop"));
        assert_eq!(devices["you"], device["deviceId"]);
    }

    #[tokio::test]
    async fn ending_a_pairing_burns_it_and_it_expires() {
        let server = Server::new(NewGroups::Open);
        let (_, _, host) = server.first_device().await;
        let (_, opened) = server.send(Call::new(Method::POST, "/v1/pairings").token(&host)).await;
        let slot = opened["slot"].as_str().unwrap().to_string();
        let (_, joined) = server
            .send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
            .await;
        let key = joined["joinerKey"].as_str().unwrap();
        let (status, _) = server
            .send(Call::new(Method::DELETE, &format!("/v1/pairings/{slot}")).token(key))
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            server
                .send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
                .await
                .0,
            StatusCode::NOT_FOUND
        );

        let (_, opened) = server.send(Call::new(Method::POST, "/v1/pairings").token(&host)).await;
        let slot = opened["slot"].as_str().unwrap().to_string();
        server.advance(PAIRING_MS);
        assert_eq!(
            server
                .send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn ending_after_admitting_but_before_collecting_removes_the_new_device() {
        let server = Server::new(NewGroups::Open);
        let (_, _, host) = server.first_device().await;
        let (_, opened) = server.send(Call::new(Method::POST, "/v1/pairings").token(&host)).await;
        let slot = opened["slot"].as_str().unwrap().to_string();
        server
            .send(Call::new(Method::POST, &format!("/v1/pairings/{slot}/join")))
            .await;
        server
            .send(
                Call::new(Method::POST, &format!("/v1/pairings/{slot}/admit"))
                    .token(&host)
                    .json(json!({ "label": label("desktop") })),
            )
            .await;
        server
            .send(Call::new(Method::DELETE, &format!("/v1/pairings/{slot}")).token(&host))
            .await;
        let (_, devices) = server.send(Call::new(Method::GET, "/v1/devices").token(&host)).await;
        assert_eq!(devices["devices"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_expired_pairing_removes_its_uncollected_device() {
        let server = Server::new(NewGroups::Open);
        let (_, _, host) = server.first_device().await;
        server.admit_without_collecting(&host).await;
        assert_eq!(server.device_count(&host).await, 2);
        server.advance(PAIRING_MS);
        forget_uncollected(&server.app).await.unwrap();
        assert_eq!(server.device_count(&host).await, 1);
    }

    #[tokio::test]
    async fn a_replaced_pairing_removes_its_uncollected_device() {
        let server = Server::new(NewGroups::Open);
        let (_, _, host) = server.first_device().await;
        server.admit_without_collecting(&host).await;
        let (status, _) = server.send(Call::new(Method::POST, "/v1/pairings").token(&host)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(server.device_count(&host).await, 1);
    }

    #[tokio::test]
    async fn a_removed_device_is_signed_out_and_the_last_one_leaving_deletes_the_group() {
        let server = Server::new(NewGroups::FirstOnly);
        let (_, laptop_id, laptop) = server.first_device().await;
        let desktop = server.pair(&laptop).await;
        let (_, devices) = server.send(Call::new(Method::GET, "/v1/devices").token(&desktop)).await;
        let desktop_id = devices["you"].as_str().unwrap().to_string();

        // Remove the laptop from the desktop, as with a lost laptop.
        let (status, _) = server
            .send(Call::new(Method::DELETE, &format!("/v1/devices/{laptop_id}")).token(&desktop))
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            server
                .send(Call::new(Method::GET, "/v1/devices").token(&laptop))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        // Then the desktop leaves, and with no devices left the server takes a first group again.
        server
            .send(Call::new(Method::DELETE, &format!("/v1/devices/{desktop_id}")).token(&desktop))
            .await;
        server.first_device().await;
    }

    #[tokio::test]
    async fn joining_is_rate_limited_by_the_proxy_header_when_set() {
        let server = Server::with(NewGroups::Open, |config| {
            config.client_ip_header = Some(axum::http::HeaderName::from_static("cf-connecting-ip"));
        });
        let join = |ip| {
            let mut call = Call::new(Method::POST, "/v1/pairings/0000/join");
            call.ip = Some(ip);
            call
        };
        for _ in 0..10 {
            assert_eq!(server.send(join("203.0.113.5")).await.0, StatusCode::NOT_FOUND);
        }
        assert_eq!(server.send(join("203.0.113.5")).await.0, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(server.send(join("203.0.113.6")).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn live_connections_hear_of_the_others_batches_and_messages() {
        let server = Server::new(NewGroups::Open);
        let (_, laptop, laptop_token) = server.first_device().await;
        let desktop_token = server.pair(&laptop_token).await;
        let mut laptop_events = server.listen(&laptop_token).await;
        let mut desktop_events = server.listen(&desktop_token).await;
        assert_eq!(laptop_events.next().await.unwrap().0, "ready");
        assert_eq!(desktop_events.next().await.unwrap().0, "ready");

        let (status, _) = server
            .send(
                Call::new(Method::POST, "/v1/batches")
                    .token(&laptop_token)
                    .bytes(b"sealed"),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(desktop_events.next().await, Some(("batch".into(), json!({ "seq": 1 }))));
        // Not its own.
        assert_eq!(laptop_events.next().await, None);

        let (status, _) = server
            .send(Call::new(Method::POST, "/v1/live").token(&laptop_token).bytes(b"idle?"))
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            desktop_events.next().await,
            Some((
                "message".into(),
                json!({ "deviceId": laptop, "data": STANDARD.encode("idle?") })
            ))
        );
        let too_long = vec![1; MAX_LIVE_MESSAGE_BYTES + 1];
        let (status, _) = server
            .send(
                Call::new(Method::POST, "/v1/live")
                    .token(&laptop_token)
                    .bytes(&too_long),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn live_connections_end_when_the_device_goes_or_connects_again() {
        let server = Server::new(NewGroups::Open);
        let (_, _, laptop_token) = server.first_device().await;
        let desktop_token = server.pair(&laptop_token).await;
        let (_, devices) = server
            .send(Call::new(Method::GET, "/v1/devices").token(&desktop_token))
            .await;
        let desktop = devices["you"].as_str().unwrap().to_string();

        let mut first = server.listen(&desktop_token).await;
        assert_eq!(first.next().await.unwrap().0, "ready");
        let mut second = server.listen(&desktop_token).await;
        assert_eq!(second.next().await.unwrap().0, "ready");
        assert!(first.body.frame().await.is_none(), "the older one ends");

        let (status, _) = server
            .send(Call::new(Method::DELETE, &format!("/v1/devices/{desktop}")).token(&laptop_token))
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(second.body.frame().await.is_none());
        assert_eq!(
            server
                .send(Call::new(Method::GET, "/v1/live").token(&desktop_token))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn live_connections_end_when_the_server_shuts_down() {
        let server = Server::new(NewGroups::Open);
        let (_, _, token) = server.first_device().await;
        let mut events = server.listen(&token).await;
        assert_eq!(events.next().await.unwrap().0, "ready");
        server.app.close_live();
        assert!(events.body.frame().await.is_none());
    }
}
