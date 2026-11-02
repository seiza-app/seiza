import {
  endLock,
  findRule,
  hasRoomForOwnSite,
  ownSiteDomain,
  pageWaitFor,
  redirectSettingsFor,
  starterProfiles,
  withNewProfile,
  withProfileDuplicated,
  withProfileMoved,
  withProfileRemoved,
  withProfilesRemoved,
  withProfileRenamed,
  withSiteLevel,
  withStarColor,
  withPathRuleRemoved,
  withPathRules,
  withStarterProfilesReset,
  withGateVisit,
  withGateVisitContinued,
  withGateVisitWhy,
  withAllTheTime,
  withAllTheTimeToggle,
  withHiddenPart,
  withHiddenPartRemoved,
  withPass,
  withProfileToggle,
  withSiteRemoved,
  withRedirectSettings,
  sitesLooser,
  withSessionOverridesSaved,
  withSessionToggle,
  type PageWait,
  type HiddenPart,
  type Profile,
  type RedirectSettings,
  type SiteSettings,
  type SiteLevel,
  type SiteRecipe,
  type WhyReason,
  sessionProfile,
  withRulesEdited,
  type Session,
} from '@lock-in/core';

// Every change to sessions and profiles goes through these actions. Today they save to
// extension storage; once the desktop app is connected, they'll forward to it instead,
// and the UI won't need to change.

// Edits read what's saved, change it and save it back. They run one at a time, so a quick
// second edit can't start from something the first hasn't saved yet and undo it.
let edits: Promise<unknown> = Promise.resolve();
function inTurn<T>(edit: () => Promise<T>): Promise<T> {
  const next = edits.then(edit);
  edits = next.catch(() => {});
  return next;
}

/** Saves an edit to the profiles, and to the running session's copy of its own (`withRulesEdited`). */
async function saveEdit(edit: (list: Profile[]) => Profile[], list: Profile[], session: Session | null) {
  const edited = session && withRulesEdited(session, edit, Date.now());
  await storage.setItems([
    { item: profiles, value: edit(list) },
    ...(edited && edited !== session ? [{ item: activeSession, value: edited }] : []),
  ]);
}

function editProfiles(edit: (list: Profile[]) => Profile[]): Promise<void> {
  return inTurn(async () => {
    const [session, list] = await Promise.all([activeSession.getValue(), profiles.getValue()]);
    await saveEdit(edit, list, session);
  });
}

/**
 * Changes a profile's sites. While one of its Hard sessions is running they can only get
 * stricter, as in the desktop app; anything looser is left as it was. Resolves to whether it saved.
 */
function editSites(profileId: string, edit: (list: Profile[]) => Profile[]): Promise<boolean> {
  return inTurn(async () => {
    const [session, list] = await Promise.all([activeSession.getValue(), profiles.getValue()]);
    const sitesOf = (from: Profile[]) => from.find((p) => p.id === profileId)?.sites ?? [];
    const hard = session?.profileId === profileId && endLock(session, Date.now()) === 'hard';
    // The session's own copy too, which a change synced to the desktop app may have left stricter.
    const running = session?.rules ? [session.rules] : [];
    if (hard && [list, running].some((from) => sitesLooser(sitesOf(from), sitesOf(edit(from))))) return false;
    await saveEdit(edit, list, session);
    return true;
  });
}

export async function startSession(profileId: string): Promise<void> {
  await inTurn(async () => {
    const rules = (await profiles.getValue()).find((p) => p.id === profileId);
    await activeSession.setValue({ id: crypto.randomUUID(), profileId, startedAt: Date.now(), rules });
  });
}

/**
 * Ends the session. The popup asks for a pause or typing first if its timer is still running.
 * A Hard session can't end here before its timer is done; the desktop app would only bring it back.
 */
export function endSession(): Promise<void> {
  return inTurn(async () => {
    if (endLock((await activeSession.getValue()) ?? { startedAt: 0 }, Date.now()) === 'hard') return;
    await activeSession.setValue(null);
  });
}

/** Changes a toggle for the running session only. */
export function setSessionToggle(recipe: SiteRecipe, toggleId: string, value: boolean): Promise<void> {
  return inTurn(async () => {
    const [session, profileList] = await Promise.all([activeSession.getValue(), profiles.getValue()]);
    if (!session) return;

    const rule = findRule(sessionProfile(profileList, session), recipe.id);
    await activeSession.setValue(withSessionToggle(session, recipe, rule, toggleId, value));
  });
}

