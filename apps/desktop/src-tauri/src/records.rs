//! The records that sync between devices (PLAN §8 Sync): finished sessions, trips to redirected
//! sites, constellation names and profiles. This is only what goes in and out of the database; sync itself
//! wraps them up, encrypts them and sends them on.
//!
//! Every record carries `updated_at`, when it last changed (epoch ms), and the newest one wins.
//! Each change made here also takes the next number from a counter (`change_seq`), so sync can
//! ask for everything changed since the last number it sent, whatever the clocks say. Records
//! from other devices are saved with their own `updated_at` and number 0, so they aren't sent
//! back. Nothing is deleted: a cleared constellation name stays, with no name, and a deleted
//! profile stays, marked deleted. Sample data (samples.rs) never leaves this device.
//!
//! Profiles are also kept as one list in `state`, in the order the extension shows them, which
//! is what the app and the extension use. The `profiles` table here is sync's copy of each one.

use std::cmp::Ordering;

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::samples::SAMPLE_PREFIX;
use crate::session::{EndReason, GateVisit, Profile, SessionRecord};
use crate::store::insert_session;

/// One record, as it travels between devices: `{"kind": "session", "id": …, "updatedAt": …}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Record {
    Session(SyncedSession),
    Trip(SyncedTrip),
    ConstellationName(SyncedName),
    Profile(SyncedProfile),
}

/// A finished session from the log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedSession {
    #[serde(flatten)]
    pub session: SessionRecord,
    pub updated_at: i64,
}

/// A trip to a redirected site, with the session it happened in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedTrip {
    #[serde(flatten)]
    pub visit: GateVisit,
    pub session_id: Option<String>,
    pub updated_at: i64,
}

/// A constellation's name, by its first star's id. None once it's been cleared.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedName {
    pub id: String,
    pub name: Option<String>,
    pub updated_at: i64,
}

/// A profile, kept whole under `profile` so its own fields can never clash with these. A
/// deleted one keeps its last copy, for its stars' color.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedProfile {
    pub id: String,
    pub profile: Profile,
    #[serde(default)]
    pub deleted: bool,
    pub updated_at: i64,
}

impl Record {
    pub fn id(&self) -> &str {
        match self {
            Record::Session(s) => &s.session.id,
            Record::Trip(t) => &t.visit.id,
            Record::ConstellationName(n) => &n.id,
            Record::Profile(p) => &p.id,
        }
    }

    pub fn updated_at(&self) -> i64 {
        match self {
            Record::Session(s) => s.updated_at,
            Record::Trip(t) => t.updated_at,
            Record::ConstellationName(n) => n.updated_at,
            Record::Profile(p) => p.updated_at,
        }
    }

    /// Whether this copy should replace `here`, another copy of the same record. The newest
    /// wins, except that an answered "Did you do it?" beats an unanswered one whatever the time.
    /// A tie goes the same way on every device, by comparing the two as JSON, and a copy that's
    /// the same as `here` doesn't replace it.
    pub fn wins_over(&self, here: &Record) -> bool {
        if let (Record::Session(theirs), Record::Session(ours)) = (self, here) {
            match (theirs.session.goal_met.is_some(), ours.session.goal_met.is_some()) {
                (true, false) => return true,
                (false, true) => return false,
                _ => {}
            }
        }
        match self.updated_at().cmp(&here.updated_at()) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => json(self) > json(here),
        }
    }

    /// Whether this can be saved here: not sample data, and a profile whose id is its own.
    fn can_apply(&self) -> bool {
        let consistent = match self {
            Record::Profile(p) => p.profile.id == p.id,
            _ => true,
        };
        consistent && !self.id().starts_with(SAMPLE_PREFIX)
    }
}

fn json(record: &Record) -> String {
    // Serializing these plain structs can't fail.
    serde_json::to_string(record).unwrap_or_default()
}

