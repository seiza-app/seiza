use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::backup::{History, ImportReport, LoggedSatellite, LoggedVisit};
use crate::engine::ConstellationName;
use crate::error::Result;
use crate::records::{self, Record};
use crate::satellites::{Satellite, SatelliteRecord, SatelliteState};
use crate::session::{EndReason, GateVisit, Profile, SessionRecord};

/// A value that's kept in step with the extension, with when it last changed. The newest
/// change wins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Synced<T> {
    pub value: T,
    /// Epoch milliseconds. 0 means it has never been changed.
    pub updated_at: i64,
}

/// The app's SQLite database: the profiles, the running session, the session log and
/// satellites.
pub struct Store {
    db: Connection,
}

/// Each entry upgrades the database by one version. Add new ones to the end; never edit old ones.
pub(crate) const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE state (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE TABLE sessions (
        id TEXT PRIMARY KEY,
        profile_id TEXT NOT NULL,
        profile_name TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        ended_at INTEGER NOT NULL,
        timer_minutes INTEGER,
        focused_ms INTEGER NOT NULL,
        overtime_ms INTEGER NOT NULL,
        idle_tail_ms INTEGER NOT NULL,
        end_reason TEXT NOT NULL
    );
    CREATE INDEX sessions_started_at ON sessions (started_at);
"#,
    r#"
    CREATE TABLE gate_visits (
        id TEXT PRIMARY KEY,
        session_id TEXT,
        recipe_id TEXT NOT NULL,
        at INTEGER NOT NULL,
        destination TEXT,
        why TEXT,
        intention TEXT,
        continued INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX gate_visits_at ON gate_visits (at);
"#,
    r#"
    ALTER TABLE sessions ADD COLUMN broken INTEGER NOT NULL DEFAULT 0;
"#,
    // Satellites that are done or deleted stay, for insights, but deleted ones lose their text.
    r#"
    CREATE TABLE satellites (
        id TEXT PRIMARY KEY,
        text TEXT NOT NULL,
        launched_at INTEGER NOT NULL,
        session_id TEXT,
        state TEXT NOT NULL,
        state_at INTEGER NOT NULL,
        snoozes INTEGER NOT NULL DEFAULT 0,
        tasked_at INTEGER
    );
    CREATE INDEX satellites_state ON satellites (state);
"#,
    // Only the names people give constellations. The constellations themselves are worked out
    // from the session log, keyed by their first star's id.
    r#"
    CREATE TABLE constellation_names (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL
    );
"#,
    // A session's goal, if its profile asks for one, and whether it was met (NULL until answered).
    r#"
    ALTER TABLE sessions ADD COLUMN goal TEXT;
    ALTER TABLE sessions ADD COLUMN goal_met INTEGER;
"#,
    // Ending sessions for being idle became off by default. Installs from before then that
    // never changed their settings keep the old 15 minutes; a new one has nothing saved yet.
    // And sessions are no longer marked broken, so the running one's mark goes.
    r#"
    INSERT INTO state (key, value, updated_at)
    SELECT 'settings', '{"idleMinutes":15}', 0
    WHERE NOT EXISTS (SELECT 1 FROM state WHERE key = 'settings')
        AND (EXISTS (SELECT 1 FROM state) OR EXISTS (SELECT 1 FROM sessions));
    DELETE FROM state WHERE key = 'brokenSession';
"#,
    // Ready for sync (records.rs): each session, trip and constellation name gets when it last
    // changed, and a number from a counter of changes made here, so sync can send what's new.
    // What's here so far is numbered as made here. A cleared name stays, with no name, so
    // clearing it can reach other devices.
    r#"
    CREATE TABLE change_counter (last INTEGER NOT NULL);
    INSERT INTO change_counter (last) VALUES (0);

    ALTER TABLE sessions ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN change_seq INTEGER NOT NULL DEFAULT 0;
    UPDATE sessions SET updated_at = ended_at, change_seq = rowid;
    UPDATE change_counter SET last = (SELECT COALESCE(MAX(rowid), 0) FROM sessions);
    CREATE INDEX sessions_change_seq ON sessions (change_seq);

    ALTER TABLE gate_visits ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE gate_visits ADD COLUMN change_seq INTEGER NOT NULL DEFAULT 0;
    UPDATE gate_visits SET updated_at = at, change_seq = rowid + (SELECT last FROM change_counter);
    UPDATE change_counter SET last = last + (SELECT COALESCE(MAX(rowid), 0) FROM gate_visits);
    CREATE INDEX gate_visits_change_seq ON gate_visits (change_seq);

    CREATE TABLE names (
        id TEXT PRIMARY KEY,
        name TEXT,
        updated_at INTEGER NOT NULL DEFAULT 0,
        change_seq INTEGER NOT NULL DEFAULT 0
    );
    INSERT INTO names (id, name) SELECT id, name FROM constellation_names;
    UPDATE names SET change_seq = rowid + (SELECT last FROM change_counter);
    UPDATE change_counter SET last = last + (SELECT COALESCE(MAX(rowid), 0) FROM names);
    DROP TABLE constellation_names;
    ALTER TABLE names RENAME TO constellation_names;
    CREATE INDEX constellation_names_change_seq ON constellation_names (change_seq);
"#,
    // Each profile on its own for sync (records.rs), next to the whole list the extension gets,
    // which stays in `state`. A deleted profile stays, marked deleted, so the deletion can sync
    // and its stars keep their color.
    r#"
    CREATE TABLE profiles (
        id TEXT PRIMARY KEY,
        profile TEXT NOT NULL,
        deleted INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL DEFAULT 0,
        change_seq INTEGER NOT NULL DEFAULT 0
    );
    INSERT OR IGNORE INTO profiles (id, profile, updated_at)
    SELECT json_extract(each.value, '$.id'), each.value, (SELECT updated_at FROM state WHERE key = 'profiles')
    FROM json_each((SELECT value FROM state WHERE key = 'profiles' AND json_valid(value))) AS each
    WHERE each.type = 'object' AND json_type(each.value, '$.id') = 'text';
    UPDATE profiles SET change_seq = rowid + (SELECT last FROM change_counter);
    UPDATE change_counter SET last = last + (SELECT COALESCE(MAX(rowid), 0) FROM profiles);
    CREATE INDEX profiles_change_seq ON profiles (change_seq);
"#,
];

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::migrated(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::migrated(Connection::open_in_memory()?)
    }

    /// Brings a database made by the tests up to date.
    #[cfg(test)]
    pub fn migrated_for_tests(db: Connection) -> Self {
        Self::migrated(db).unwrap()
    }

    #[cfg(test)]
    pub fn db_for_tests(&self) -> &Connection {
        &self.db
    }

    fn migrated(mut db: Connection) -> Result<Self> {
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let tx = db.transaction()?;
        for (index, migration) in (0_i64..).zip(MIGRATIONS).skip(version as usize) {
            tx.execute_batch(migration)?;
            tx.pragma_update(None, "user_version", index + 1)?;
        }
        tx.commit()?;
        Ok(Self { db })
    }

    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Synced<T>>> {
        let row = self
            .db
            .query_row(
                "SELECT value, updated_at FROM state WHERE key = ?1",
                params![key],
                |row| Ok((row.get::<_, String>(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((json, updated_at)) = row else {
            return Ok(None);
        };
        Ok(Some(Synced {
            value: serde_json::from_str(&json)?,
            updated_at,
        }))
    }

    pub fn save<T: Serialize>(&self, key: &str, synced: &Synced<T>) -> Result<()> {
        save(&self.db, key, synced)
    }

    /// Logs a finished session, saves the new state and brings satellites back, all in one go,
    /// so a crash can't log a session that still looks like it's running.
    pub fn end_session<T: Serialize>(&mut self, record: &SessionRecord, key: &str, synced: &Synced<T>) -> Result<()> {
        let tx = self.db.transaction()?;
        let change = records::next_change(&tx)?;
        insert_session(&tx, record, record.ended_at, change)?;
        save(&tx, key, synced)?;
        tx.execute(
            "UPDATE satellites SET state = ?1, state_at = ?2 WHERE state = ?3",
            params![
                SatelliteState::Back.as_str(),
                record.ended_at,
                SatelliteState::Orbiting.as_str()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Saves a new satellite, in whatever state it's in. One that's already saved is left as
    /// it is. Returns whether it was new.
    pub fn save_satellite(&self, satellite: &Satellite, session_id: Option<&str>) -> Result<bool> {
        let tasked = matches!(satellite.state, SatelliteState::Task | SatelliteState::Done);
        let added = self.db.execute(
            "INSERT OR IGNORE INTO satellites (id, text, launched_at, session_id, state, state_at, tasked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                satellite.id,
                satellite.text,
                satellite.launched_at,
                session_id,
                satellite.state.as_str(),
                satellite.state_at,
                tasked.then_some(satellite.state_at)
            ],
        )?;
        Ok(added > 0)
    }

    /// Moves a satellite to `state`, counting snoozes and when it first became a task.
    pub fn move_satellite(&self, id: &str, state: SatelliteState, now: i64) -> Result<()> {
        let extra = match state {
            SatelliteState::Orbiting => ", snoozes = snoozes + 1",
            SatelliteState::Task => ", tasked_at = COALESCE(tasked_at, ?3)",
            SatelliteState::Deleted => ", text = ''",
            SatelliteState::Back | SatelliteState::Done => "",
        };
        self.db.execute(
            &format!("UPDATE satellites SET state = ?2, state_at = ?3{extra} WHERE id = ?1"),
            params![id, state.as_str(), now],
        )?;
        Ok(())
    }

    /// The satellites still in orbit, back or on the task list, and tasks ticked off since
    /// `done_since`, oldest first.
    pub fn shown_satellites(&self, done_since: i64) -> Result<Vec<Satellite>> {
        let mut statement = self.db.prepare(
            "SELECT id, text, launched_at, state, state_at FROM satellites
             WHERE state IN (?1, ?2, ?3) OR (state = ?4 AND tasked_at IS NOT NULL AND state_at >= ?5)
             ORDER BY launched_at",
        )?;
        let rows = statement.query_map(
            params![
                SatelliteState::Orbiting.as_str(),
                SatelliteState::Back.as_str(),
                SatelliteState::Task.as_str(),
                SatelliteState::Done.as_str(),
                done_since
            ],
            |row| {
                Ok(Satellite {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    launched_at: row.get(2)?,
                    state: SatelliteState::parse(&row.get::<_, String>(3)?).unwrap_or(SatelliteState::Back),
                    state_at: row.get(4)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Saves a trip to a redirected site. The extension sends it again as the user answers
    /// "why?" or continues, so later sends update it, keeping the session it started in. A send
    /// that changes nothing leaves it as it was, so it isn't synced again.
    pub fn save_gate_visit(&self, visit: &GateVisit, session_id: Option<&str>, now: i64) -> Result<()> {
        self.db.execute(
            "INSERT INTO gate_visits (id, session_id, recipe_id, at, destination, why, intention, continued,
                updated_at, change_seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (id) DO UPDATE SET destination = excluded.destination, why = excluded.why,
                intention = excluded.intention, continued = excluded.continued,
                updated_at = MAX(excluded.updated_at, gate_visits.updated_at + 1), change_seq = excluded.change_seq
             WHERE gate_visits.destination IS NOT excluded.destination OR gate_visits.why IS NOT excluded.why
                OR gate_visits.intention IS NOT excluded.intention OR gate_visits.continued IS NOT excluded.continued",
            params![
                visit.id,
                session_id,
                visit.recipe_id,
                visit.at,
                visit.destination,
                visit.why,
                visit.intention,
                visit.continued,
                now,
                records::next_change(&self.db)?,
            ],
        )?;
        Ok(())
    }

    /// Adds made-up sessions and gate visits, for trying the app (samples.rs).
    pub fn add_samples(&mut self, sessions: &[SessionRecord], visits: &[GateVisit]) -> Result<()> {
        let tx = self.db.transaction()?;
        for record in sessions {
            insert_session(&tx, record, record.ended_at, records::next_change(&tx)?)?;
        }
        tx.commit()?;
        for visit in visits {
            self.save_gate_visit(visit, None, visit.at)?;
        }
        Ok(())
    }

    /// Removes everything `add_samples` added, which is everything with a sample id.
    pub fn remove_samples(&self) -> Result<()> {
        let pattern = format!("{}%", crate::samples::SAMPLE_PREFIX);
        self.db
            .execute("DELETE FROM sessions WHERE id LIKE ?1", params![pattern])?;
        self.db
            .execute("DELETE FROM gate_visits WHERE id LIKE ?1", params![pattern])?;
        self.db
            .execute("DELETE FROM constellation_names WHERE id LIKE ?1", params![pattern])?;
        Ok(())
    }

    /// Whether the session log has a session with this id.
    pub fn has_session(&self, id: &str) -> Result<bool> {
        let found = self
            .db
            .query_row("SELECT 1 FROM sessions WHERE id = ?1", params![id], |_| Ok(()))
            .optional()?;
        Ok(found.is_some())
    }

    /// The names people gave constellations, as (constellation id, name). Cleared ones are
    /// left out.
    pub fn constellation_names(&self) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .db
            .prepare("SELECT id, name FROM constellation_names WHERE name IS NOT NULL ORDER BY id")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Names a constellation, or with None, forgets its name. A forgotten name is kept as
    /// cleared, so sync can tell other devices.
    pub fn name_constellation(&self, id: &str, name: Option<&str>, now: i64) -> Result<()> {
        let change = records::next_change(&self.db)?;
        match name {
            Some(name) => self.db.execute(
                "INSERT INTO constellation_names (id, name, updated_at, change_seq) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name,
                    updated_at = MAX(excluded.updated_at, constellation_names.updated_at + 1),
                    change_seq = excluded.change_seq
                 WHERE constellation_names.name IS NOT excluded.name",
                params![id, name, now, change],
            )?,
            None => self.db.execute(
                "UPDATE constellation_names SET name = NULL, updated_at = MAX(?2, updated_at + 1), change_seq = ?3
                 WHERE id = ?1 AND name IS NOT NULL",
                params![id, now, change],
            )?,
        };
        Ok(())
    }

    /// Gate visits as (session id, visit), oldest first.
    pub fn gate_visits(&self) -> Result<Vec<(Option<String>, GateVisit)>> {
        self.gate_visits_between(i64::MIN, i64::MAX)
    }

    /// Gate visits from `from` up to (not including) `to`, as (session id, visit), oldest first.
    pub fn gate_visits_between(&self, from: i64, to: i64) -> Result<Vec<(Option<String>, GateVisit)>> {
        let mut statement = self.db.prepare(
            "SELECT session_id, id, recipe_id, at, destination, why, intention, continued
             FROM gate_visits WHERE at >= ?1 AND at < ?2 ORDER BY at",
        )?;
        let rows = statement.query_map(params![from, to], |row| {
            Ok((
                row.get(0)?,
                GateVisit {
                    id: row.get(1)?,
                    recipe_id: row.get(2)?,
                    at: row.get(3)?,
                    destination: row.get(4)?,
                    why: row.get(5)?,
                    intention: row.get(6)?,
                    continued: row.get(7)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// When each emergency pass was used (epoch ms), oldest first.
    pub fn pass_uses(&self) -> Result<Vec<i64>> {
        let mut statement = self
            .db
            .prepare("SELECT ended_at FROM sessions WHERE end_reason = ?1 ORDER BY ended_at")?;
        let rows = statement.query_map(params![EndReason::EmergencyPass.as_str()], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The session log, oldest first.
    pub fn sessions(&self) -> Result<Vec<SessionRecord>> {
        self.sessions_between(i64::MIN, i64::MAX)
    }

    /// The sessions that started from `from` up to (not including) `to`, oldest first.
    pub fn sessions_between(&self, from: i64, to: i64) -> Result<Vec<SessionRecord>> {
        let mut statement = self.db.prepare(
            "SELECT id, profile_id, profile_name, started_at, ended_at, timer_minutes,
                focused_ms, overtime_ms, idle_tail_ms, end_reason, broken, goal, goal_met
             FROM sessions WHERE started_at >= ?1 AND started_at < ?2 ORDER BY started_at",
        )?;
        let rows = statement.query_map(params![from, to], |row| {
            Ok(SessionRecord {
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
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Saves the answer to "Did you do it?" for a logged session that had a goal.
    pub fn set_goal_met(&self, session_id: &str, met: bool, now: i64) -> Result<()> {
        self.db.execute(
            "UPDATE sessions SET goal_met = ?2, updated_at = MAX(?3, updated_at + 1), change_seq = ?4
             WHERE id = ?1 AND goal IS NOT NULL AND goal_met IS NOT ?2",
            params![session_id, met, now, records::next_change(&self.db)?],
        )?;
        Ok(())
    }

    /// The satellites launched from `from` up to (not including) `to`, and what became of
    /// them, oldest first. What they said is left out; insights don't need it.
    pub fn satellites_launched_between(&self, from: i64, to: i64) -> Result<Vec<SatelliteRecord>> {
        let mut statement = self.db.prepare(
            "SELECT launched_at, state, state_at, snoozes, tasked_at FROM satellites
             WHERE launched_at >= ?1 AND launched_at < ?2 ORDER BY launched_at",
        )?;
        let rows = statement.query_map(params![from, to], |row| {
            Ok(SatelliteRecord {
                launched_at: row.get(0)?,
                state: SatelliteState::parse(&row.get::<_, String>(1)?).unwrap_or(SatelliteState::Back),
                state_at: row.get(2)?,
                snoozes: row.get(3)?,
                tasked_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Everything logged, for a backup. Sample data stays out.
    pub fn history(&self) -> Result<History> {
        let sample = |id: &str| id.starts_with(crate::samples::SAMPLE_PREFIX);
        let mut statement = self.db.prepare(
            "SELECT id, text, launched_at, session_id, state, state_at, snoozes, tasked_at
             FROM satellites ORDER BY launched_at",
        )?;
        let satellites = statement.query_map([], |row| {
            Ok(LoggedSatellite {
                id: row.get(0)?,
                text: row.get(1)?,
                launched_at: row.get(2)?,
                session_id: row.get(3)?,
                state: SatelliteState::parse(&row.get::<_, String>(4)?).unwrap_or(SatelliteState::Back),
                state_at: row.get(5)?,
                snoozes: row.get(6)?,
                tasked_at: row.get(7)?,
            })
        })?;
        Ok(History {
            sessions: self.sessions()?.into_iter().filter(|s| !sample(&s.id)).collect(),
            gate_visits: self
                .gate_visits()?
                .into_iter()
                .filter(|(_, visit)| !sample(&visit.id))
                .map(|(session_id, visit)| LoggedVisit { session_id, visit })
                .collect(),
            satellites: satellites.collect::<rusqlite::Result<_>>()?,
            constellation_names: self
                .constellation_names()?
                .into_iter()
                .filter(|(id, _)| !sample(id))
                .map(|(id, name)| ConstellationName { id, name })
                .collect(),
        })
    }

    /// Brings in a backup's history and saves `state` (key, value), all in one go, so an import
    /// that fails part way changes nothing. What's here already wins over the backup's copy.
    pub fn import(&mut self, history: &History, state: &[(&str, Synced<Value>)]) -> Result<ImportReport> {
        let tx = self.db.transaction()?;
        let mut report = ImportReport::default();
        for record in &history.sessions {
            report.sessions += tx.execute(
                "INSERT OR IGNORE INTO sessions (id, profile_id, profile_name, started_at, ended_at, timer_minutes,
                    focused_ms, overtime_ms, idle_tail_ms, end_reason, broken, goal, goal_met, updated_at, change_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?5, ?14)",
                params![
                    record.id,
                    record.profile_id,
                    record.profile_name,
                    record.started_at,
                    record.ended_at,
                    record.timer_minutes,
                    record.focused_ms,
                    record.overtime_ms,
                    record.idle_tail_ms,
                    record.end_reason.as_str(),
                    record.broken,
                    record.goal,
                    record.goal_met,
                    records::next_change(&tx)?,
                ],
            )? as u32;
        }
        for LoggedVisit { session_id, visit } in &history.gate_visits {
            report.gate_visits += tx.execute(
                "INSERT OR IGNORE INTO gate_visits (id, session_id, recipe_id, at, destination, why, intention, continued,
                    updated_at, change_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?4, ?9)",
                params![
                    visit.id,
                    session_id,
                    visit.recipe_id,
                    visit.at,
                    visit.destination,
                    visit.why,
                    visit.intention,
                    visit.continued,
                    records::next_change(&tx)?,
                ],
            )? as u32;
        }
        for satellite in &history.satellites {
            report.satellites += tx.execute(
                "INSERT OR IGNORE INTO satellites (id, text, launched_at, session_id, state, state_at, snoozes, tasked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    satellite.id,
                    satellite.text,
                    satellite.launched_at,
                    satellite.session_id,
                    satellite.state.as_str(),
                    satellite.state_at,
                    satellite.snoozes,
                    satellite.tasked_at,
                ],
            )? as u32;
        }
        for name in &history.constellation_names {
            tx.execute(
                "INSERT OR IGNORE INTO constellation_names (id, name, change_seq) VALUES (?1, ?2, ?3)",
                params![name.id, name.name, records::next_change(&tx)?],
            )?;
        }
        for (key, synced) in state {
            save(&tx, key, synced)?;
        }
        tx.commit()?;
        Ok(report)
    }

    /// Saves the whole list of profiles under `key`, and notes each one that changed from
    /// `before` for sync, all in one go.
    pub fn save_profiles(
        &mut self,
        key: &str,
        profiles: &Synced<Vec<Profile>>,
        before: &[Profile],
        now: i64,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        save(&tx, key, profiles)?;
        records::note_profile_changes(&tx, before, &profiles.value, now)?;
        tx.commit()?;
        Ok(())
    }

    /// Notes each profile that changed from `before` to `after` for sync.
    pub fn note_profile_changes(&self, before: &[Profile], after: &[Profile], now: i64) -> Result<()> {
        records::note_profile_changes(&self.db, before, after, now)
    }

    /// Every profile sync knows of, in the order they were first saved, and whether each one
    /// has been deleted.
    pub fn known_profiles(&self) -> Result<Vec<(Profile, bool)>> {
        records::known_profiles(&self.db)
    }

    /// Records changed here since `cursor`, for sync (records.rs).
    pub fn changed_since(&self, cursor: i64, limit: usize) -> Result<(Vec<Record>, i64)> {
        records::changed_since(&self.db, cursor, limit)
    }

    /// Saves records from another device where they win over the copy here (records.rs).
    pub fn apply(&mut self, records: &[Record]) -> Result<usize> {
        records::apply(&mut self.db, records)
    }

    /// Marks every record to be sent again (records.rs).
    pub fn resend_all(&mut self) -> Result<()> {
        records::resend_all(&mut self.db)
    }

    /// When the first session, gate visit or satellite was logged (epoch ms), if anything has
    /// been.
    pub fn first_logged_at(&self) -> Result<Option<i64>> {
        Ok(self.db.query_row(
            "SELECT MIN(at) FROM (
                SELECT MIN(started_at) AS at FROM sessions
                UNION ALL SELECT MIN(at) FROM gate_visits
                UNION ALL SELECT MIN(launched_at) FROM satellites
             )",
            [],
            |row| row.get(0),
        )?)
    }
}

/// Saves a session to the log, replacing any with its id. `change` is its number from the
/// counter of changes made here (records.rs), or 0 for one that came from another device.
pub(crate) fn insert_session(db: &Connection, record: &SessionRecord, updated_at: i64, change: i64) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO sessions (id, profile_id, profile_name, started_at, ended_at, timer_minutes,
            focused_ms, overtime_ms, idle_tail_ms, end_reason, broken, goal, goal_met, updated_at, change_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            record.id,
            record.profile_id,
            record.profile_name,
            record.started_at,
            record.ended_at,
            record.timer_minutes,
            record.focused_ms,
            record.overtime_ms,
            record.idle_tail_ms,
            record.end_reason.as_str(),
            record.broken,
            record.goal,
            record.goal_met,
            updated_at,
            change,
        ],
    )?;
    Ok(())
}

fn save<T: Serialize>(db: &Connection, key: &str, synced: &Synced<T>) -> Result<()> {
    db.execute(
        "INSERT INTO state (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, serde_json::to_string(&synced.value)?, synced.updated_at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_state() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.load::<Vec<String>>("profiles").unwrap(), None);

        let synced = Synced {
            value: vec!["a".to_string()],
            updated_at: 42,
        };
        store.save("profiles", &synced).unwrap();
        store
            .save(
                "profiles",
                &Synced {
                    value: vec!["b".to_string()],
                    updated_at: 43,
                },
            )
            .unwrap();
        assert_eq!(
            store.load::<Vec<String>>("profiles").unwrap(),
            Some(Synced {
                value: vec!["b".to_string()],
                updated_at: 43
            })
        );
    }

    #[test]
    fn migrations_run_once() {
        let dir = std::env::temp_dir().join(format!("lock-in-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        Store::open(&path)
            .unwrap()
            .save(
                "k",
                &Synced {
                    value: 1,
                    updated_at: 1,
                },
            )
            .unwrap();
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.load::<i32>("k").unwrap().map(|s| s.value), Some(1));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A database as it was before ending sessions for being idle became off by default.
    fn from_before_idle_off(rows: &[(&str, &str)]) -> Store {
        let db = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..6] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 6).unwrap();
        for (key, value) in rows {
            db.execute(
                "INSERT INTO state (key, value, updated_at) VALUES (?1, ?2, 1)",
                params![key, value],
            )
            .unwrap();
        }
        Store::migrated(db).unwrap()
    }

    #[test]
    fn older_installs_keep_ending_idle_sessions() {
        use crate::settings::Settings;

        let fresh = Store::open_in_memory().unwrap();
        assert_eq!(fresh.load::<Settings>("settings").unwrap(), None);
        assert_eq!(Settings::default().idle_minutes, 0);

        let older = from_before_idle_off(&[("tour", "true"), ("brokenSession", r#""s1""#)]);
        let settings = older.load::<Settings>("settings").unwrap().unwrap();
        assert_eq!(settings.value.idle_minutes, 15);
        assert_eq!(settings.updated_at, 0);
        assert_eq!(older.load::<Option<String>>("brokenSession").unwrap(), None);

        // Settings someone saved are left as they were.
        let changed = from_before_idle_off(&[("settings", r#"{"idleMinutes":30,"passesPerWeek":4}"#)]);
        let settings = changed.load::<Settings>("settings").unwrap().unwrap().value;
        assert_eq!((settings.idle_minutes, settings.passes_per_week), (30, 4));
    }
}
