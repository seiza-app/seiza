// Sync between a person's computers (PLAN §8, Sync): the group's encryption, adding a device by
// pairing code, the client for the sync server, and Settings › Sync. Sending and fetching
// records comes next (build step 5).

pub mod client;
pub mod crypto;
pub mod pairing;
pub mod settings;
pub mod setup;
