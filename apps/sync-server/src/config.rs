use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::http::HeaderName;

/// Who can start a new sync group on this server (PLAN §8, Sync).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewGroups {
    /// A group can be started only while the server has none, so the owner's first device starts
    /// theirs and then the server is closed to new ones. The default.
    FirstOnly,
    /// Each new group needs a one-time invite code from `seiza-sync invite`.
    Invites,
    /// Anyone can start one, within the size caps and rate limits.
    Open,
}

impl fmt::Display for NewGroups {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FirstOnly => "first-group-only",
            Self::Invites => "invites",
            Self::Open => "open",
        })
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub db_path: PathBuf,
    pub new_groups: NewGroups,
    /// The most a group can store, in bytes.
    pub group_max_bytes: u64,
    /// A header holding the client's address, set by a proxy in front (e.g. Cloudflare's
    /// `CF-Connecting-IP`). Without it, rate limits go by the connection's address.
    pub client_ip_header: Option<HeaderName>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let addr = match lookup("SEIZA_SYNC_ADDR") {
            Some(addr) => addr
                .parse()
                .map_err(|_| format!("SEIZA_SYNC_ADDR \"{addr}\" isn't an address like 0.0.0.0:8080"))?,
            None => SocketAddr::from(([0, 0, 0, 0], 8080)),
        };
        let db_path = lookup("SEIZA_SYNC_DB").map_or_else(|| PathBuf::from("seiza-sync.db"), PathBuf::from);
        let new_groups = match lookup("SEIZA_SYNC_NEW_GROUPS").as_deref() {
            None | Some("first-group-only") => NewGroups::FirstOnly,
            Some("invites") => NewGroups::Invites,
            Some("open") => NewGroups::Open,
            Some(other) => {
                return Err(format!(
                    "SEIZA_SYNC_NEW_GROUPS \"{other}\" should be first-group-only, invites or open"
                ));
            }
        };
        let group_max_mb: u64 = match lookup("SEIZA_SYNC_GROUP_MAX_MB") {
            Some(mb) => mb
                .parse()
                .ok()
                .filter(|mb| *mb > 0)
                .ok_or_else(|| format!("SEIZA_SYNC_GROUP_MAX_MB \"{mb}\" should be a whole number of megabytes"))?,
            None => 50,
        };
        let client_ip_header = match lookup("SEIZA_SYNC_CLIENT_IP_HEADER") {
            Some(name) if !name.is_empty() => Some(
                HeaderName::try_from(name.as_str())
                    .map_err(|_| format!("SEIZA_SYNC_CLIENT_IP_HEADER \"{name}\" isn't a header name"))?,
            ),
            _ => None,
        };
        Ok(Self {
            addr,
            db_path,
            new_groups,
            group_max_bytes: group_max_mb * 1_000_000,
            client_ip_header,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_lookup(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        })
    }

    #[test]
    fn defaults_to_one_group_on_port_8080() {
        let config = config(&[]).unwrap();
        assert_eq!(config.addr, SocketAddr::from(([0, 0, 0, 0], 8080)));
        assert_eq!(config.new_groups, NewGroups::FirstOnly);
        assert_eq!(config.group_max_bytes, 50_000_000);
        assert!(config.client_ip_header.is_none());
    }

    #[test]
    fn reads_each_setting() {
        let config = config(&[
            ("SEIZA_SYNC_ADDR", "127.0.0.1:9000"),
            ("SEIZA_SYNC_DB", "/data/sync.db"),
            ("SEIZA_SYNC_NEW_GROUPS", "invites"),
            ("SEIZA_SYNC_GROUP_MAX_MB", "5"),
            ("SEIZA_SYNC_CLIENT_IP_HEADER", "CF-Connecting-IP"),
        ])
        .unwrap();
        assert_eq!(config.addr.port(), 9000);
        assert_eq!(config.db_path, PathBuf::from("/data/sync.db"));
        assert_eq!(config.new_groups, NewGroups::Invites);
        assert_eq!(config.group_max_bytes, 5_000_000);
        assert_eq!(config.client_ip_header.unwrap().as_str(), "cf-connecting-ip");
    }

    #[test]
    fn says_what_is_wrong() {
        assert!(
            config(&[("SEIZA_SYNC_NEW_GROUPS", "everyone")])
                .unwrap_err()
                .contains("first-group-only")
        );
        assert!(config(&[("SEIZA_SYNC_GROUP_MAX_MB", "0")]).is_err());
        assert!(config(&[("SEIZA_SYNC_ADDR", "port 80")]).is_err());
    }
}
