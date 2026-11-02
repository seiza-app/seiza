// Pairing a new device (PLAN §8, Sync). A device in the group opens a pairing and gets a short
// slot number, which its app shows as the start of the pairing code. The words after it are the
// PAKE's secret and never reach the server. The new device joins by the slot, then the two swap
// a few opaque messages through it (the PAKE and its key confirmation, then the group key,
// encrypted) until the group's device admits the new one, which then collects its token.
//
// Pairings live in memory only: they last five minutes, and a restart just means opening a new
// one.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::db::NewDevice;
use crate::error::ApiError;
use crate::tokens;

/// How long a pairing stays open.
pub const PAIRING_MS: i64 = 5 * 60_000;
/// The most messages each side can send in one pairing. The exchange needs about three.
const MAX_MESSAGES: usize = 8;
/// The largest message, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 4096;
/// The most pairings open at once on the whole server.
const MAX_OPEN: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The device already in the group.
    Host,
    /// The new device.
    Joiner,
}

struct Pairing {
    group_id: String,
    host_device_id: String,
    expires_at: i64,
    /// The hash of the key the new device got when it joined. None until one joins.
    joiner_key_hash: Option<Vec<u8>>,
    from_host: Vec<Vec<u8>>,
    from_joiner: Vec<Vec<u8>>,
    /// Set once the host lets the new device in, until the new device collects it.
    admitted: Option<NewDevice>,
}

/// What one side sees of a pairing.
#[derive(Debug, PartialEq, Eq)]
pub struct Inbox {
    /// The other side's messages after the ones already read.
    pub messages: Vec<Vec<u8>>,
    /// Whether a new device has joined.
    pub joined: bool,
}

#[derive(Default)]
pub struct Pairings {
    open: Mutex<HashMap<String, Pairing>>,
    /// New devices that were admitted but whose pairing went (expired or replaced) before they
    /// collected their token, until they're handed back to be removed.
    uncollected: Mutex<Vec<NewDevice>>,
}

