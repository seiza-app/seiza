// Sync between a person's computers (PLAN §8, Sync): the group's encryption, adding a device by
// pairing code, the client for the sync server, Settings › Sync, and sending and fetching records.

pub mod client;
pub mod crypto;
pub mod exchange;
pub mod pairing;
pub mod settings;
pub mod setup;
