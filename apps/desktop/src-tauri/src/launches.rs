//! Hearing when apps launch or come back from being hidden, so a session can close or hide
//! them straight away instead of checking on a timer. macOS announces both; Linux has no way
//! for an ordinary program to hear about new processes, so there the session checks every
//! few seconds instead (`POLL_EVERY_MS`). Also hearing when the computer wakes from sleep, so
//! sync's live connection comes back straight away (sync/live.rs).
//!
//! This is the one place Seiza talks to macOS through `unsafe` code: the compiler can't
//! check the promises Apple's API asks for, so each one is written down where it's kept.

use std::sync::Arc;

use tokio::sync::Notify;

/// How often a session checks for apps and browsers where launches can't be heard. None
/// where they can.
#[cfg(target_os = "macos")]
pub const POLL_EVERY_MS: Option<i64> = None;
#[cfg(not(target_os = "macos"))]
pub const POLL_EVERY_MS: Option<i64> = Some(10_000);

/// Wakes `wake` whenever an app launches or is unhidden, for as long as the app runs.
#[cfg(target_os = "macos")]
pub fn listen(wake: Arc<Notify>) {
    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceDidLaunchApplicationNotification, NSWorkspaceDidUnhideApplicationNotification,
    };

    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: these are constants defined by AppKit, which is always loaded in a macOS app.
    // Rust can't see that something outside Rust has set them up, so reading them is unsafe.
    let names = unsafe {
        [
            NSWorkspaceDidLaunchApplicationNotification,
            NSWorkspaceDidUnhideApplicationNotification,
        ]
    };
    for name in names {
        let wake = Arc::clone(&wake);
        // Only wakes the session's watch; the work happens there, not on the main thread.
        let block = RcBlock::new(move |_| wake.notify_one());
        // SAFETY: Apple's three promises for this call:
        // - `object` is the right type: it's None, for notifications from every app.
        // - The queue's threading rules: it's None, so the block runs on the thread that posts
        //   the notification (the main thread, for these).
        // - The block can be sent to another thread: it only holds an `Arc<Notify>`, which
        //   is made to be shared between threads. Keep it that way; anything else captured
        //   here must be `Send + Sync` too.
        let observer = unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
        // The observer lasts as long as the app, so it's never removed: the token that would
        // remove it is deliberately kept forever.
        std::mem::forget(observer);
    }
}

/// Calls `woke` whenever the computer wakes from sleep, for as long as the app runs.
#[cfg(target_os = "macos")]
pub fn on_wake(woke: impl Fn() + Send + Sync + 'static) {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};

    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: a constant defined by AppKit, as in `listen`.
    let name = unsafe { NSWorkspaceDidWakeNotification };
    let block = RcBlock::new(move |_| woke());
    // SAFETY: as in `listen`: `object` and the queue are None, and the block holds only what
    // `woke` captures, which must be `Send + Sync` (the bound above keeps it so).
    let observer = unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
    // Kept forever, as in `listen`.
    std::mem::forget(observer);
}

/// Elsewhere, the live connection notices it's gone after a while instead.
#[cfg(not(target_os = "macos"))]
pub fn on_wake(_woke: impl Fn() + Send + Sync + 'static) {}

/// Elsewhere, sessions check on a timer instead (`POLL_EVERY_MS`).
#[cfg(not(target_os = "macos"))]
pub fn listen(_wake: Arc<Notify>) {}