impl Pairings {
    fn lock(&self, now: i64) -> std::sync::MutexGuard<'_, HashMap<String, Pairing>> {
        let mut open = self.open.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.drop_where(&mut open, |pairing| pairing.expires_at <= now);
        open
    }

    /// Drops the pairings that match, setting aside any new device they admitted.
    fn drop_where(&self, open: &mut HashMap<String, Pairing>, matches: impl Fn(&Pairing) -> bool) {
        let mut uncollected = Vec::new();
        open.retain(|_, pairing| {
            if !matches(pairing) {
                return true;
            }
            uncollected.extend(pairing.admitted.take());
            false
        });
        if !uncollected.is_empty() {
            self.uncollected
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .append(&mut uncollected);
        }
    }

    /// Drops expired pairings, and hands back the new devices that were admitted but never
    /// collected their token, so they can be removed.
    pub fn sweep(&self, now: i64) -> Vec<NewDevice> {
        let _open = self.lock(now);
        std::mem::take(&mut *self.uncollected.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }

    /// Opens a pairing for a device in a group, replacing one that group already has open (whose
    /// uncollected device, if any, `sweep` hands back). Returns its slot and when it expires.
    pub fn open(&self, group_id: &str, host_device_id: &str, now: i64) -> Result<(String, i64), ApiError> {
        let mut open = self.lock(now);
        self.drop_where(&mut open, |pairing| pairing.group_id == group_id);
        if open.len() >= MAX_OPEN {
            return Err(ApiError::TooManyRequests);
        }
        let slot = loop {
            let slot = random_slot();
            if !open.contains_key(&slot) {
                break slot;
            }
        };
        let expires_at = now + PAIRING_MS;
        open.insert(
            slot.clone(),
            Pairing {
                group_id: group_id.to_string(),
                host_device_id: host_device_id.to_string(),
                expires_at,
                joiner_key_hash: None,
                from_host: Vec::new(),
                from_joiner: Vec::new(),
                admitted: None,
            },
        );
        Ok((slot, expires_at))
    }

    /// A new device joins a pairing by its slot. Only one can: the key it gets back is what it
    /// uses for the rest of the pairing.
    pub fn join(&self, slot: &str, now: i64) -> Result<(String, i64), ApiError> {
        let mut open = self.lock(now);
        let pairing = open.get_mut(slot).ok_or(ApiError::NotFound)?;
        if pairing.joiner_key_hash.is_some() {
            return Err(ApiError::Conflict("someone has already joined this pairing"));
        }
        let key = tokens::token();
        pairing.joiner_key_hash = Some(tokens::hash(&key));
        Ok((key, pairing.expires_at))
    }

    /// Whether this is the joiner's key for the pairing.
    pub fn is_joiner(&self, slot: &str, key: &str, now: i64) -> bool {
        let open = self.lock(now);
        open.get(slot)
            .and_then(|pairing| pairing.joiner_key_hash.as_ref())
            .is_some_and(|hash| *hash == tokens::hash(key))
    }

    /// Whether this device opened the pairing.
    pub fn is_host(&self, slot: &str, device_id: &str, now: i64) -> bool {
        let open = self.lock(now);
        open.get(slot)
            .is_some_and(|pairing| pairing.host_device_id == device_id)
    }

    pub fn send(&self, slot: &str, side: Side, message: Vec<u8>, now: i64) -> Result<(), ApiError> {
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(ApiError::TooLarge("pairing messages are at most 4 KB"));
        }
        let mut open = self.lock(now);
        let pairing = open.get_mut(slot).ok_or(ApiError::NotFound)?;
        if pairing.joiner_key_hash.is_none() {
            return Err(ApiError::Conflict("no one has joined this pairing yet"));
        }
        let outbox = match side {
            Side::Host => &mut pairing.from_host,
            Side::Joiner => &mut pairing.from_joiner,
        };
        if outbox.len() >= MAX_MESSAGES {
            return Err(ApiError::Conflict("too many messages in this pairing"));
        }
        outbox.push(message);
        Ok(())
    }

    /// The other side's messages after the first `after`.
    pub fn receive(&self, slot: &str, side: Side, after: usize, now: i64) -> Result<Inbox, ApiError> {
        let open = self.lock(now);
        let pairing = open.get(slot).ok_or(ApiError::NotFound)?;
        let inbox = match side {
            Side::Host => &pairing.from_joiner,
            Side::Joiner => &pairing.from_host,
        };
        Ok(Inbox {
            messages: inbox.iter().skip(after).cloned().collect(),
            joined: pairing.joiner_key_hash.is_some(),
        })
    }

    /// The group a pairing would admit its new device to, if one has joined and it hasn't been
    /// admitted yet.
    pub fn group_to_admit(&self, slot: &str, now: i64) -> Result<String, ApiError> {
        let open = self.lock(now);
        let pairing = open.get(slot).ok_or(ApiError::NotFound)?;
        if pairing.joiner_key_hash.is_none() {
            return Err(ApiError::Conflict("no one has joined this pairing yet"));
        }
        if pairing.admitted.is_some() {
            return Err(ApiError::Conflict("this device has already been added"));
        }
        Ok(pairing.group_id.clone())
    }

    /// Keeps the new device's token for it to collect. Gives the device back if the pairing has
    /// gone in the meantime (expired or ended), so it can be removed again.
    pub fn admit(&self, slot: &str, device: NewDevice, now: i64) -> Result<(), NewDevice> {
        let mut open = self.lock(now);
        match open.get_mut(slot) {
            Some(pairing) if pairing.admitted.is_none() => {
                pairing.admitted = Some(device);
                Ok(())
            }
            _ => Err(device),
        }
    }

    /// The new device collects its token once it's been admitted, which ends the pairing.
    pub fn collect(&self, slot: &str, now: i64) -> Result<Option<NewDevice>, ApiError> {
        let mut open = self.lock(now);
        let pairing = open.get(slot).ok_or(ApiError::NotFound)?;
        if pairing.admitted.is_none() {
            return Ok(None);
        }
        Ok(open.remove(slot).and_then(|pairing| pairing.admitted))
    }

    /// Ends a pairing, e.g. when the PAKE fails, so a wrong code can't be tried again.
    pub fn end(&self, slot: &str, now: i64) -> Option<NewDevice> {
        self.lock(now).remove(slot).and_then(|pairing| pairing.admitted)
    }
}