/** Changes a toggle on a profile, for every future session that uses it. */
export async function setProfileToggle(
  profileId: string,
  recipeId: string,
  toggleId: string,
  value: boolean,
): Promise<void> {
  await editProfiles((list) => withProfileToggle(list, profileId, recipeId, toggleId, value));
}

/** Keeps this session's changes to a site by saving them into the session's profile. */
export function saveSessionChanges(recipeId: string): Promise<void> {
  return inTurn(async () => {
    const [session, profileList] = await Promise.all([activeSession.getValue(), profiles.getValue()]);
    if (!session) return;

    const saved = withSessionOverridesSaved(profileList, session, recipeId);
    await storage.setItems([
      { item: profiles, value: saved.profiles },
      { item: activeSession, value: saved.session },
    ]);
  });
}

/** Adds an empty profile and returns its id. */
export async function createProfile(name = 'New profile'): Promise<string> {
  const id = crypto.randomUUID();
  await editProfiles((list) => withNewProfile(list, id, name));
  return id;
}

/** Copies a profile and returns the copy's id. */
export async function duplicateProfile(profileId: string): Promise<string> {
  const id = crypto.randomUUID();
  await editProfiles((list) => withProfileDuplicated(list, profileId, id));
  return id;
}

/** Deletes a profile, unless it's the one the running session uses. */
export async function deleteProfile(profileId: string): Promise<void> {
  if ((await activeSession.getValue())?.profileId === profileId) return;
  await editProfiles((list) => withProfileRemoved(list, profileId));
}

/** Deletes several profiles at once, skipping the one the running session uses. At least one is always kept. */
export async function deleteProfiles(profileIds: string[]): Promise<void> {
  const running = (await activeSession.getValue())?.profileId;
  const ids = profileIds.filter((id) => id !== running);
  await editProfiles((list) => withProfilesRemoved(list, ids));
}

export async function renameProfile(profileId: string, name: string): Promise<void> {
  await editProfiles((list) => withProfileRenamed(list, profileId, name));
}

/** Sets the color of a profile's stars in the desktop app's night sky. */
export async function setStarColor(profileId: string, color: string): Promise<void> {
  await editProfiles((list) => withStarColor(list, profileId, color));
}

/** Moves a profile earlier (-1) or later (1) in the list. */
export async function moveProfile(profileId: string, offset: number): Promise<void> {
  await editProfiles((list) => withProfileMoved(list, profileId, offset));
}

/**
 * Sets whether a profile allows, tames, redirects or restricts a site. A site someone added
 * themselves joins the profile this way, as long as it has room for another.
 */
export function setSiteLevel(profileId: string, recipeId: string, level: SiteLevel): Promise<boolean> {
  return editSites(profileId, (list) => {
    const profile = list.find((p) => p.id === profileId);
    const isNew = !findRule(profile, recipeId);
    if (isNew && ownSiteDomain(recipeId) && (level === 'allowed' || !hasRoomForOwnSite(profile))) return list;
    return withSiteLevel(list, profileId, recipeId, level);
  });
}

/** Takes a site someone added off a profile. */
export function removeSite(profileId: string, recipeId: string): Promise<boolean> {
  return editSites(profileId, (list) => withSiteRemoved(list, profileId, recipeId));
}

/** Adds page rules to a site, or changes the level of ones it has. Invalid paths are skipped. */
export async function setPageLevel(
  profileId: string,
  recipeId: string,
  paths: string[],
  level: SiteLevel,
): Promise<void> {
  await editSites(profileId, (list) => withPathRules(list, profileId, recipeId, paths, level));
}

/** Removes a page rule, so the page follows its site's level again. */
export async function removePageRule(profileId: string, recipeId: string, path: string): Promise<void> {
  await editSites(profileId, (list) => withPathRuleRemoved(list, profileId, recipeId, path));
}

/** Changes what's set for sites outside any profile. */
function editSiteSettings(edit: (list: SiteSettings[]) => SiteSettings[]): Promise<void> {
  return inTurn(async () => siteSettings.setValue(edit(await siteSettings.getValue())));
}

/** Whether a Hard session is running, during which hidden parts can't be shown again. */
async function hardSession(): Promise<boolean> {
  const session = await activeSession.getValue();
  return !!session && endLock(session, Date.now()) === 'hard';
}

