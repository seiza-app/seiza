// Adding a device to a sync group with a pairing code (PLAN §8, Sync). The device already in the
// group (the host) opens a pairing on the server and shows a code: the server's number for the
// pairing, then four words. Typed on the new device (the joiner), the words are the password
// for SPAKE2, so the two agree on a one-off key that the server, which relays their messages,
// can't learn, even though the code is short. Three messages:
//
//   1. hello, host → joiner: the group's id and the host's half of SPAKE2.
//   2. reply, joiner → host: the joiner's half, proof that it has the same key, and its name,
//      sealed with that key.
//   3. welcome, host → joiner, once its person says Add: the host's proof, and the group's key,
//      sealed with that key.
//
// A wrong code fails one of the proofs, and that side then ends the pairing on the server, so
// each code gets one guess. The group's id, the pairing's number and a purpose go into the keys,
// so a pairing can't be replayed or moved to another group.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::OsRng;
use chacha20poly1305::aead::rand_core::RngCore;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::crypto::{self, GroupKey};

/// The 2048 words of BIP-39's English list (MIT licensed): common, at least three letters, and
/// no two share their first four. Four of them give 2048⁴, about 1.8 × 10¹³, codes.
const WORDS: &str = include_str!("words.txt");
const WORD_COUNT: usize = 4;

const PURPOSE: &[u8] = b"seiza pairing v1";
const HOST: &[u8] = b"seiza pairing v1 host";
const JOINER: &[u8] = b"seiza pairing v1 new device";

/// The most of a device's name that's sent, in characters.
pub const MAX_DEVICE_NAME: usize = 100;

/// A pairing code, e.g. `4821-orbit-lamp-quote-river`.
#[derive(Clone, PartialEq, Eq)]
pub struct Code {
    slot: String,
    words: [&'static str; WORD_COUNT],
}

impl Code {
    /// A new code for the pairing the server opened as `slot`.
    pub fn new(slot: &str) -> Self {
        let words = std::array::from_fn(|_| {
            // 2048 is a power of two, so taking the low 11 bits favors no word.
            let index = OsRng.next_u32() as usize % 2048;
            WORDS.lines().nth(index).expect("the list has 2048 words")
        });
        Self {
            slot: slot.to_owned(),
            words,
        }
    }

    /// Reads a code as someone typed it: any case, with dashes, spaces or both between the parts.
    pub fn parse(text: &str) -> Result<Self, BadCode> {
        let lower = text.to_lowercase();
        let mut parts = lower
            .split(|c: char| c == '-' || c.is_whitespace())
            .filter(|part| !part.is_empty());
        let slot = parts.next().ok_or(BadCode::Shape)?;
        if slot.len() != 4 || !slot.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BadCode::Shape);
        }
        let typed: Vec<&str> = parts.collect();
        if typed.len() != WORD_COUNT {
            return Err(BadCode::Shape);
        }
        let mut words = [""; WORD_COUNT];
        for (word, typed) in words.iter_mut().zip(typed) {
            *word = WORDS
                .lines()
                .find(|known| *known == typed)
                .ok_or_else(|| BadCode::UnknownWord(typed.to_owned()))?;
        }
        Ok(Self {
            slot: slot.to_owned(),
            words,
        })
    }

    /// The server's number for the pairing. Unlike the words, it isn't secret.
    pub fn slot(&self) -> &str {
        &self.slot
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.slot, self.words.join("-"))
    }
}

// Only the number is printed, so the words can't end up in a log.
impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Code({}-..)", self.slot)
    }
}

/// Why a typed code can't be a pairing code.
#[derive(Debug, PartialEq, Eq)]
pub enum BadCode {
    Shape,
    UnknownWord(String),
}

impl fmt::Display for BadCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadCode::Shape => {
                f.write_str("a pairing code is a number and four words, like 4821-orbit-lamp-quote-river")
            }
            BadCode::UnknownWord(word) => write!(f, "\"{word}\" isn't one of the pairing words; check its spelling"),
        }
    }
}

