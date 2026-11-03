// Sync between a person's computers (PLAN §8, Sync): the group's encryption, adding a device by
// pairing code, the client for the sync server, Settings › Sync, sending and fetching records,
// and the quiet connection that live sessions use.

pub mod client;
pub mod crypto;
pub mod exchange;
pub mod live;
pub mod pairing;
pub mod settings;
pub mod setup;
