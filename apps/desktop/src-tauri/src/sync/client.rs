// Talking to a sync server (apps/sync-server; its README lists the API). Only sealed data goes
// through here: batches and labels are sealed by the group's key (crypto.rs), and pairing
// messages by the pairing's one-off key (pairing.rs).

use std::fmt;
use std::sync::Once;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

/// Sent with every request; the server drops anything without it. Not a secret.
const PROTOCOL: (&str, &str) = ("Seiza-Protocol", "1");

/// Why a request to the server didn't work.
#[derive(Debug)]
pub enum ClientError {
    /// The server couldn't be reached, or took too long. The cause is kept for debugging.
    Offline(#[allow(dead_code)] reqwest::Error),
    /// The server turned the request away, with its reason.
    Refused { status: StatusCode, message: String },
    /// The answer wasn't what this version of the app expects.
    Unexpected,
}

impl ClientError {
    /// This device's token is no longer known, e.g. because another device removed it. Only the
    /// sync server's own answer counts, not a 401 from a proxy in front of it (say, one that
    /// wants a sign-in), which would otherwise turn sync off here.
    pub fn is_signed_out(&self) -> bool {
        matches!(
            self,
            ClientError::Refused {
                status: StatusCode::UNAUTHORIZED,
                message,
            } if message == "unknown device"
        )
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Offline(_) => f.write_str("couldn't reach the sync server"),
            ClientError::Refused { message, .. } => write!(f, "the sync server said: {message}"),
            ClientError::Unexpected => f.write_str("the sync server sent something this version of Seiza can't read"),
        }
    }
}

type Result<T> = std::result::Result<T, ClientError>;

/// A device's place in a group, from starting it or being added to it.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Membership {
    pub group_id: String,
    pub device_id: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    /// The device's name, sealed (`GroupKey::open_label`).
    pub label: String,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct Devices {
    pub devices: Vec<Device>,
    /// This device's id.
    pub you: String,
}

#[derive(Debug)]
pub struct Batch {
    pub seq: i64,
    pub device_id: String,
    /// Sealed (`GroupKey::open` with `Purpose::Batch`).
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct Batches {
    pub batches: Vec<Batch>,
    /// More are waiting after these.
    pub more: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    /// The number at the front of the code.
    pub slot: String,
    pub expires_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Joined {
    /// The new device's key for the rest of the pairing, used like a token.
    pub joiner_key: String,
}

#[derive(Debug)]
pub struct Inbox {
    pub messages: Vec<Vec<u8>>,
    /// The new device has joined the pairing.
    pub joined: bool,
}

/// One sync server, at an address like `https://sync.example.com`.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    address: String,
}

impl Client {
    pub fn new(address: &str) -> Result<Self> {
        // As the updater does (it uses the same HTTP client): rustls needs a crypto provider,
        // and ring is the one already built in.
        static PROVIDER: Once = Once::new();
        PROVIDER.call_once(|| {
            if rustls::crypto::CryptoProvider::get_default().is_none() {
                let _ = rustls::crypto::ring::default_provider().install_default();
            }
        });
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(ClientError::Offline)?;
        Ok(Self {
            http,
            address: address.trim_end_matches('/').to_owned(),
        })
    }