/**
 * Saves what the element picker changed on a site: parts picked to hide whenever it's tamed, and
 * ones hidden before to show again. During a Hard session nothing is shown again. Resolves to
 * whether everything asked for was saved.
 */
export async function changeHiddenParts(
  recipeId: string,
  change: { add: HiddenPart[]; remove: string[] },
): Promise<boolean> {
  const remove = (await hardSession()) ? [] : change.remove;
  await editSiteSettings((list) => {
    const shown = remove.reduce((edited, selector) => withHiddenPartRemoved(edited, recipeId, selector), list);
    return change.add.reduce((edited, part) => withHiddenPart(edited, recipeId, part), shown);
  });
  return remove.length === change.remove.length;
}

/** Shows a hidden part of a site's page again. Resolves to false during a Hard session. */
export async function showPart(recipeId: string, selector: string): Promise<boolean> {
  if (await hardSession()) return false;
  await editSiteSettings((list) => withHiddenPartRemoved(list, recipeId, selector));
  return true;
}

/** Tames a site all the time, even with no session running, or stops. It never changes a session. */
export async function setAllTheTime(recipeId: string, on: boolean): Promise<void> {
  await editSiteSettings((list) => withAllTheTime(list, recipeId, on));
}

/** Sets a toggle for a site tamed all the time. */
export async function setAllTheTimeToggle(recipeId: string, toggleId: string, value: boolean): Promise<void> {
  await editSiteSettings((list) => withAllTheTimeToggle(list, recipeId, toggleId, value));
}

/** Restores the starter profiles to how they shipped. The user's own profiles are kept. */
export async function resetStarterProfiles(): Promise<void> {
  await editProfiles((list) => withStarterProfilesReset(list, starterProfiles));
}

/** Changes how a profile handles redirected sites, starting from its saved settings. */
export async function setRedirectSettings(
  profileId: string,
  change: (current: RedirectSettings) => RedirectSettings,
): Promise<void> {
  await editProfiles((list) =>
    withRedirectSettings(list, profileId, change(redirectSettingsFor(list.find((p) => p.id === profileId)))),
  );
}

/** Records a trip to a redirected site and returns its id. */
export async function recordGateVisit(recipeId: string, destination: 'page' | 'blocked'): Promise<string> {
  const visit = { id: crypto.randomUUID(), recipeId, at: Date.now(), destination };
  await inTurn(async () => gateVisits.setValue(withGateVisit(await gateVisits.getValue(), visit)));
  return visit.id;
}

/**
 * Gives the session a pass to a redirected site. Resolves once the browser has stopped
 * redirecting it, so the caller can navigate straight there.
 */
export async function continueToSite(visitId: string, recipeId: string, intention: string): Promise<void> {
  const continued = await inTurn(async () => {
    const [session, profileList, visits] = await Promise.all([
      activeSession.getValue(),
      profiles.getValue(),
      gateVisits.getValue(),
    ]);
    if (!session) return false;

    const { passMinutes } = redirectSettingsFor(sessionProfile(profileList, session)).gate;
    // Going on to the site ends its wait, so the next trip after the pass waits again.
    const { [recipeId]: _ended, ...waits } = await pageWaits.getValue();
    await storage.setItems([
      { item: activeSession, value: withPass(session, recipeId, Date.now(), passMinutes) },
      { item: gateVisits, value: withGateVisitContinued(visits, visitId, intention) },
      { item: pageWaits, value: waits },
    ]);
    return true;
  });
  if (continued) await syncRedirectsNow();
}

/**
 * The redirect page's wait for a trip to a site: the one already running, or a new one of the
 * given length. Waits from other sessions are dropped along the way.
 */
export async function pageWaitForTrip(recipeId: string, sessionId: string, seconds: number): Promise<PageWait> {
  return inTurn(async () => {
    const all = await pageWaits.getValue();
    const wait = pageWaitFor(all[recipeId], sessionId, seconds, Date.now());
    const kept = Object.fromEntries(Object.entries(all).filter(([, w]) => w.sessionId === sessionId));
    await pageWaits.setValue({ ...kept, [recipeId]: wait });
    return wait;
  });
}

/** Saves the one-tap answer to "why are you here?" on a visit. */
export async function setVisitWhy(visitId: string, why: WhyReason | undefined): Promise<void> {
  await inTurn(async () => gateVisits.setValue(withGateVisitWhy(await gateVisits.getValue(), visitId, why)));
}