/// Why a pairing stopped. Either way, the side that finds out ends the pairing on the server.
#[derive(Debug, PartialEq, Eq)]
pub enum PairingError {
    /// The other device's proof didn't match: the code was typed wrong, or someone other than
    /// the two devices is in the middle.
    WrongCode,
    /// A message wasn't one of these, e.g. from a different version of the app.
    Garbled,
}

impl fmt::Display for PairingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PairingError::WrongCode => f.write_str("the pairing code didn't match; make a new one and try again"),
            PairingError::Garbled => f.write_str("the other device sent something this version of Seiza can't read"),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hello {
    v: u32,
    group_id: String,
    spake: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply {
    spake: String,
    proof: String,
    name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Welcome {
    proof: String,
    key: String,
}

/// The keys both sides derive from SPAKE2's result.
struct Keys {
    host_proof: [u8; 32],
    joiner_proof: [u8; 32],
    channel: [u8; 32],
}

impl Keys {
    fn derive(shared: &[u8], group_id: &str, slot: &str) -> Self {
        let hkdf = Hkdf::<Sha256>::new(None, shared);
        let key = |name: &[u8]| {
            let info = [PURPOSE, b"\0", name, b"\0", group_id.as_bytes(), b"\0", slot.as_bytes()].concat();
            let mut key = [0; 32];
            hkdf.expand(&info, &mut key)
                .expect("32 bytes is a valid length for SHA-256");
            key
        };
        Self {
            host_proof: key(b"host proof"),
            joiner_proof: key(b"joiner proof"),
            channel: key(b"channel"),
        }
    }
}

/// A proof that a side has the key: a MAC over both halves of SPAKE2.
fn proof(key: &[u8; 32], host_half: &[u8], joiner_half: &[u8]) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(&(host_half.len() as u32).to_be_bytes());
    mac.update(host_half);
    mac.update(joiner_half);
    mac
}

fn password(code: &Code) -> Password {
    Password::new(code.to_string())
}

fn decode(text: &str) -> Result<Vec<u8>, PairingError> {
    STANDARD.decode(text).map_err(|_| PairingError::Garbled)
}

fn read<'a, T: Deserialize<'a>>(message: &'a [u8]) -> Result<T, PairingError> {
    serde_json::from_slice(message).map_err(|_| PairingError::Garbled)
}

fn write(message: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(message).expect("pairing messages always serialize")
}

/// The device already in the group, from opening the pairing until the new device's reply.
pub struct Host {
    spake: Spake2<Ed25519Group>,
    half: Vec<u8>,
    group_id: String,
    slot: String,
}

impl Host {
    /// Starts the host's side for `code` in group `group_id`, returning the hello to send.
    pub fn start(code: &Code, group_id: &str) -> (Self, Vec<u8>) {
        let (spake, half) =
            Spake2::<Ed25519Group>::start_a(&password(code), &Identity::new(HOST), &Identity::new(JOINER));
        Self::started(spake, half, code, group_id)
    }

    fn started(spake: Spake2<Ed25519Group>, half: Vec<u8>, code: &Code, group_id: &str) -> (Self, Vec<u8>) {
        let hello = write(&Hello {
            v: 1,
            group_id: group_id.to_owned(),
            spake: STANDARD.encode(&half),
        });
        (
            Self {
                spake,
                half,
                group_id: group_id.to_owned(),
                slot: code.slot.clone(),
            },
            hello,
        )
    }

    /// Checks the new device's reply. If its proof holds, the code matched, and its name is
    /// what to ask about: "Add *MacBook Air*?"
    pub fn check(self, reply: &[u8]) -> Result<Checked, PairingError> {
        let reply: Reply = read(reply)?;
        let joiner_half = decode(&reply.spake)?;
        let shared = self.spake.finish(&joiner_half).map_err(|_| PairingError::Garbled)?;
        let keys = Keys::derive(&shared, &self.group_id, &self.slot);
        proof(&keys.joiner_proof, &self.half, &joiner_half)
            .verify_slice(&decode(&reply.proof)?)
            .map_err(|_| PairingError::WrongCode)?;
        let name = crypto::open(&keys.channel, b"seiza pairing v1 name", &decode(&reply.name)?)
            .ok()
            .and_then(|name| String::from_utf8(name).ok())
            .ok_or(PairingError::Garbled)?;
        Ok(Checked {
            device_name: name,
            keys,
            host_half: self.half,
            joiner_half,
        })
    }
}

/// The host once the new device has proven it typed the right code.
pub struct Checked {
    pub device_name: String,
    keys: Keys,
    host_half: Vec<u8>,
    joiner_half: Vec<u8>,
}

impl Checked {
    /// The welcome to send once its person says Add, carrying the group's key.
    pub fn welcome(&self, group_key: &GroupKey) -> Vec<u8> {
        let proof = proof(&self.keys.host_proof, &self.host_half, &self.joiner_half)
            .finalize()
            .into_bytes();
        let key = crypto::seal(&self.keys.channel, b"seiza pairing v1 key", group_key.as_bytes());
        write(&Welcome {
            proof: STANDARD.encode(proof),
            key: STANDARD.encode(key),
        })
    }
}

/// The new device, from sending its reply until the welcome.
pub struct Joiner {
    keys: Keys,
    group_id: String,
    host_half: Vec<u8>,
    joiner_half: Vec<u8>,
}

impl Joiner {
    /// Answers the host's hello with this device's half, its proof and its name.
    pub fn reply(code: &Code, hello: &[u8], device_name: &str) -> Result<(Self, Vec<u8>), PairingError> {
        let (spake, half) =
            Spake2::<Ed25519Group>::start_b(&password(code), &Identity::new(HOST), &Identity::new(JOINER));
        Self::replied(spake, half, code, hello, device_name)
    }

    fn replied(
        spake: Spake2<Ed25519Group>,
        joiner_half: Vec<u8>,
        code: &Code,
        hello: &[u8],
        device_name: &str,
    ) -> Result<(Self, Vec<u8>), PairingError> {
        let hello: Hello = read(hello)?;
        if hello.v != 1 {
            return Err(PairingError::Garbled);
        }
        let host_half = decode(&hello.spake)?;
        let shared = spake.finish(&host_half).map_err(|_| PairingError::Garbled)?;
        let keys = Keys::derive(&shared, &hello.group_id, &code.slot);
        let proof = proof(&keys.joiner_proof, &host_half, &joiner_half)
            .finalize()
            .into_bytes();
        let name: String = device_name.chars().take(MAX_DEVICE_NAME).collect();
        let name = crypto::seal(&keys.channel, b"seiza pairing v1 name", name.as_bytes());
        let reply = write(&Reply {
            spake: STANDARD.encode(&joiner_half),
            proof: STANDARD.encode(proof),
            name: STANDARD.encode(name),
        });
        Ok((
            Self {
                keys,
                group_id: hello.group_id,
                host_half,
                joiner_half,
            },
            reply,
        ))
    }

    /// The group this device is joining, as the host said. Once the host's proof holds in
    /// `finish`, this is known to be the host's own; the server's answer should match it.
    pub fn group_id(&self) -> &str {
        &self.group_id
    }

    /// Checks the host's proof and takes the group's key from its welcome.
    pub fn finish(self, welcome: &[u8]) -> Result<GroupKey, PairingError> {
        let welcome: Welcome = read(welcome)?;
        proof(&self.keys.host_proof, &self.host_half, &self.joiner_half)
            .verify_slice(&decode(&welcome.proof)?)
            .map_err(|_| PairingError::WrongCode)?;
        crypto::open(&self.keys.channel, b"seiza pairing v1 key", &decode(&welcome.key)?)
            .ok()
            .and_then(|key| GroupKey::from_bytes(&key))
            .ok_or(PairingError::Garbled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP: &str = "group-one";

    fn pair(host_code: &Code, joiner_code: &Code) -> Result<(String, GroupKey, GroupKey), PairingError> {
        let group_key = GroupKey::generate();
        let (host, hello) = Host::start(host_code, GROUP);
        let (joiner, reply) = Joiner::reply(joiner_code, &hello, "MacBook Air")?;
        assert_eq!(joiner.group_id(), GROUP);
        let checked = host.check(&reply)?;
        let received = joiner.finish(&checked.welcome(&group_key))?;
        Ok((checked.device_name, group_key, received))
    }

    #[test]
    fn the_right_code_hands_over_the_group_key() {
        let code = Code::new("4821");
        let (name, sent, received) = pair(&code, &code).unwrap();
        assert_eq!(name, "MacBook Air");
        assert_eq!(sent, received);
    }

    #[test]
    fn a_wrong_word_is_caught_by_the_host_before_anything_is_sent() {
        let code = Code::new("4821");
        let mut typo = code.clone();
        typo.words[3] = if code.words[3] == "zoo" { "zone" } else { "zoo" };
        assert_eq!(pair(&code, &typo).unwrap_err(), PairingError::WrongCode);
    }

    #[test]
    fn a_host_pretending_with_a_guess_is_caught_by_the_new_device() {
        let code = Code::new("4821");
        let (_, hello) = Host::start(&code, GROUP);
        let (joiner, reply) = Joiner::reply(&code, &hello, "MacBook Air").unwrap();
        // Someone in the middle answers with a guess of their own instead of the real host.
        let guess = Code::new("4821");
        let (impostor, _) = Host::start(&guess, GROUP);
        let welcome = match impostor.check(&reply) {
            Err(error) => {
                assert_eq!(error, PairingError::WrongCode);
                // And making up a welcome without the key doesn't work either.
                write(&Welcome {
                    proof: STANDARD.encode([0; 32]),
                    key: STANDARD.encode([0; 64]),
                })
            }
            Ok(checked) => checked.welcome(&GroupKey::generate()),
        };
        assert_eq!(joiner.finish(&welcome).unwrap_err(), PairingError::WrongCode);
    }

    #[test]
    fn a_pairing_moved_to_another_group_or_number_fails() {
        let code = Code::new("4821");
        // The server swaps the group's id in the hello.
        let (host, hello) = Host::start(&code, GROUP);
        let mut changed: Hello = serde_json::from_slice(&hello).unwrap();
        changed.group_id = "group-two".into();
        let (_, reply) = Joiner::reply(&code, &write(&changed), "MacBook Air").unwrap();
        assert_eq!(host.check(&reply).err(), Some(PairingError::WrongCode));

        // The same words under another pairing's number.
        let other = Code {
            slot: "1111".into(),
            words: code.words,
        };
        assert_eq!(pair(&code, &other).unwrap_err(), PairingError::WrongCode);
    }

    #[test]
    fn messages_from_elsewhere_are_garbled_not_wrong() {
        let code = Code::new("4821");
        assert_eq!(
            Joiner::reply(&code, b"not json", "Mac").err(),
            Some(PairingError::Garbled)
        );
        let (host, hello) = Host::start(&code, GROUP);
        // The host's own hello reflected back as a reply.
        let reflected: Hello = serde_json::from_slice(&hello).unwrap();
        let reflected = write(&Reply {
            spake: reflected.spake,
            proof: String::new(),
            name: String::new(),
        });
        assert_eq!(host.check(&reflected).err(), Some(PairingError::Garbled));
    }

    #[test]
    fn long_names_are_cut_short() {
        let code = Code::new("4821");
        let (host, hello) = Host::start(&code, GROUP);
        let (_, reply) = Joiner::reply(&code, &hello, &"a".repeat(500)).unwrap();
        assert_eq!(host.check(&reply).unwrap().device_name.len(), MAX_DEVICE_NAME);
    }

    #[test]
    fn codes_read_as_typed_and_print_back() {
        let code = Code::parse("4821-orbit-lamp-quote-river").unwrap();
        assert_eq!(code.slot(), "4821");
        assert_eq!(code.to_string(), "4821-orbit-lamp-quote-river");
        assert_eq!(Code::parse("  4821 Orbit LAMP-quote  river ").unwrap(), code);
        assert_eq!(format!("{code:?}"), "Code(4821-..)");

        assert_eq!(Code::parse("orbit-lamp-quote-river"), Err(BadCode::Shape));
        assert_eq!(Code::parse("48210-orbit-lamp-quote-river"), Err(BadCode::Shape));
        assert_eq!(Code::parse("4821-orbit-lamp-quote"), Err(BadCode::Shape));
        assert_eq!(Code::parse("4821-orbit-lamp-quote-river-zoo"), Err(BadCode::Shape));
        assert_eq!(
            Code::parse("4821-orbit-lantern-quote-river"),
            Err(BadCode::UnknownWord("lantern".into()))
        );
    }

    #[test]
    fn new_codes_use_the_list_and_differ() {
        assert_eq!(WORDS.lines().count(), 2048);
        let code = Code::new("0042");
        assert_eq!(Code::parse(&code.to_string()).unwrap(), code);
        assert_ne!(Code::new("0042"), Code::new("0042"));
    }

    /// Always the same bytes, so SPAKE2's messages and key come out the same every run.
    struct Fixed(u8);

    impl RngCore for Fixed {
        fn next_u32(&mut self) -> u32 {
            u32::from_le_bytes([self.0; 4])
        }
        fn next_u64(&mut self) -> u64 {
            u64::from_le_bytes([self.0; 8])
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(self.0);
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            dest.fill(self.0);
            Ok(())
        }
    }

    impl rand_core::CryptoRng for Fixed {}

    /// SPAKE2 and the keys from it, against values recorded from this version of the crate, so
    /// an update that changed either (and so couldn't pair with older copies of the app) fails
    /// here first.
    #[test]
    fn matches_the_recorded_vectors() {
        let code = Code::parse("4821-orbit-lamp-quote-river").unwrap();
        let ids = (Identity::new(HOST), Identity::new(JOINER));
        let (host_spake, host_half) =
            Spake2::<Ed25519Group>::start_a_with_rng(&password(&code), &ids.0, &ids.1, Fixed(1));
        let (joiner_spake, joiner_half) =
            Spake2::<Ed25519Group>::start_b_with_rng(&password(&code), &ids.0, &ids.1, Fixed(2));
        let shared = host_spake.finish(&joiner_half).unwrap();
        assert_eq!(joiner_spake.finish(&host_half).unwrap(), shared);
        let keys = Keys::derive(&shared, GROUP, code.slot());
        let proof = proof(&keys.host_proof, &host_half, &joiner_half)
            .finalize()
            .into_bytes();

        assert_eq!(
            STANDARD.encode(&host_half),
            "QUGTUZ3syIDHGgsJ2lKCPWzucNj9vfZ7rm+Pm0kef3Hx"
        );
        assert_eq!(
            STANDARD.encode(&joiner_half),
            "Qo1aw8jhpScB3AJXrPq/F2+8QKHoG9Pst7wwbrY6CsYr"
        );
        assert_eq!(STANDARD.encode(&shared), "q1bxbgQLg+CMuAzMZgyST8Y0K22P/D2itAeG/xqgCfM=");
        assert_eq!(
            STANDARD.encode(keys.channel),
            "5kz0r1aLbBQ2Me9/wUuMmpTUKPscKLVUN0uSB8ABD+M="
        );
        assert_eq!(STANDARD.encode(proof), "NpsXZBEz84dMDgPr3dSN2BGIfunlj6NjwBwRNEPAzfQ=");
    }

    /// The whole exchange through the deterministic halves too, so `started` and `replied` stay
    /// in step with `start` and `reply`.
    #[test]
    fn the_exchange_works_from_recorded_halves() {
        let code = Code::parse("4821-orbit-lamp-quote-river").unwrap();
        let ids = (Identity::new(HOST), Identity::new(JOINER));
        let (spake, half) = Spake2::<Ed25519Group>::start_a_with_rng(&password(&code), &ids.0, &ids.1, Fixed(1));
        let (host, hello) = Host::started(spake, half, &code, GROUP);
        let (spake, half) = Spake2::<Ed25519Group>::start_b_with_rng(&password(&code), &ids.0, &ids.1, Fixed(2));
        let (joiner, reply) = Joiner::replied(spake, half, &code, &hello, "Mac").unwrap();
        let key = GroupKey::generate();
        assert_eq!(joiner.finish(&host.check(&reply).unwrap().welcome(&key)).unwrap(), key);
    }
}
