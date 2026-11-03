// The time the app goes by. It's the computer's clock, except while a session runs: then a
// change to the clock (setting it hours ahead, say) is left out, so a Hard session can't be ended
// early that way while Seiza runs. The time since the computer started, which keeps counting
// while it sleeps and can't be set, says how much time really went by. Once the session ends,
// the app goes by the computer's clock again, changes and all.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// How far the computer's clock can drift from the time since it started between two readings
/// before it counts as changed. The system's own corrections are much smaller than this.
const CHANGE_MS: i64 = 5_000;

static CLOCK: Mutex<Clock> = Mutex::new(Clock {
    last: None,
    held: false,
});

/// The current time in epoch milliseconds, like `Date.now()`.
pub fn now_ms() -> i64 {
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let Some(boot) = boot_ms() else { return wall };
    lock().read(wall, boot)
}

/// Leaves changes to the computer's clock out while a session runs, and stops once it ends.
pub fn hold(running: bool) {
    lock().held = running;
}

fn lock() -> std::sync::MutexGuard<'static, Clock> {
    CLOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug)]
struct Clock {
    /// The last reading kept: the computer's clock, and the time since it started, then.
    last: Option<(i64, i64)>,
    held: bool,
}

impl Clock {
    fn read(&mut self, wall: i64, boot: i64) -> i64 {
        if let Some((last_wall, last_boot)) = self.last {
            let steady = last_wall + (boot - last_boot);
            if self.held && (wall - steady).abs() > CHANGE_MS {
                return steady;
            }
        }
        self.last = Some((wall, boot));
        wall
    }
}

/// The time since the computer started, sleep included, in milliseconds. Rust's `Instant`
/// leaves sleep out on both systems.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn boot_ms() -> Option<i64> {
    // On macOS, CLOCK_MONOTONIC counts sleep; on Linux, that's CLOCK_BOOTTIME.
    #[cfg(target_os = "macos")]
    const CLOCK_ID: libc::clockid_t = libc::CLOCK_MONOTONIC;
    #[cfg(target_os = "linux")]
    const CLOCK_ID: libc::clockid_t = libc::CLOCK_BOOTTIME;
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `time` is a valid timespec for the call to write into.
    let read = unsafe { libc::clock_gettime(CLOCK_ID, &mut time) };
    // Both are already i64 on 64-bit systems, but not on every 32-bit one.
    #[allow(clippy::unnecessary_cast)]
    (read == 0).then(|| time.tv_sec as i64 * 1000 + time.tv_nsec as i64 / 1_000_000)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn boot_ms() -> Option<i64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 60 * 60_000;

    #[test]
    fn a_running_session_goes_by_the_time_that_really_went_by() {
        let mut clock = Clock { last: None, held: true };
        assert_eq!(clock.read(1_000 * HOUR, 5 * HOUR), 1_000 * HOUR);
        // Ten minutes later, the clock is set three hours ahead.
        assert_eq!(
            clock.read(1_003 * HOUR + 10 * 60_000, 5 * HOUR + 10 * 60_000),
            1_000 * HOUR + 10 * 60_000
        );
        // And keeps being left out, as is setting it back.
        assert_eq!(clock.read(1_004 * HOUR, 6 * HOUR), 1_001 * HOUR);
        assert_eq!(clock.read(990 * HOUR, 7 * HOUR), 1_002 * HOUR);
    }

    #[test]
    fn small_corrections_and_sleep_go_by_the_computers_clock() {
        let mut clock = Clock { last: None, held: true };
        clock.read(1_000 * HOUR, 5 * HOUR);
        // Asleep for eight hours, and the clock corrected by a second on waking.
        assert_eq!(clock.read(1_008 * HOUR + 1_000, 13 * HOUR), 1_008 * HOUR + 1_000);
    }

    #[test]
    fn between_sessions_it_goes_by_the_computers_clock() {
        let mut clock = Clock { last: None, held: true };
        clock.read(1_000 * HOUR, 5 * HOUR);
        assert_eq!(clock.read(1_003 * HOUR, 5 * HOUR + 60_000), 1_000 * HOUR + 60_000);
        clock.held = false;
        assert_eq!(
            clock.read(1_003 * HOUR + 60_000, 5 * HOUR + 2 * 60_000),
            1_003 * HOUR + 60_000
        );
        // A session started then goes by the clock as it is from then on.
        clock.held = true;
        assert_eq!(
            clock.read(1_003 * HOUR + 2 * 60_000, 5 * HOUR + 3 * 60_000),
            1_003 * HOUR + 2 * 60_000
        );
    }

    #[test]
    fn the_time_since_the_computer_started_counts_up() {
        let first = boot_ms().unwrap();
        assert!(first > 0);
        assert!(boot_ms().unwrap() >= first);
    }
}
