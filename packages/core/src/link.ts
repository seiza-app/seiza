import type { GateVisit, Profile, Session, SiteSettings } from './types';

// Messages between the extension and the desktop app, which talk over Native Messaging.
// They mirror the Rust ones in apps/desktop/src-tauri/src/link.rs.

/** The Native Messaging host the extension connects to: the desktop app's bridge. */
export const desktopHostName = 'io.github.lock_in_focus_app.lock_in';

/** Bumped when a change to the messages would break the other side. */
export const linkProtocol = 1;

/** A value kept in step between the extension and the app. The newest change wins. */
export interface Synced<T> {
  value: T;
  /** Epoch milliseconds. 0 means it has never been changed. */
  updatedAt: number;
}

export type ToDesktop =
  | { type: 'hello'; protocol: number; browser: string }
  | {
      type: 'state';
      profiles?: Synced<Profile[]>;
      /**
       * The `updatedAt` of the app's profiles this side last took, so the app keeps changes it
       * made after that (from sync, say) rather than letting this side's list undo them.
       */
      profilesBasedOn?: number;
      activeSession?: Synced<Session | null>;
      siteSettings?: Synced<SiteSettings[]>;
    }
  | { type: 'gateVisit'; visit: GateVisit };

export type FromDesktop =
  /** The bridge reached the app, or lost it. Sent by the bridge itself. */
  | { type: 'linked' }
  | { type: 'unlinked' }
  | {
      type: 'state';
      profiles: Synced<Profile[]>;
      activeSession: Synced<Session | null>;
      /** Missing from apps older than site settings. */
      siteSettings?: Synced<SiteSettings[]>;
    };