/// Takes the next number from the counter of changes made here.
pub fn next_change(db: &Connection) -> Result<i64> {
    Ok(
        db.query_row("UPDATE change_counter SET last = last + 1 RETURNING last", [], |row| {
            row.get(0)
        })?,
    )
}

const SESSION_COLUMNS: &str = "id, profile_id, profile_name, started_at, ended_at, timer_minutes, focused_ms, \
     overtime_ms, idle_tail_ms, end_reason, broken, goal, goal_met, updated_at, change_seq";
const TRIP_COLUMNS: &str =
    "id, recipe_id, at, destination, why, intention, continued, session_id, updated_at, change_seq";
const NAME_COLUMNS: &str = "id, name, updated_at, change_seq";
const PROFILE_COLUMNS: &str = "id, profile, deleted, updated_at, change_seq";

fn session_row(row: &Row) -> rusqlite::Result<(Record, i64)> {
    let session = SessionRecord {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        profile_name: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
        timer_minutes: row.get(5)?,
        focused_ms: row.get(6)?,
        overtime_ms: row.get(7)?,
        idle_tail_ms: row.get(8)?,
        end_reason: EndReason::parse(&row.get::<_, String>(9)?).unwrap_or(EndReason::Stopped),
        broken: row.get(10)?,
        goal: row.get(11)?,
        goal_met: row.get(12)?,
    };
    let record = Record::Session(SyncedSession {
        session,
        updated_at: row.get(13)?,
    });
    Ok((record, row.get(14)?))
}

fn trip_row(row: &Row) -> rusqlite::Result<(Record, i64)> {
    let visit = GateVisit {
        id: row.get(0)?,
        recipe_id: row.get(1)?,
        at: row.get(2)?,
        destination: row.get(3)?,
        why: row.get(4)?,
        intention: row.get(5)?,
        continued: row.get(6)?,
    };
    let record = Record::Trip(SyncedTrip {
        visit,
        session_id: row.get(7)?,
        updated_at: row.get(8)?,
    });
    Ok((record, row.get(9)?))
}

fn name_row(row: &Row) -> rusqlite::Result<(Record, i64)> {
    let record = Record::ConstellationName(SyncedName {
        id: row.get(0)?,
        name: row.get(1)?,
        updated_at: row.get(2)?,
    });
    Ok((record, row.get(3)?))
}

fn profile_row(row: &Row) -> rusqlite::Result<(Record, i64)> {
    let json: String = row.get(1)?;
    let profile = serde_json::from_str(&json)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error)))?;
    let record = Record::Profile(SyncedProfile {
        id: row.get(0)?,
        profile,
        deleted: row.get(2)?,
        updated_at: row.get(3)?,
    });
    Ok((record, row.get(4)?))
}

/// Reads a row as a record and its number from the counter of changes made here.
type ReadRow = fn(&Row) -> rusqlite::Result<(Record, i64)>;

/// Each kind of record: its table, its columns and how to read a row of them.
const TABLES: [(&str, &str, ReadRow); 4] = [
    ("sessions", SESSION_COLUMNS, session_row),
    ("gate_visits", TRIP_COLUMNS, trip_row),
    ("constellation_names", NAME_COLUMNS, name_row),
    ("profiles", PROFILE_COLUMNS, profile_row),
];

