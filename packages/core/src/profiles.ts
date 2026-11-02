import type { Profile, Session } from './types';
import starters from './starter-profiles.json' with { type: 'json' };

/**
 * Profiles every new install starts with. They're ordinary, editable profiles that show off
 * what's possible. They live in JSON so the desktop app can read the same list.
 */
export const starterProfiles = starters as Profile[];

/**
 * The profile a session runs by: its own copy, taken when it started, or, for a session from
 * before sessions had one, its profile in the list.
 */
export function sessionProfile(
  profiles: readonly Profile[],
  session: Pick<Session, 'profileId' | 'rules'> | null | undefined,
): Profile | undefined {
  return session ? (session.rules ?? profiles.find((p) => p.id === session.profileId)) : undefined;
}
