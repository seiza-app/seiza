use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::config::NewGroups;
use crate::tokens;

/// Each entry upgrades the database by one version. Add new ones to the end; never edit old ones.
const MIGRATIONS: &[&str] = &[r#"
    CREATE TABLE groups (
        id TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL,
        -- The seq the group's next batch gets, so seqs never repeat even as batches go.
        next_seq INTEGER NOT NULL DEFAULT 1,
        -- What the group's batches add up to, for its size cap.
        bytes INTEGER NOT NULL DEFAULT 0
    );

    -- A device's label is whatever its app sends, encrypted with the group's key.
    CREATE TABLE devices (
        id TEXT PRIMARY KEY,
        group_id TEXT NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
        token_hash BLOB NOT NULL UNIQUE,
        label BLOB NOT NULL,
        created_at INTEGER NOT NULL,
        last_seen_at INTEGER NOT NULL
    );
    CREATE INDEX devices_group ON devices (group_id);

    CREATE TABLE batches (
        group_id TEXT NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
        seq INTEGER NOT NULL,
        device_id TEXT NOT NULL,
        data BLOB NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (group_id, seq)
    ) WITHOUT ROWID;

    CREATE TABLE invites (
        code_hash BLOB PRIMARY KEY,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    );
"#];

/// How long an invite code from `seiza-sync invite` lasts.
const INVITE_MS: i64 = 7 * DAY_MS;
const DAY_MS: i64 = 24 * 60 * 60_000;
/// A device's last-seen time is only written when it's this much older, so most requests read
/// the database without writing to it.
const SEEN_EVERY_MS: i64 = 60_000;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    migrated(Connection::open(path)?)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    migrated(Connection::open_in_memory()?)
}

fn migrated(mut db: Connection) -> rusqlite::Result<Connection> {
    // WAL lets reads go on while a batch is written. Foreign keys make removing a group remove
    // its devices and batches.
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "foreign_keys", true)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let tx = db.transaction()?;
    for (index, migration) in (0_i64..).zip(MIGRATIONS).skip(version as usize) {
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", index + 1)?;
    }
    tx.commit()?;
    Ok(db)
}

/// A device that was just added, with its token. The token is only ever given out here.
#[derive(Debug, PartialEq, Eq)]
pub struct NewDevice {
    pub group_id: String,
    pub device_id: String,
    pub token: String,
}

#[derive(Debug)]
pub enum CreateGroup {
    Created(NewDevice),
    /// The server doesn't take new groups now.
    Closed,
    /// Invites are needed, and this one is missing, wrong, used or expired.
    BadInvite,
}

/// Starts a group with its first device, if the server's setting allows it.
pub fn create_group(
    db: &mut Connection,
    label: &[u8],
    new_groups: NewGroups,
    invite: Option<&str>,
    now: i64,
) -> rusqlite::Result<CreateGroup> {
    // Immediate, so two first devices racing can't both see an empty server.
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    match new_groups {
        NewGroups::FirstOnly => {
            let groups: i64 = tx.query_row("SELECT COUNT(*) FROM groups", [], |row| row.get(0))?;
            if groups > 0 {
                return Ok(CreateGroup::Closed);
            }
        }
        NewGroups::Invites => {
            let Some(invite) = invite else {
                return Ok(CreateGroup::BadInvite);
            };
            let used = tx.execute(
                "DELETE FROM invites WHERE code_hash = ?1 AND expires_at > ?2",
                params![tokens::hash(invite.trim()), now],
            )?;
            if used == 0 {
                return Ok(CreateGroup::BadInvite);
            }
        }
        NewGroups::Open => {}
    }
    let group_id = tokens::random(16);
    tx.execute(
        "INSERT INTO groups (id, created_at) VALUES (?1, ?2)",
        params![group_id, now],
    )?;
    let device = add_device(&tx, &group_id, label, now)?;
    tx.commit()?;
    Ok(CreateGroup::Created(device))
}