/// Up to `limit` records changed here after `cursor`, in the order they changed, and the
/// cursor to ask from next time. Start from 0. Once it returns fewer than `limit`, everything
/// changed so far has been returned.
pub fn changed_since(db: &Connection, cursor: i64, limit: usize) -> Result<(Vec<Record>, i64)> {
    let limit = limit.max(1);
    let sample = format!("{SAMPLE_PREFIX}%");
    let mut changed = Vec::new();
    for (table, columns, read) in TABLES {
        let mut statement = db.prepare(&format!(
            "SELECT {columns} FROM {table} WHERE change_seq > ?1 AND id NOT LIKE ?2 ORDER BY change_seq LIMIT ?3"
        ))?;
        let rows = statement.query_map(params![cursor, sample, limit as i64], read)?;
        changed.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    changed.sort_by_key(|(_, change)| *change);
    changed.truncate(limit);
    let next = match changed.last() {
        Some((_, change)) if changed.len() == limit => *change,
        _ => {
            let last: i64 = db.query_row("SELECT last FROM change_counter", [], |row| row.get(0))?;
            last.max(cursor)
        }
    };
    Ok((changed.into_iter().map(|(record, _)| record).collect(), next))
}

/// Saves records from another device wherever they win over the copy here (`Record::wins_over`),
/// keeping their `updated_at`, all in one go. Returns how many changed something.
pub fn apply(db: &mut Connection, records: &[Record]) -> Result<usize> {
    let tx = db.transaction()?;
    let mut applied = 0;
    for record in records.iter().filter(|r| r.can_apply()) {
        let index = match record {
            Record::Session(_) => 0,
            Record::Trip(_) => 1,
            Record::ConstellationName(_) => 2,
            Record::Profile(_) => 3,
        };
        let (table, columns, read) = TABLES[index];
        let here = tx
            .query_row(
                &format!("SELECT {columns} FROM {table} WHERE id = ?1"),
                params![record.id()],
                read,
            )
            .optional()?;
        if here.is_some_and(|(here, _)| !record.wins_over(&here)) {
            continue;
        }
        save(&tx, record)?;
        applied += 1;
    }
    tx.commit()?;
    Ok(applied)
}

/// Saves a record from another device as it is, numbered 0 so it isn't sent back.
fn save(db: &Connection, record: &Record) -> Result<()> {
    match record {
        Record::Session(s) => insert_session(db, &s.session, s.updated_at, 0)?,
        Record::Trip(t) => {
            let v = &t.visit;
            db.execute(
                &format!(
                    "INSERT OR REPLACE INTO gate_visits ({TRIP_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)"
                ),
                params![
                    v.id,
                    v.recipe_id,
                    v.at,
                    v.destination,
                    v.why,
                    v.intention,
                    v.continued,
                    t.session_id,
                    t.updated_at
                ],
            )?;
        }
        Record::ConstellationName(n) => {
            db.execute(
                &format!("INSERT OR REPLACE INTO constellation_names ({NAME_COLUMNS}) VALUES (?1, ?2, ?3, 0)"),
                params![n.id, n.name, n.updated_at],
            )?;
        }
        Record::Profile(p) => {
            db.execute(
                &format!("INSERT OR REPLACE INTO profiles ({PROFILE_COLUMNS}) VALUES (?1, ?2, ?3, ?4, 0)"),
                params![p.id, serde_json::to_string(&p.profile)?, p.deleted, p.updated_at],
            )?;
        }
    }
    Ok(())
}

/// Notes each profile that changed between two versions of the list, as changes made here: new
/// and edited ones, and deleted ones, which keep their last copy.
pub fn note_profile_changes(db: &Connection, before: &[Profile], after: &[Profile], now: i64) -> Result<()> {
    let changed = after
        .iter()
        .filter(|p| before.iter().find(|b| b.id == p.id) != Some(*p))
        .map(|p| (p, false));
    let deleted = before
        .iter()
        .filter(|b| !after.iter().any(|p| p.id == b.id))
        .map(|b| (b, true));
    for (profile, deleted) in changed.chain(deleted) {
        db.execute(
            "INSERT INTO profiles (id, profile, deleted, updated_at, change_seq) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET profile = excluded.profile, deleted = excluded.deleted,
                updated_at = MAX(excluded.updated_at, profiles.updated_at + 1), change_seq = excluded.change_seq
             WHERE profiles.profile IS NOT excluded.profile OR profiles.deleted IS NOT excluded.deleted",
            params![
                profile.id,
                serde_json::to_string(profile)?,
                deleted,
                now,
                next_change(db)?
            ],
        )?;
    }
    Ok(())
}

/// Every profile sync knows of, in the order they were first saved here, and whether each one
/// has been deleted.
pub fn known_profiles(db: &Connection) -> Result<Vec<(Profile, bool)>> {
    let mut statement = db.prepare(&format!("SELECT {PROFILE_COLUMNS} FROM profiles ORDER BY rowid"))?;
    let rows = statement.query_map([], profile_row)?;
    let mut known = vec![];
    for row in rows {
        if let (Record::Profile(p), _) = row? {
            known.push((p.profile, p.deleted));
        }
    }
    Ok(known)
}

/// Numbers every record as changed here again, from other devices too, so the next
/// `changed_since` from the old cursor sends the whole log: for moving to another server, or
/// starting a new group.
pub fn resend_all(db: &mut Connection) -> Result<()> {
    let tx = db.transaction()?;
    for (table, _, _) in TABLES {
        tx.execute_batch(&format!(
            "UPDATE {table} SET change_seq = rowid + (SELECT last FROM change_counter);
             UPDATE change_counter SET last = last + (SELECT COALESCE(MAX(rowid), 0) FROM {table});"
        ))?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Store, Synced};

    const MIN: i64 = 60_000;

    fn session(id: &str, ended_at: i64) -> SessionRecord {
        SessionRecord {
            id: id.into(),
            profile_id: "deep-work".into(),
            profile_name: "Deep Work".into(),
            started_at: ended_at - 30 * MIN,
            ended_at,
            timer_minutes: Some(30),
            focused_ms: 30 * MIN,
            overtime_ms: 0,
            idle_tail_ms: 0,
            end_reason: EndReason::Stopped,
            broken: false,
            goal: Some("Draft the intro".into()),
            goal_met: None,
        }
    }

    fn visit(id: &str, at: i64) -> GateVisit {
        GateVisit {
            id: id.into(),
            recipe_id: "reddit".into(),
            at,
            destination: Some("gate".into()),
            why: None,
            intention: None,
            continued: false,
        }
    }

    /// Logs a session the way the engine does when one ends here.
    fn log(store: &mut Store, record: &SessionRecord) {
        let none = Synced {
            value: None::<()>,
            updated_at: record.ended_at,
        };
        store.end_session(record, "activeSession", &none).unwrap();
    }

    fn remote_session(id: &str, ended_at: i64, updated_at: i64) -> Record {
        Record::Session(SyncedSession {
            session: session(id, ended_at),
            updated_at,
        })
    }

    fn remote_name(id: &str, name: Option<&str>, updated_at: i64) -> Record {
        Record::ConstellationName(SyncedName {
            id: id.into(),
            name: name.map(Into::into),
            updated_at,
        })
    }

    fn everything(store: &Store) -> Vec<Record> {
        store.changed_since(0, 1000).unwrap().0
    }

    #[test]
    fn records_travel_as_flat_json_with_a_kind() {
        let records = vec![
            remote_session("s1", 30 * MIN, 31 * MIN),
            Record::Trip(SyncedTrip {
                visit: visit("t1", 5 * MIN),
                session_id: Some("s1".into()),
                updated_at: 6 * MIN,
            }),
            remote_name("s1", None, 40 * MIN),
        ];
        let json = serde_json::to_value(&records).unwrap();
        assert_eq!(json[0]["kind"], "session");
        assert_eq!(json[0]["id"], "s1");
        assert_eq!(json[0]["updatedAt"], 31 * MIN);
        assert_eq!(json[0]["endReason"], "stopped");
        assert_eq!(json[1]["kind"], "trip");
        assert_eq!(json[1]["sessionId"], "s1");
        assert_eq!(json[1]["recipeId"], "reddit");
        assert_eq!(json[2]["kind"], "constellationName");
        assert_eq!(json[2]["name"], serde_json::Value::Null);
        let back: Vec<Record> = serde_json::from_value(json).unwrap();
        assert_eq!(back, records);
    }

    #[test]
    fn sends_what_changed_here_in_order_without_samples() {
        let mut store = Store::open_in_memory().unwrap();
        log(&mut store, &session("s1", 30 * MIN));
        store
            .save_gate_visit(&visit("t1", 10 * MIN), Some("s1"), 10 * MIN)
            .unwrap();
        store.name_constellation("s1", Some("Orion"), 40 * MIN).unwrap();
        store
            .add_samples(&[session("sample-s", 20 * MIN)], &[visit("sample-t", 20 * MIN)])
            .unwrap();

        let (changed, cursor) = store.changed_since(0, 100).unwrap();
        let ids: Vec<_> = changed.iter().map(|r| (r.id(), r.updated_at())).collect();
        assert_eq!(ids, [("s1", 30 * MIN), ("t1", 10 * MIN), ("s1", 40 * MIN)]);
        assert!(matches!(changed[2], Record::ConstellationName(_)));
        assert_eq!(store.changed_since(cursor, 100).unwrap(), (vec![], cursor));

        // Answering the goal changes the session again, and only that's sent next.
        store.set_goal_met("s1", true, 45 * MIN).unwrap();
        let (changed, next) = store.changed_since(cursor, 100).unwrap();
        assert!(next > cursor);
        assert!(
            matches!(&changed[..], [Record::Session(s)] if s.session.goal_met == Some(true) && s.updated_at == 45 * MIN)
        );

        // The extension sending a trip again unchanged doesn't count as a change.
        store
            .save_gate_visit(&visit("t1", 10 * MIN), Some("s1"), 50 * MIN)
            .unwrap();
        assert_eq!(store.changed_since(next, 100).unwrap().0, vec![]);
    }

    #[test]
    fn sends_a_little_at_a_time() {
        let mut store = Store::open_in_memory().unwrap();
        for i in 0..5 {
            log(&mut store, &session(&format!("s{i}"), (i + 1) * 30 * MIN));
        }
        let mut cursor = 0;
        let mut sent = vec![];
        loop {
            let (changed, next) = store.changed_since(cursor, 2).unwrap();
            let done = changed.len() < 2;
            sent.extend(changed.iter().map(|r| r.id().to_owned()));
            cursor = next;
            if done {
                break;
            }
        }
        assert_eq!(sent, ["s0", "s1", "s2", "s3", "s4"]);
        assert_eq!(store.changed_since(cursor, 2).unwrap().0, vec![]);
    }

    #[test]
    fn the_newest_copy_wins_the_same_way_everywhere() {
        let older = remote_name("c", Some("Orion"), 10);
        let newer = remote_name("c", Some("Lyra"), 20);
        assert!(newer.wins_over(&older));
        assert!(!older.wins_over(&newer));

        // A tie goes the same way whichever device decides it, and a copy never beats itself.
        let a = remote_name("c", Some("Orion"), 10);
        let b = remote_name("c", Some("Lyra"), 10);
        assert_ne!(a.wins_over(&b), b.wins_over(&a));
        assert!(!a.wins_over(&a.clone()));

        // An answered goal beats an unanswered one, even an unanswered one that's newer.
        let mut answered = remote_session("s", 30 * MIN, 31 * MIN);
        if let Record::Session(s) = &mut answered {
            s.session.goal_met = Some(false);
        }
        let unanswered = remote_session("s", 30 * MIN, 90 * MIN);
        assert!(answered.wins_over(&unanswered));
        assert!(!unanswered.wins_over(&answered));
    }

    #[test]
    fn applies_what_wins_and_doesnt_send_it_back() {
        let mut store = Store::open_in_memory().unwrap();
        store.name_constellation("c", Some("Orion"), 20).unwrap();
        let (_, cursor) = store.changed_since(0, 100).unwrap();

        let applied = store
            .apply(&[
                remote_session("theirs", 30 * MIN, 30 * MIN),
                remote_name("c", Some("Older"), 10),
                remote_name("c2", Some("Lyra"), 5),
                remote_session("sample-1", 30 * MIN, 30 * MIN),
            ])
            .unwrap();
        assert_eq!(applied, 2);
        assert_eq!(
            store
                .sessions()
                .unwrap()
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["theirs"]
        );
        assert_eq!(
            store.constellation_names().unwrap(),
            [("c".into(), "Orion".into()), ("c2".into(), "Lyra".into())]
        );
        // What came from elsewhere keeps its time and isn't sent back.
        assert_eq!(store.changed_since(cursor, 100).unwrap().0, vec![]);

        // The same records again change nothing.
        assert_eq!(store.apply(&[remote_session("theirs", 30 * MIN, 30 * MIN)]).unwrap(), 0);
    }

    #[test]
    fn a_cleared_name_stays_cleared() {
        let mut store = Store::open_in_memory().unwrap();
        store.name_constellation("c", Some("Orion"), 10).unwrap();
        store.name_constellation("c", None, 20).unwrap();
        assert_eq!(store.constellation_names().unwrap(), vec![]);
        assert_eq!(everything(&store), [remote_name("c", None, 20)]);

        // An older name from another device doesn't bring it back; a newer one does.
        store.apply(&[remote_name("c", Some("Orion"), 15)]).unwrap();
        assert_eq!(store.constellation_names().unwrap(), vec![]);
        store.apply(&[remote_name("c", Some("Lyra"), 25)]).unwrap();
        assert_eq!(store.constellation_names().unwrap(), [("c".into(), "Lyra".into())]);

        // And a clear from elsewhere clears it here.
        store.apply(&[remote_name("c", None, 30)]).unwrap();
        assert_eq!(store.constellation_names().unwrap(), vec![]);
    }

    #[test]
    fn a_change_here_beats_a_copy_from_a_clock_thats_ahead() {
        let mut store = Store::open_in_memory().unwrap();
        // Another device's clock is an hour ahead.
        store.apply(&[remote_name("c", Some("Orion"), 60 * MIN)]).unwrap();
        store.name_constellation("c", Some("Lyra"), MIN).unwrap();
        let sent = everything(&store);
        assert_eq!(sent, [remote_name("c", Some("Lyra"), 60 * MIN + 1)]);
        assert!(sent[0].wins_over(&remote_name("c", Some("Orion"), 60 * MIN)));
    }

    #[test]
    fn resending_sends_everything_again() {
        let mut store = Store::open_in_memory().unwrap();
        log(&mut store, &session("mine", 30 * MIN));
        store.apply(&[remote_session("theirs", 60 * MIN, 60 * MIN)]).unwrap();
        let (_, cursor) = store.changed_since(0, 100).unwrap();

        store.resend_all().unwrap();
        let (changed, _) = store.changed_since(cursor, 100).unwrap();
        let mut ids: Vec<_> = changed.iter().map(Record::id).collect();
        ids.sort_unstable();
        assert_eq!(ids, ["mine", "theirs"]);
    }

    #[test]
    fn a_log_from_before_sync_is_ready_to_send() {
        use crate::store::MIGRATIONS;

        let db = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..7] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 7).unwrap();
        db.execute_batch(
            "INSERT INTO sessions (id, profile_id, profile_name, started_at, ended_at, focused_ms, overtime_ms,
                idle_tail_ms, end_reason) VALUES ('s1', 'p', 'P', 0, 1800000, 1800000, 0, 0, 'stopped');
             INSERT INTO gate_visits (id, recipe_id, at) VALUES ('t1', 'reddit', 600000);
             INSERT INTO constellation_names (id, name) VALUES ('s1', 'Orion');",
        )
        .unwrap();
        let mut store = Store::migrated_for_tests(db);

        let sent = everything(&store);
        let sent: Vec<_> = sent.iter().map(|r| (r.id(), r.updated_at())).collect();
        assert_eq!(sent, [("s1", 1_800_000), ("t1", 600_000), ("s1", 0)]);
        // Names can be cleared now, and new changes number on from the old ones.
        store.name_constellation("s1", None, 1).unwrap();
        log(&mut store, &session("s2", 60 * MIN));
        let (changed, _) = store.changed_since(3, 100).unwrap();
        assert_eq!(changed.len(), 2);
    }

    fn profile(id: &str, name: &str) -> Profile {
        serde_json::from_value(serde_json::json!({ "id": id, "name": name, "sites": [{ "domain": "reddit.com" }] }))
            .unwrap()
    }

    #[test]
    fn a_profile_travels_whole_under_its_own_key() {
        let record = Record::Profile(SyncedProfile {
            id: "p".into(),
            profile: profile("p", "Deep Work"),
            deleted: true,
            updated_at: 7,
        });
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["kind"], "profile");
        assert_eq!(json["id"], "p");
        assert_eq!(json["deleted"], true);
        assert_eq!(json["updatedAt"], 7);
        assert_eq!(json["profile"]["name"], "Deep Work");
        assert_eq!(json["profile"]["sites"][0]["domain"], "reddit.com");
        assert_eq!(serde_json::from_value::<Record>(json).unwrap(), record);
    }

    #[test]
    fn notes_new_edited_and_deleted_profiles() {
        let store = Store::open_in_memory().unwrap();
        let (a, b) = (profile("a", "A"), profile("b", "B"));
        store.note_profile_changes(&[], &[a.clone(), b.clone()], 10).unwrap();
        let (_, cursor) = store.changed_since(0, 100).unwrap();

        // Saving the same list again changes nothing; an edit and a deletion do.
        store
            .note_profile_changes(&[a.clone(), b.clone()], &[a.clone(), b.clone()], 20)
            .unwrap();
        assert_eq!(store.changed_since(cursor, 100).unwrap().0, vec![]);
        let renamed = profile("a", "A2");
        store
            .note_profile_changes(&[a, b.clone()], std::slice::from_ref(&renamed), 30)
            .unwrap();
        let changed = store.changed_since(cursor, 100).unwrap().0;
        assert_eq!(
            changed,
            [
                Record::Profile(SyncedProfile {
                    id: "a".into(),
                    profile: renamed,
                    deleted: false,
                    updated_at: 30
                }),
                Record::Profile(SyncedProfile {
                    id: "b".into(),
                    profile: b,
                    deleted: true,
                    updated_at: 30
                }),
            ]
        );
        let known: Vec<_> = store
            .known_profiles()
            .unwrap()
            .into_iter()
            .map(|(p, d)| (p.name, d))
            .collect();
        assert_eq!(known, [("A2".to_string(), false), ("B".to_string(), true)]);
    }

    #[test]
    fn a_profile_must_carry_its_own_id() {
        let mut store = Store::open_in_memory().unwrap();
        let mismatched = Record::Profile(SyncedProfile {
            id: "a".into(),
            profile: profile("b", "B"),
            deleted: false,
            updated_at: 1,
        });
        assert_eq!(store.apply(&[mismatched]).unwrap(), 0);
        assert_eq!(store.known_profiles().unwrap(), vec![]);
    }

    #[test]
    fn saved_profiles_are_ready_to_send() {
        use crate::store::MIGRATIONS;

        let db = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..8] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 8).unwrap();
        db.execute(
            "INSERT INTO state (key, value, updated_at) VALUES ('profiles', ?1, 42)",
            params![r#"[{"id":"a","name":"A","sites":[]},{"name":"no id"},7,{"id":"b","name":"B","sites":[]}]"#],
        )
        .unwrap();
        let store = Store::migrated_for_tests(db);
        let sent: Vec<_> = everything(&store)
            .iter()
            .map(|r| (r.id().to_owned(), r.updated_at()))
            .collect();
        assert_eq!(sent, [("a".to_string(), 42), ("b".to_string(), 42)]);
    }
}