/// Four digits.
fn random_slot() -> String {
    let mut bytes = [0; 2];
    getrandom::fill(&mut bytes).expect("the system's random source should work");
    format!("{:04}", u16::from_le_bytes(bytes) % 10_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> NewDevice {
        NewDevice {
            group_id: "g".into(),
            device_id: "new".into(),
            token: "token".into(),
        }
    }

    #[test]
    fn swaps_messages_both_ways_then_hands_over_the_token_once() {
        let pairings = Pairings::default();
        let (slot, expires_at) = pairings.open("g", "host", 0).unwrap();
        assert_eq!((slot.len(), expires_at), (4, PAIRING_MS));
        assert!(pairings.is_host(&slot, "host", 1));
        assert!(!pairings.is_host(&slot, "someone", 1));

        let (key, _) = pairings.join(&slot, 1).unwrap();
        assert!(pairings.is_joiner(&slot, &key, 1));
        assert!(!pairings.is_joiner(&slot, "guess", 1));

        pairings.send(&slot, Side::Joiner, b"pake a".to_vec(), 2).unwrap();
        pairings.send(&slot, Side::Host, b"pake b".to_vec(), 3).unwrap();
        pairings.send(&slot, Side::Host, b"sealed key".to_vec(), 4).unwrap();
        let host_inbox = pairings.receive(&slot, Side::Host, 0, 5).unwrap();
        assert_eq!(
            host_inbox,
            Inbox {
                messages: vec![b"pake a".to_vec()],
                joined: true
            }
        );
        let joiner_inbox = pairings.receive(&slot, Side::Joiner, 1, 5).unwrap();
        assert_eq!(joiner_inbox.messages, vec![b"sealed key".to_vec()]);

        assert_eq!(pairings.collect(&slot, 6).unwrap(), None);
        assert_eq!(pairings.group_to_admit(&slot, 6).unwrap(), "g");
        pairings.admit(&slot, device(), 6).unwrap();
        assert!(pairings.group_to_admit(&slot, 6).is_err());
        assert_eq!(pairings.collect(&slot, 7).unwrap().unwrap().token, "token");
        assert_eq!(pairings.collect(&slot, 8), Err(ApiError::NotFound));
    }

    #[test]
    fn only_one_device_can_join() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        pairings.join(&slot, 1).unwrap();
        assert!(matches!(pairings.join(&slot, 2), Err(ApiError::Conflict(_))));
    }

    #[test]
    fn nothing_is_sent_or_admitted_before_someone_joins() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        assert!(matches!(
            pairings.send(&slot, Side::Host, b"x".to_vec(), 1),
            Err(ApiError::Conflict(_))
        ));
        assert!(matches!(pairings.group_to_admit(&slot, 1), Err(ApiError::Conflict(_))));
    }

    #[test]
    fn expires_after_five_minutes() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        assert!(pairings.join(&slot, PAIRING_MS - 1).is_ok());
        assert_eq!(
            pairings.receive(&slot, Side::Host, 0, PAIRING_MS).unwrap_err(),
            ApiError::NotFound
        );
        // A token that couldn't be handed over comes back, to be removed.
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        assert!(pairings.admit(&slot, device(), PAIRING_MS).is_err());
    }

    #[test]
    fn ending_a_pairing_burns_it() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        pairings.end(&slot, 1);
        assert_eq!(pairings.join(&slot, 2), Err(ApiError::NotFound));
    }

    #[test]
    fn a_group_has_one_pairing_open_at_a_time() {
        let pairings = Pairings::default();
        let (first, _) = pairings.open("g", "host", 0).unwrap();
        let (second, _) = pairings.open("g", "host", 1).unwrap();
        assert_eq!(
            pairings.join(&first, 2).err(),
            (first != second).then_some(ApiError::NotFound)
        );
        assert!(pairings.join(&second, 2).is_ok());
    }

    #[test]
    fn an_expired_pairing_hands_back_its_uncollected_device() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        pairings.join(&slot, 1).unwrap();
        pairings.admit(&slot, device(), 2).unwrap();
        assert_eq!(pairings.sweep(PAIRING_MS - 1), vec![]);
        assert_eq!(pairings.sweep(PAIRING_MS), vec![device()]);
        assert_eq!(pairings.sweep(PAIRING_MS), vec![]);
    }

    #[test]
    fn a_replaced_pairing_hands_back_its_uncollected_device() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        pairings.join(&slot, 1).unwrap();
        pairings.admit(&slot, device(), 2).unwrap();
        pairings.open("other", "host", 3).unwrap();
        assert_eq!(pairings.sweep(3), vec![]);
        pairings.open("g", "host", 3).unwrap();
        assert_eq!(pairings.sweep(3), vec![device()]);
    }

    #[test]
    fn a_collected_device_is_not_handed_back() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        pairings.join(&slot, 1).unwrap();
        pairings.admit(&slot, device(), 2).unwrap();
        pairings.collect(&slot, 3).unwrap();
        pairings.open("g", "host", 4).unwrap();
        assert_eq!(pairings.sweep(PAIRING_MS * 2), vec![]);
    }

    #[test]
    fn caps_message_size_and_count() {
        let pairings = Pairings::default();
        let (slot, _) = pairings.open("g", "host", 0).unwrap();
        pairings.join(&slot, 0).unwrap();
        assert!(matches!(
            pairings.send(&slot, Side::Joiner, vec![0; MAX_MESSAGE_BYTES + 1], 1),
            Err(ApiError::TooLarge(_))
        ));
        for _ in 0..MAX_MESSAGES {
            pairings.send(&slot, Side::Joiner, b"m".to_vec(), 1).unwrap();
        }
        assert!(matches!(
            pairings.send(&slot, Side::Joiner, b"m".to_vec(), 1),
            Err(ApiError::Conflict(_))
        ));
    }
}