/// Adds a device to a group, for a new group or once pairing lets one in.
pub fn add_device(db: &Connection, group_id: &str, label: &[u8], now: i64) -> rusqlite::Result<NewDevice> {
    let device_id = tokens::random(16);
    let token = tokens::token();
    db.execute(
        "INSERT INTO devices (id, group_id, token_hash, label, created_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![device_id, group_id, tokens::hash(&token), label, now],
    )?;
    Ok(NewDevice {
        group_id: group_id.to_string(),
        device_id,
        token,
    })
}

/// Whether a group still exists, e.g. for a pairing that waited while its last device left.
pub fn group_exists(db: &Connection, group_id: &str) -> rusqlite::Result<bool> {
    db.query_row("SELECT 1 FROM groups WHERE id = ?1", params![group_id], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

/// The device a request comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRef {
    pub id: String,
    pub group_id: String,
}

/// The device with this token, noting that it was seen now.
pub fn device_for_token(db: &Connection, token: &str, now: i64) -> rusqlite::Result<Option<DeviceRef>> {
    let found = db
        .query_row(
            "SELECT id, group_id, last_seen_at FROM devices WHERE token_hash = ?1",
            params![tokens::hash(token)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((id, group_id, last_seen_at)) = found else {
        return Ok(None);
    };
    if now - last_seen_at >= SEEN_EVERY_MS {
        db.execute("UPDATE devices SET last_seen_at = ?2 WHERE id = ?1", params![id, now])?;
    }
    Ok(Some(DeviceRef { id, group_id }))
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeviceRow {
    pub id: String,
    pub label: Vec<u8>,
    pub created_at: i64,
    pub last_seen_at: i64,
}

pub fn devices(db: &Connection, group_id: &str) -> rusqlite::Result<Vec<DeviceRow>> {
    let mut statement = db.prepare(
        "SELECT id, label, created_at, last_seen_at FROM devices WHERE group_id = ?1 ORDER BY created_at, id",
    )?;
    statement
        .query_map(params![group_id], |row| {
            Ok(DeviceRow {
                id: row.get(0)?,
                label: row.get(1)?,
                created_at: row.get(2)?,
                last_seen_at: row.get(3)?,
            })
        })?
        .collect()
}

/// Removes a device from its group, whichever device in the group asks (Remove, or Leave sync
/// for itself). A group with no devices left is deleted, with everything it stored. False if
/// the group has no such device.
pub fn remove_device(db: &mut Connection, group_id: &str, device_id: &str) -> rusqlite::Result<bool> {
    let tx = db.transaction()?;
    let removed = tx.execute(
        "DELETE FROM devices WHERE id = ?1 AND group_id = ?2",
        params![device_id, group_id],
    )?;
    let left: i64 = tx.query_row(
        "SELECT COUNT(*) FROM devices WHERE group_id = ?1",
        params![group_id],
        |row| row.get(0),
    )?;
    if left == 0 {
        tx.execute("DELETE FROM groups WHERE id = ?1", params![group_id])?;
    }
    tx.commit()?;
    Ok(removed > 0)
}

#[derive(Debug, PartialEq, Eq)]
pub enum AddBatch {
    Added {
        seq: i64,
    },
    /// The group has reached its size cap.
    Full,
}

/// Keeps a batch from a device, after the group's others.
pub fn add_batch(
    db: &mut Connection,
    device: &DeviceRef,
    data: &[u8],
    group_max_bytes: u64,
    now: i64,
) -> rusqlite::Result<AddBatch> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (seq, bytes): (i64, i64) = tx.query_row(
        "SELECT next_seq, bytes FROM groups WHERE id = ?1",
        params![device.group_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let size = data.len() as i64;
    if (bytes + size) as u64 > group_max_bytes {
        return Ok(AddBatch::Full);
    }
    tx.execute(
        "INSERT INTO batches (group_id, seq, device_id, data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![device.group_id, seq, device.id, data, now],
    )?;
    tx.execute(
        "UPDATE groups SET next_seq = next_seq + 1, bytes = bytes + ?2 WHERE id = ?1",
        params![device.group_id, size],
    )?;
    tx.commit()?;
    Ok(AddBatch::Added { seq })
}

#[derive(Debug, PartialEq, Eq)]
pub struct BatchRow {
    pub seq: i64,
    pub device_id: String,
    pub data: Vec<u8>,
    pub created_at: i64,
}

/// A group's batches after `after`, in order: at most `max_count`, and stopping once they pass
/// `max_bytes` (always at least one). True as well if there are more to fetch.
pub fn batches_after(
    db: &Connection,
    group_id: &str,
    after: i64,
    max_count: usize,
    max_bytes: usize,
) -> rusqlite::Result<(Vec<BatchRow>, bool)> {
    let mut statement = db.prepare(
        "SELECT seq, device_id, data, created_at FROM batches WHERE group_id = ?1 AND seq > ?2 ORDER BY seq",
    )?;
    let mut rows = statement.query(params![group_id, after])?;
    let mut batches = Vec::new();
    let mut bytes = 0;
    while let Some(row) = rows.next()? {
        if batches.len() == max_count || (bytes >= max_bytes && !batches.is_empty()) {
            return Ok((batches, true));
        }
        let batch = BatchRow {
            seq: row.get(0)?,
            device_id: row.get(1)?,
            data: row.get(2)?,
            created_at: row.get(3)?,
        };
        bytes += batch.data.len();
        batches.push(batch);
    }
    Ok((batches, false))
}

/// A one-time invite code for a new group. Only its hash is kept.
pub fn make_invite(db: &Connection, now: i64) -> rusqlite::Result<String> {
    db.execute("DELETE FROM invites WHERE expires_at <= ?1", params![now])?;
    // 12 characters: about 72 bits, so they can't be guessed even without the rate limits.
    let code = tokens::random(9);
    db.execute(
        "INSERT INTO invites (code_hash, created_at, expires_at) VALUES (?1, ?2, ?3)",
        params![tokens::hash(&code), now, now + INVITE_MS],
    )?;
    Ok(code)
}

/// Copies the whole database to a new file, consistent even while the server writes to it.
pub fn backup(db: &Connection, to: &Path) -> rusqlite::Result<()> {
    let to = to.to_str().ok_or(rusqlite::Error::InvalidPath(to.to_path_buf()))?;
    db.execute("VACUUM INTO ?1", params![to]).map(|_| ())
}

/// Counts for the server's owner (PLAN §8, Sync: counting users, with no telemetry). A group is
/// active if any of its devices was seen in that time.
#[derive(Debug, PartialEq, Eq)]
pub struct Stats {
    pub groups: i64,
    pub devices: i64,
    pub active_7_days: i64,
    pub active_30_days: i64,
    pub bytes: i64,
}

pub fn stats(db: &Connection, now: i64) -> rusqlite::Result<Stats> {
    let active_since = |days: i64| -> rusqlite::Result<i64> {
        db.query_row(
            "SELECT COUNT(DISTINCT group_id) FROM devices WHERE last_seen_at >= ?1",
            params![now - days * DAY_MS],
            |row| row.get(0),
        )
    };
    Ok(Stats {
        groups: db.query_row("SELECT COUNT(*) FROM groups", [], |row| row.get(0))?,
        devices: db.query_row("SELECT COUNT(*) FROM devices", [], |row| row.get(0))?,
        active_7_days: active_since(7)?,
        active_30_days: active_since(30)?,
        bytes: db.query_row("SELECT COALESCE(SUM(bytes), 0) FROM groups", [], |row| row.get(0))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn created(outcome: CreateGroup) -> NewDevice {
        match outcome {
            CreateGroup::Created(device) => device,
            other => panic!("expected a new group, got {other:?}"),
        }
    }

    fn device(new: &NewDevice) -> DeviceRef {
        DeviceRef {
            id: new.device_id.clone(),
            group_id: new.group_id.clone(),
        }
    }

    #[test]
    fn first_group_only_closes_after_the_first_and_opens_when_it_is_gone() {
        let mut db = open_in_memory().unwrap();
        let first = created(create_group(&mut db, b"laptop", NewGroups::FirstOnly, None, 1).unwrap());
        assert!(matches!(
            create_group(&mut db, b"other", NewGroups::FirstOnly, None, 2).unwrap(),
            CreateGroup::Closed
        ));
        assert!(remove_device(&mut db, &first.group_id, &first.device_id).unwrap());
        created(create_group(&mut db, b"again", NewGroups::FirstOnly, None, 3).unwrap());
    }

    #[test]
    fn invites_are_single_use_and_expire() {
        let mut db = open_in_memory().unwrap();
        assert!(matches!(
            create_group(&mut db, b"a", NewGroups::Invites, None, 1).unwrap(),
            CreateGroup::BadInvite
        ));
        let code = make_invite(&db, 1).unwrap();
        assert_eq!(code.len(), 12);
        created(create_group(&mut db, b"a", NewGroups::Invites, Some(&code), 2).unwrap());
        assert!(matches!(
            create_group(&mut db, b"b", NewGroups::Invites, Some(&code), 3).unwrap(),
            CreateGroup::BadInvite
        ));
        let late = make_invite(&db, 10).unwrap();
        assert!(matches!(
            create_group(&mut db, b"c", NewGroups::Invites, Some(&late), 10 + INVITE_MS).unwrap(),
            CreateGroup::BadInvite
        ));
    }

    #[test]
    fn tokens_find_their_device_and_only_hashes_are_kept() {
        let mut db = open_in_memory().unwrap();
        let new = created(create_group(&mut db, b"laptop", NewGroups::Open, None, 1).unwrap());
        assert_eq!(device_for_token(&db, &new.token, 2).unwrap(), Some(device(&new)));
        assert_eq!(device_for_token(&db, "not a token", 2).unwrap(), None);
        let stored: Vec<u8> = db
            .query_row("SELECT token_hash FROM devices", [], |row| row.get(0))
            .unwrap();
        assert_ne!(stored, new.token.as_bytes());
    }

    #[test]
    fn last_seen_is_written_at_most_once_a_minute() {
        let mut db = open_in_memory().unwrap();
        let new = created(create_group(&mut db, b"laptop", NewGroups::Open, None, 0).unwrap());
        device_for_token(&db, &new.token, 30_000).unwrap();
        assert_eq!(devices(&db, &new.group_id).unwrap()[0].last_seen_at, 0);
        device_for_token(&db, &new.token, 61_000).unwrap();
        assert_eq!(devices(&db, &new.group_id).unwrap()[0].last_seen_at, 61_000);
    }

    #[test]
    fn removing_the_last_device_deletes_the_group_and_its_batches() {
        let mut db = open_in_memory().unwrap();
        let new = created(create_group(&mut db, b"laptop", NewGroups::Open, None, 1).unwrap());
        let second = add_device(&db, &new.group_id, b"desktop", 2).unwrap();
        add_batch(&mut db, &device(&new), b"sealed", 1000, 3).unwrap();
        assert!(remove_device(&mut db, &new.group_id, &second.device_id).unwrap());
        assert!(group_exists(&db, &new.group_id).unwrap());
        assert!(remove_device(&mut db, &new.group_id, &new.device_id).unwrap());
        assert!(!group_exists(&db, &new.group_id).unwrap());
        let batches: i64 = db
            .query_row("SELECT COUNT(*) FROM batches", [], |row| row.get(0))
            .unwrap();
        assert_eq!(batches, 0);
    }

    #[test]
    fn a_device_can_only_remove_devices_in_its_own_group() {
        let mut db = open_in_memory().unwrap();
        let ours = created(create_group(&mut db, b"a", NewGroups::Open, None, 1).unwrap());
        let theirs = created(create_group(&mut db, b"b", NewGroups::Open, None, 1).unwrap());
        assert!(!remove_device(&mut db, &ours.group_id, &theirs.device_id).unwrap());
        assert_eq!(devices(&db, &theirs.group_id).unwrap().len(), 1);
    }

    #[test]
    fn batches_come_back_in_order_and_in_pages() {
        let mut db = open_in_memory().unwrap();
        let new = created(create_group(&mut db, b"laptop", NewGroups::Open, None, 1).unwrap());
        for (index, data) in [&b"one"[..], b"two", b"three"].into_iter().enumerate() {
            let added = add_batch(&mut db, &device(&new), data, 1000, index as i64).unwrap();
            assert_eq!(added, AddBatch::Added { seq: index as i64 + 1 });
        }
        let (all, more) = batches_after(&db, &new.group_id, 0, 10, 1000).unwrap();
        assert_eq!(all.iter().map(|batch| batch.seq).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(!more);
        assert_eq!(all[2].data, b"three");
        assert_eq!(all[0].device_id, new.device_id);

        let (page, more) = batches_after(&db, &new.group_id, 1, 1, 1000).unwrap();
        assert_eq!((page[0].seq, more), (2, true));
        // A page stops once it passes the byte limit, but always holds at least one batch.
        let (page, more) = batches_after(&db, &new.group_id, 0, 10, 2).unwrap();
        assert_eq!((page.len(), more), (1, true));
    }

    #[test]
    fn a_full_group_takes_no_more() {
        let mut db = open_in_memory().unwrap();
        let new = created(create_group(&mut db, b"laptop", NewGroups::Open, None, 1).unwrap());
        assert_eq!(
            add_batch(&mut db, &device(&new), &[0; 6], 10, 2).unwrap(),
            AddBatch::Added { seq: 1 }
        );
        assert_eq!(
            add_batch(&mut db, &device(&new), &[0; 5], 10, 3).unwrap(),
            AddBatch::Full
        );
        assert_eq!(
            add_batch(&mut db, &device(&new), &[0; 4], 10, 4).unwrap(),
            AddBatch::Added { seq: 2 }
        );
    }

    #[test]
    fn backs_up_to_a_new_file_that_opens_with_everything() {
        let dir = std::env::temp_dir().join(format!("seiza-sync-test-{}", tokens::random(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = open(&dir.join("live.db")).unwrap();
        created(create_group(&mut db, b"laptop", NewGroups::Open, None, 1).unwrap());
        backup(&db, &dir.join("copy.db")).unwrap();
        let copy = open(&dir.join("copy.db")).unwrap();
        assert_eq!(stats(&copy, 1).unwrap().devices, 1);
        // It never writes over a file that's already there.
        assert!(backup(&db, &dir.join("copy.db")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stats_count_groups_by_their_newest_device() {
        let mut db = open_in_memory().unwrap();
        let recent = created(create_group(&mut db, b"a", NewGroups::Open, None, 0).unwrap());
        add_device(&db, &recent.group_id, b"a2", 0).unwrap();
        let old = created(create_group(&mut db, b"b", NewGroups::Open, None, 0).unwrap());
        add_batch(&mut db, &device(&old), &[0; 3], 100, 0).unwrap();
        let now = 20 * DAY_MS;
        device_for_token(&db, &recent.token, now).unwrap();
        assert_eq!(
            stats(&db, now).unwrap(),
            Stats {
                groups: 2,
                devices: 3,
                active_7_days: 1,
                active_30_days: 2,
                bytes: 3,
            }
        );
    }
}