    fn request(&self, method: Method, path: &str, token: Option<&str>) -> RequestBuilder {
        let request = self
            .http
            .request(method, format!("{}/v1{path}", self.address))
            .header(PROTOCOL.0, PROTOCOL.1);
        match token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    /// Whether a Seiza sync server answers at this address.
    pub async fn health(&self) -> Result<()> {
        let response = checked(self.http.get(format!("{}/health", self.address)).send().await).await?;
        match response.text().await {
            Ok(text) if text.trim() == "ok" => Ok(()),
            _ => Err(ClientError::Unexpected),
        }
    }

    /// Starts a group with this device as its first, named by its sealed `label`.
    pub async fn create_group(&self, label: &str, invite: Option<&str>) -> Result<Membership> {
        let body = json!({ "label": label, "invite": invite });
        json_from(self.request(Method::POST, "/groups", None).json(&body).send().await).await
    }

    pub async fn devices(&self, token: &str) -> Result<Devices> {
        json_from(self.request(Method::GET, "/devices", Some(token)).send().await).await
    }

    /// Removes a device from the group: another one, or this one to leave sync.
    pub async fn remove_device(&self, token: &str, device_id: &str) -> Result<()> {
        let path = format!("/devices/{}", segment(device_id));
        empty_from(self.request(Method::DELETE, &path, Some(token)).send().await).await
    }

    /// Adds a sealed batch, returning its place in the group's order.
    pub async fn upload_batch(&self, token: &str, sealed: Vec<u8>) -> Result<i64> {
        #[derive(Deserialize)]
        struct Seq {
            seq: i64,
        }
        let response = self
            .request(Method::POST, "/batches", Some(token))
            .body(sealed)
            .send()
            .await;
        Ok(json_from::<Seq>(response).await?.seq)
    }

    /// The group's batches after `after`, in order, a page at a time.
    pub async fn batches_after(&self, token: &str, after: i64) -> Result<Batches> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            seq: i64,
            device_id: String,
            data: String,
        }
        #[derive(Deserialize)]
        struct Page {
            batches: Vec<Wire>,
            more: bool,
        }
        let path = format!("/batches?after={after}");
        let page: Page = json_from(self.request(Method::GET, &path, Some(token)).send().await).await?;
        let batches = page
            .batches
            .into_iter()
            .map(|batch| {
                Ok(Batch {
                    seq: batch.seq,
                    device_id: batch.device_id,
                    data: STANDARD.decode(batch.data).map_err(|_| ClientError::Unexpected)?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Batches {
            batches,
            more: page.more,
        })
    }

    /// Opens a pairing for a new device (the host's side).
    pub async fn open_pairing(&self, token: &str) -> Result<Pairing> {
        json_from(self.request(Method::POST, "/pairings", Some(token)).send().await).await
    }

    /// Joins the pairing with this number (the new device's side).
    pub async fn join_pairing(&self, slot: &str) -> Result<Joined> {
        let path = format!("/pairings/{}/join", segment(slot));
        json_from(self.request(Method::POST, &path, None).send().await).await
    }

    /// Sends the other side a message. `key` is the host's token or the new device's joiner key.
    pub async fn send_message(&self, key: &str, slot: &str, message: Vec<u8>) -> Result<()> {
        let path = format!("/pairings/{}/messages", segment(slot));
        empty_from(self.request(Method::POST, &path, Some(key)).body(message).send().await).await
    }

    /// The other side's messages after the first `after`.
    pub async fn messages(&self, key: &str, slot: &str, after: usize) -> Result<Inbox> {
        #[derive(Deserialize)]
        struct Wire {
            messages: Vec<String>,
            joined: bool,
        }
        let path = format!("/pairings/{}/messages?after={after}", segment(slot));
        let wire: Wire = json_from(self.request(Method::GET, &path, Some(key)).send().await).await?;
        let messages = wire
            .messages
            .iter()
            .map(|message| STANDARD.decode(message).map_err(|_| ClientError::Unexpected))
            .collect::<Result<_>>()?;
        Ok(Inbox {
            messages,
            joined: wire.joined,
        })
    }

    /// Adds the new device to the group under its sealed `label`, once its person said Add.
    pub async fn admit(&self, token: &str, slot: &str, label: &str) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Admitted {
            device_id: String,
        }
        let path = format!("/pairings/{}/admit", segment(slot));
        let request = self
            .request(Method::POST, &path, Some(token))
            .json(&json!({ "label": label }));
        Ok(json_from::<Admitted>(request.send().await).await?.device_id)
    }

    /// The new device's membership once it's been added, or `None` while it waits.
    pub async fn collect_device(&self, joiner_key: &str, slot: &str) -> Result<Option<Membership>> {
        let path = format!("/pairings/{}/device", segment(slot));
        let response = checked(self.request(Method::GET, &path, Some(joiner_key)).send().await).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        response.json().await.map(Some).map_err(|_| ClientError::Unexpected)
    }

    /// Ends the pairing, e.g. because the code was wrong, so it can't be tried again.
    pub async fn end_pairing(&self, key: &str, slot: &str) -> Result<()> {
        let path = format!("/pairings/{}", segment(slot));
        empty_from(self.request(Method::DELETE, &path, Some(key)).send().await).await
    }
}

/// Keeps an id or number to one path segment, whatever it holds.
fn segment(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Turns a refusal into its error, with the server's reason.
async fn checked(response: reqwest::Result<Response>) -> Result<Response> {
    let response = response.map_err(ClientError::Offline)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    #[derive(Deserialize)]
    struct Refusal {
        error: String,
    }
    let message = match response.json::<Refusal>().await {
        Ok(refusal) => refusal.error,
        Err(_) => status.canonical_reason().unwrap_or("an error").to_owned(),
    };
    Err(ClientError::Refused { status, message })
}

async fn json_from<T: DeserializeOwned>(response: reqwest::Result<Response>) -> Result<T> {
    checked(response)
        .await?
        .json()
        .await
        .map_err(|_| ClientError::Unexpected)
}

async fn empty_from(response: reqwest::Result<Response>) -> Result<()> {
    checked(response).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crypto::{GroupKey, Purpose};
    use crate::sync::pairing::{Code, Host, Joiner};

    #[test]
    fn only_the_sync_server_signs_a_device_out() {
        let refused = |status, message: &str| ClientError::Refused {
            status,
            message: message.into(),
        };
        assert!(refused(StatusCode::UNAUTHORIZED, "unknown device").is_signed_out());
        // A proxy in front of the server that wants a sign-in, say.
        assert!(!refused(StatusCode::UNAUTHORIZED, "Unauthorized").is_signed_out());
        assert!(!refused(StatusCode::FORBIDDEN, "unknown device").is_signed_out());
    }

    #[test]
    fn path_segments_stay_one_segment() {
        assert_eq!(segment("4821"), "4821");
        assert_eq!(segment("a-B_9"), "a-B_9");
        assert_eq!(segment("../x?y"), "%2E%2E%2Fx%3Fy");
    }

    /// The whole flow against a real server: start a group, add a device by pairing code, and
    /// send a batch from one to the other. Needs one running with an empty database:
    ///
    ///   cd apps/sync-server && SEIZA_SYNC_DB=$(mktemp -d)/test.db cargo run
    ///   SEIZA_SYNC_TEST_SERVER=http://127.0.0.1:8080 cargo test -- --ignored against_a_server
    #[tokio::test]
    #[ignore = "needs a running sync server; see the comment"]
    async fn against_a_server() {
        let address = std::env::var("SEIZA_SYNC_TEST_SERVER").expect("set SEIZA_SYNC_TEST_SERVER");
        let client = Client::new(&address).unwrap();

        // The first device makes the group's key and starts the group.
        let key = GroupKey::generate();
        let first = client.create_group(&key.seal_label("Mac mini"), None).await.unwrap();

        // It opens a pairing and shows the code; the new device types it in.
        let pairing = client.open_pairing(&first.token).await.unwrap();
        let code = Code::new(&pairing.slot);
        let typed = Code::parse(&code.to_string()).unwrap();
        let joined = client.join_pairing(typed.slot()).await.unwrap();

        let (host, hello) = Host::start(&code, &first.group_id);
        client.send_message(&first.token, &pairing.slot, hello).await.unwrap();

        let inbox = client.messages(&joined.joiner_key, typed.slot(), 0).await.unwrap();
        let (joiner, reply) = Joiner::reply(&typed, &inbox.messages[0], "MacBook Air").unwrap();
        client
            .send_message(&joined.joiner_key, typed.slot(), reply)
            .await
            .unwrap();

        let inbox = client.messages(&first.token, &pairing.slot, 0).await.unwrap();
        assert!(inbox.joined);
        let checked = host.check(&inbox.messages[0]).unwrap();
        assert_eq!(checked.device_name, "MacBook Air");

        // Its person says Add.
        client
            .send_message(&first.token, &pairing.slot, checked.welcome(&key, "sky"))
            .await
            .unwrap();
        let label = key.seal_label(&checked.device_name);
        client.admit(&first.token, &pairing.slot, &label).await.unwrap();

        // The new device takes the group's key from the welcome, then collects its token.
        let inbox = client.messages(&joined.joiner_key, typed.slot(), 1).await.unwrap();
        let received = joiner.finish(&inbox.messages[0]).unwrap().key;
        assert_eq!(received, key);
        let membership = client
            .collect_device(&joined.joiner_key, typed.slot())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(membership.group_id, first.group_id);

        // Both are in the group now, and a batch from one reaches the other.
        let devices = client.devices(&membership.token).await.unwrap();
        assert_eq!(devices.devices.len(), 2);
        assert_eq!(devices.you, membership.device_id);
        let names: Vec<String> = devices
            .devices
            .iter()
            .map(|d| received.open_label(&d.label).unwrap())
            .collect();
        assert_eq!(names, ["Mac mini", "MacBook Air"]);

        let sealed = received.seal(Purpose::Batch, &membership.group_id, b"a session");
        let seq = client.upload_batch(&membership.token, sealed).await.unwrap();
        let page = client.batches_after(&first.token, seq - 1).await.unwrap();
        assert_eq!(
            key.open(Purpose::Batch, &first.group_id, &page.batches[0].data)
                .unwrap(),
            b"a session"
        );

        // The pairing is over, and the new device can leave again.
        assert!(client.messages(&joined.joiner_key, typed.slot(), 0).await.is_err());
        client
            .remove_device(&membership.token, &membership.device_id)
            .await
            .unwrap();
        assert!(client.devices(&membership.token).await.unwrap_err().is_signed_out());
    }
}
