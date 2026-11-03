// The sync group's encryption (PLAN §8, Sync). Everything the server keeps for a group, its
// batches of records and its devices' names, is sealed here first with the group's key, which
// the server never sees: XChaCha20-Poly1305, with a fresh random nonce each time, so two devices
// can never reuse one.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

/// The first byte of everything sealed, so the format can change later.
const VERSION: u8 = 1;
const NONCE_BYTES: usize = 24;
/// Poly1305's tag.
const TAG_BYTES: usize = 16;

/// What a sealed value is for. It's bound into the encryption with the group's id, so a value
/// can't be passed off as another kind, or moved to another group, without failing to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// A batch of records.
    Batch,
    /// A device's name, as the server's device list shows it. Sealed before the group has an
    /// id (the first device names itself as it starts the group), so it's bound to no group.
    Label,
    /// A message between the group's connected devices (sync/live.rs).
    Live,
}

impl Purpose {
    fn name(self) -> &'static [u8] {
        match self {
            Purpose::Batch => b"seiza batch v1",
            Purpose::Label => b"seiza label v1",
            Purpose::Live => b"seiza live v1",
        }
    }
}

/// A sealed value didn't open: it was changed, sealed with another key, for another purpose or
/// group, or isn't in this format.
#[derive(Debug, PartialEq, Eq)]
pub struct Unreadable;

/// The group's key: 32 random bytes, made by the device that starts the group and handed to
/// each new device inside the pairing (pairing.rs).
#[derive(Clone, PartialEq, Eq)]
pub struct GroupKey([u8; 32]);

impl GroupKey {
    pub fn generate() -> Self {
        Self(XChaCha20Poly1305::generate_key(&mut OsRng).into())
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self(bytes.try_into().ok()?))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Seals `plaintext` for `purpose` in group `group_id`: the version, the nonce, then the
    /// ciphertext with its tag.
    pub fn seal(&self, purpose: Purpose, group_id: &str, plaintext: &[u8]) -> Vec<u8> {
        seal(&self.0, &associated_data(purpose, group_id), plaintext)
    }

    pub fn open(&self, purpose: Purpose, group_id: &str, sealed: &[u8]) -> Result<Vec<u8>, Unreadable> {
        open(&self.0, &associated_data(purpose, group_id), sealed)
    }

    /// A device's name sealed for the server, in base64 as its API takes it.
    pub fn seal_label(&self, name: &str) -> String {
        STANDARD.encode(self.seal(Purpose::Label, "", name.as_bytes()))
    }

    /// A device's name from the server's list.
    pub fn open_label(&self, label: &str) -> Result<String, Unreadable> {
        let sealed = STANDARD.decode(label).map_err(|_| Unreadable)?;
        String::from_utf8(self.open(Purpose::Label, "", &sealed)?).map_err(|_| Unreadable)
    }
}

// Never printed, so it can't end up in a log.
impl std::fmt::Debug for GroupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GroupKey(..)")
    }
}

fn associated_data(purpose: Purpose, group_id: &str) -> Vec<u8> {
    [purpose.name(), b"\0", group_id.as_bytes()].concat()
}

/// Seals with any 32-byte key; the pairing uses this too, with its one-off key.
pub(super) fn seal(key: &[u8; 32], associated_data: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .expect("XChaCha20-Poly1305 only fails for messages far beyond any batch");
    [&[VERSION][..], nonce.as_slice(), &ciphertext].concat()
}

pub(super) fn open(key: &[u8; 32], associated_data: &[u8], sealed: &[u8]) -> Result<Vec<u8>, Unreadable> {
    if sealed.len() < 1 + NONCE_BYTES + TAG_BYTES || sealed[0] != VERSION {
        return Err(Unreadable);
    }
    let (nonce, ciphertext) = sealed[1..].split_at(NONCE_BYTES);
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|_| Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP: &str = "group-one";

    #[test]
    fn sealed_values_open_again() {
        let key = GroupKey::generate();
        let sealed = key.seal(Purpose::Batch, GROUP, b"three sessions");
        assert_eq!(sealed.len(), 1 + NONCE_BYTES + b"three sessions".len() + TAG_BYTES);
        assert_eq!(key.open(Purpose::Batch, GROUP, &sealed).unwrap(), b"three sessions");

        let label = key.seal_label("MacBook Air");
        assert_eq!(key.open_label(&label).unwrap(), "MacBook Air");
    }

    #[test]
    fn the_same_value_seals_differently_each_time() {
        let key = GroupKey::generate();
        assert_ne!(
            key.seal(Purpose::Batch, GROUP, b"same"),
            key.seal(Purpose::Batch, GROUP, b"same")
        );
    }

    #[test]
    fn another_key_purpose_or_group_can_not_open_it() {
        let key = GroupKey::generate();
        let sealed = key.seal(Purpose::Batch, GROUP, b"records");
        assert_eq!(
            GroupKey::generate().open(Purpose::Batch, GROUP, &sealed),
            Err(Unreadable)
        );
        assert_eq!(key.open(Purpose::Label, GROUP, &sealed), Err(Unreadable));
        assert_eq!(key.open(Purpose::Batch, "group-two", &sealed), Err(Unreadable));
    }

    #[test]
    fn changed_short_or_unknown_versions_do_not_open() {
        let key = GroupKey::generate();
        let sealed = key.seal(Purpose::Batch, GROUP, b"records");
        for at in [1, NONCE_BYTES + 1, sealed.len() - 1] {
            let mut changed = sealed.clone();
            changed[at] ^= 1;
            assert_eq!(key.open(Purpose::Batch, GROUP, &changed), Err(Unreadable));
        }
        let mut version_two = sealed.clone();
        version_two[0] = 2;
        assert_eq!(key.open(Purpose::Batch, GROUP, &version_two), Err(Unreadable));
        assert_eq!(key.open(Purpose::Batch, GROUP, &sealed[..20]), Err(Unreadable));
        assert_eq!(key.open_label("not base64!"), Err(Unreadable));
    }

    #[test]
    fn keys_round_trip_as_bytes_and_never_print() {
        let key = GroupKey::generate();
        assert_eq!(GroupKey::from_bytes(key.as_bytes()), Some(key.clone()));
        assert_eq!(GroupKey::from_bytes(&[0; 31]), None);
        assert_eq!(format!("{key:?}"), "GroupKey(..)");
    }

    /// XChaCha20-Poly1305 itself, against draft-irtf-cfrg-xchacha's test vector (A.3.1), so a
    /// change to the crate can't quietly change the cipher.
    #[test]
    fn matches_the_xchacha20_poly1305_test_vector() {
        let key: [u8; 32] = std::array::from_fn(|i| 0x80 + i as u8);
        let nonce: [u8; 24] = std::array::from_fn(|i| 0x40 + i as u8);
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let expected = hex(concat!(
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb",
            "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452",
            "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9",
            "21f9664c97637da9768812f615c68b13b52e",
            "c0875924c1c7987947deafd8780acf49",
        ));
        let cipher = XChaCha20Poly1305::new((&key).into());
        let sealed = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .unwrap();
        assert_eq!(sealed, expected);
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }
}
