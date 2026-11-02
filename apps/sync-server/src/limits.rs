use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;

/// What a rate limit is for. Each has its own allowance per address each minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Any request.
    Request,
    /// Joining a pairing, where a stranger could try codes.
    Join,
    /// Starting a group.
    NewGroup,
}

impl Kind {
    fn per_minute(self) -> u32 {
        match self {
            // Pairing checks for the other side's messages about once a second.
            Self::Request => 300,
            Self::Join => 10,
            Self::NewGroup => 3,
        }
    }
}

const MINUTE_MS: i64 = 60_000;
/// Old windows are cleared out once the table holds this many, so it can't grow without end.
const MAX_ENTRIES: usize = 10_000;

/// Counts requests per address in one-minute windows, in memory: a restart forgets them, which
/// is fine for limits this short. Cloudflare's rate-limit rules can sit in front as well.
#[derive(Default)]
pub struct Limits {
    windows: Mutex<HashMap<(IpAddr, Kind), (i64, u32)>>,
}

impl Limits {
    /// Counts a request, and says whether it's within the allowance.
    pub fn allow(&self, ip: IpAddr, kind: Kind, now: i64) -> bool {
        let mut windows = self.windows.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if windows.len() >= MAX_ENTRIES {
            windows.retain(|_, (start, _)| now - *start < MINUTE_MS);
        }
        let (start, count) = windows.entry((ip, kind)).or_insert((now, 0));
        if now - *start >= MINUTE_MS {
            *start = now;
            *count = 0;
        }
        *count += 1;
        *count <= kind.per_minute()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_the_limit_each_minute_per_address() {
        let limits = Limits::default();
        let (one, two) = ("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());
        for _ in 0..10 {
            assert!(limits.allow(one, Kind::Join, 0));
        }
        assert!(!limits.allow(one, Kind::Join, 59_999));
        assert!(limits.allow(two, Kind::Join, 0));
        assert!(limits.allow(one, Kind::Request, 0));
        assert!(limits.allow(one, Kind::Join, 60_000));
    }
}
