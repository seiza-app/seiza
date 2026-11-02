import type { AppRule } from './generated/AppRule';
import type { HiddenPart } from './generated/HiddenPart';
import type { OwnQuote } from './generated/OwnQuote';
import type { PathRule } from './generated/PathRule';
import type { SiteLevel } from './generated/SiteLevel';
import type { SiteRule } from './generated/SiteRule';
import type { SiteSettings } from './generated/SiteSettings';
import type { TimerSettings } from './generated/TimerSettings';

// How a profile treats each site, and what's set for a site outside any profile, is shared with the
// desktop app, so it's generated from its Rust code (sites.rs).
export type { HiddenPart, PathRule, SiteLevel, SiteRule, SiteSettings };

// So are quotes the user added (quotes.rs), which sync with the app.
export type { OwnQuote };

/** A single on/off switch in a site recipe, e.g. "Hide home feed". */
export interface RecipeToggle {
  id: string;
  label: string;
  /**
   * CSS selectors for elements hidden while the toggle is on. To hide something on some
   * pages only, start the selector with the page path the extension keeps on `<html>`,
   * e.g. `html[data-lock-in-path^="/explore"] main`.
   */
  selectors?: string[];
  /** A whole-page effect applied while the toggle is on. */
  effect?: 'grayscale';
  /** Whether the toggle is on when a profile doesn't override it. */
  default: boolean;
}

/** Defines what "tamed" means for one site. */
export interface SiteRecipe {
  id: string;
  name: string;
  /** Browser match patterns, e.g. "*://*.reddit.com/*". */
  matches: string[];
  toggles: RecipeToggle[];
  /** Pages people often want handled differently, offered as one-click page rules (e.g. YouTube Shorts). */
  pages?: SuggestedPages[];
  /** Where the selectors came from, for attribution. */
  source?: string;
}

/** A named group of paths, e.g. "For You" on TikTok is both "/" and "/foryou". */
export interface SuggestedPages {
  label: string;
  paths: string[];
}

/** A reusable session setup, e.g. "Writing" or "Wind Down". */
export interface Profile {
  id: string;
  name: string;
  sites: SiteRule[];
  /** The color of this profile's stars in the night sky, e.g. "#f5c86b". Missing, it falls back to a default. */
  starColor?: string;
  /** The quiet timer, run by the desktop app. Missing fields fall back to its defaults. */
  timer?: Partial<TimerSettings>;
  /** Desktop apps closed or hidden during sessions, set in the desktop app. */
  apps?: AppRule[];
  /** How redirected sites behave. Missing fields fall back to the defaults. */
  redirect?: {
    gate?: Partial<GateSettings>;
    links?: SavedLink[];
    /** The profile's if-then plan, e.g. "When I want Reddit, I'll write one paragraph first." */
    plan?: string;
  };
}

/** What happens to a site's open tabs once its pass runs out. */
export type PassExpiry = 'nothing' | 'pause' | 'redirect';

/** How long the redirect page waits before the way through opens. */
export type GateWait =
  | { mode: 'fixed'; seconds: number }
  /** Starts short and grows with each attempt at the same site today, up to a cap. */
  | { mode: 'escalating'; startSeconds: number; stepSeconds: number; maxSeconds: number };

export interface GateSettings {
  /** Whether going on to the site needs a typed reason. */
  requireReason: boolean;
  wait: GateWait;
  /** How long a pass to a redirected site lasts. */
  passMinutes: number;
  onPassExpiry: PassExpiry;
}

/**
 * A running wait on the redirect page. It's kept, so leaving the page to work and coming back
 * picks it up again instead of starting over.
 */
export interface PageWait {
  /** The session it belongs to; a wait never carries into another session. */
  sessionId: string;
  /** Epoch milliseconds. */
  startedAt: number;
  seconds: number;
}

/** One of the user's own links, e.g. their draft or reading list. */
export interface SavedLink {
  id: string;
  title: string;
  url: string;
}

/** How a profile handles redirected sites. */
export interface RedirectSettings {
  gate: GateSettings;
  /** Shown on the redirect page as ways back to work. */
  links: SavedLink[];
  /** The if-then plan shown on the redirect page; empty for none. */
  plan: string;
}

export interface Quote {
  text: string;
  author?: string;
  /** Where a bundled quote comes from, so it can be checked. Not shown. */
  source?: string;
}

/** The one-tap answer to "why are you here?" on redirect pages. */
export type WhyReason = 'bored' | 'stuck' | 'tired' | 'curious';

/** Toggle values keyed by recipe id, then toggle id. */
export type ToggleOverrides = Record<string, Record<string, boolean>>;

/** The session currently running, if any. */
export interface Session {
  id: string;
  profileId: string;
  /** Epoch milliseconds. */
  startedAt: number;
  /** The quiet timer, copied from the profile when the desktop app starts or picks up the session. */
  timer?: TimerSettings;
  /**
   * A copy of its profile, taken when it started, whose rules it runs by. Changes made on this
   * computer reach it too; ones the desktop app syncs from other computers don't. Missing from
   * sessions started before there was a copy, which use their profile (`sessionProfile`).
   */
  rules?: Profile;
  /** Toggles changed for this session only. They don't touch the profile unless saved. */
  overrides?: ToggleOverrides;
  /** Redirected sites the user chose to continue to, keyed by recipe id. Each value is when the pass ends (epoch ms). */
  passes?: Record<string, number>;
}

/** One trip to a redirected site, kept for the attempt counter and, later, insights. */
export interface GateVisit {
  id: string;
  recipeId: string;
  /** Epoch milliseconds. */
  at: number;
  /**
   * The page the visit was sent to: "page" for the redirect page, "blocked" for restricted sites.
   * Visits from before the page was one page say "gate", "quote", "link" or "stuck".
   */
  destination?: string;
  /** The user's one-tap answer to "why are you here?". */
  why?: WhyReason;
  /** What the user said they came for, if they typed anything. */
  intention?: string;
  /** Whether they used a pass to continue to the site. */
  continued?: boolean;
}
