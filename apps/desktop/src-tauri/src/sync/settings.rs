// What this device keeps for sync (PLAN §8, Sync), saved in the `state` table under `sync`, next
// to the sky's secret. None of it goes into backups or to the window: the token signs this device
// in to the server, and the key opens everything the group has stored there.

use std::net::IpAddr;

use reqwest::Url;
use serde::{Deserialize, Serialize};

use super::crypto::GroupKey;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    /// The server's address, as `server_address` left it.
    pub server: String,
    pub group_id: String,
    pub device_id: String,
    /// This device's token for the server.
    pub token: String,
    /// The group's key, in base64.
    pub key: String,
    /// This device's name, as the group's other devices see it.
    pub device_name: String,
    /// The last of this device's changes that's been sent (records.rs's counter).
    pub sent: i64,
    /// The last of the group's batches that's been fetched.
    pub fetched: i64,
    /// When this device last sent or fetched (epoch ms).
    pub last_synced_at: Option<i64>,
    /// Start sessions from my other computers here (live sessions, step 8). On when it joins.
    pub follow: bool,
}

impl SyncSettings {
    /// A device's settings on joining or starting a group, before anything's been synced.
    pub fn new(server: String, membership: super::client::Membership, key: &GroupKey, device_name: String) -> Self {
        use base64::Engine;
        Self {
            server,
            group_id: membership.group_id,
            device_id: membership.device_id,
            token: membership.token,
            key: base64::engine::general_purpose::STANDARD.encode(key.as_bytes()),
            device_name,
            sent: 0,
            fetched: 0,
            last_synced_at: None,
            follow: true,
        }
    }

    pub fn group_key(&self) -> Option<GroupKey> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&self.key).ok()?;
        GroupKey::from_bytes(&bytes)
    }
}

// The token and the key are never printed, so they can't end up in a log.
impl std::fmt::Debug for SyncSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncSettings")
            .field("server", &self.server)
            .field("group_id", &self.group_id)
            .field("device_id", &self.device_id)
            .field("sent", &self.sent)
            .field("fetched", &self.fetched)
            .finish_non_exhaustive()
    }
}

/// Why an address can't be used.
#[derive(Debug, PartialEq, Eq)]
pub enum BadAddress {
    Unreadable,
    /// Plain `http://` to somewhere that isn't this computer or the local network.
    NotHttps,
}

impl std::fmt::Display for BadAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BadAddress::Unreadable => {
                f.write_str("that doesn't look like a server's address, e.g. https://sync.example.com")
            }
            BadAddress::NotHttps => f.write_str(
                "a server outside your network needs an https:// address, so this computer's token stays private",
            ),
        }
    }
}

/// Tidies a server's address as it was typed: `https://` if no scheme was given, and no slash on
/// the end. Plain `http://` only reaches this computer or the local network, since the device's
/// token travels with every request (what it sends is encrypted either way).
pub fn server_address(typed: &str) -> Result<String, BadAddress> {
    let typed = typed.trim();
    let with_scheme = if typed.contains("://") {
        typed.to_owned()
    } else {
        format!("https://{typed}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| BadAddress::Unreadable)?;
    let host = url.host_str().ok_or(BadAddress::Unreadable)?;
    if host.is_empty()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(BadAddress::Unreadable);
    }
    match url.scheme() {
        "https" => {}
        "http" if local(host) => {}
        "http" => return Err(BadAddress::NotHttps),
        _ => return Err(BadAddress::Unreadable),
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// This computer, or an address on the local network.
fn local(host: &str) -> bool {
    match host.trim_start_matches('[').trim_end_matches(']').parse() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
        Err(_) => {
            let name = host.to_ascii_lowercase();
            name == "localhost" || name.ends_with(".localhost") || name.ends_with(".local") || name.ends_with(".lan")
        }
    }
}

/// This computer's name, to suggest as its name in the group: the one in System Settings on a
/// Mac, otherwise its host name.
pub fn computer_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        && output.status.success()
    {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !name.is_empty() {
            return name;
        }
    }
    sysinfo::System::host_name()
        .map(|name| name.trim_end_matches(".local").to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "This computer".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_tidied() {
        assert_eq!(server_address("sync.example.com").unwrap(), "https://sync.example.com");
        assert_eq!(
            server_address(" https://sync.example.com/ ").unwrap(),
            "https://sync.example.com"
        );
        assert_eq!(
            server_address("https://example.com/seiza/").unwrap(),
            "https://example.com/seiza"
        );
        assert_eq!(
            server_address("http://192.168.1.20:8080").unwrap(),
            "http://192.168.1.20:8080"
        );
        assert_eq!(
            server_address("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        assert_eq!(
            server_address("http://nas.local:8080").unwrap(),
            "http://nas.local:8080"
        );
        assert_eq!(server_address("http://[::1]:8080").unwrap(), "http://[::1]:8080");
    }

    #[test]
    fn plain_http_stays_on_the_local_network() {
        assert_eq!(server_address("http://sync.example.com"), Err(BadAddress::NotHttps));
        assert_eq!(server_address("http://8.8.8.8"), Err(BadAddress::NotHttps));
    }

    #[test]
    fn odd_addresses_are_turned_down() {
        for typed in [
            "",
            "https://",
            "ftp://example.com",
            "https://me:pw@example.com",
            "https://example.com/?a=1",
            "https://example.com/#x",
        ] {
            assert_eq!(server_address(typed), Err(BadAddress::Unreadable), "{typed}");
        }
    }

    #[test]
    fn the_token_and_key_never_print() {
        let settings = SyncSettings::new(
            "https://sync.example.com".into(),
            super::super::client::Membership {
                group_id: "g".into(),
                device_id: "d".into(),
                token: "the-token".into(),
            },
            &GroupKey::generate(),
            "Mac".into(),
        );
        let printed = format!("{settings:?}");
        assert!(!printed.contains("the-token") && !printed.contains(&settings.key));
        assert!(settings.group_key().is_some());
    }
}
