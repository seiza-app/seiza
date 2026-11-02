// Sync between a person's computers (PLAN §8, Sync): the group's encryption, adding a device by
// pairing code, and the client for the sync server. Settings › Sync (build step 4) is the first
// to use these; until then nothing here runs.

pub mod client;
pub mod crypto;
pub mod pairing;
