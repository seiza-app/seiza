// Shared types, schemas, and logic for Seiza (profiles, recipes, sessions, rewards).
export type * from './types';
// Types shared with the desktop app, generated from its Rust code.
export type * from './generated/ActiveSession';
export type * from './generated/Activity';
export type * from './generated/AppAction';
export type * from './generated/AppRule';
export type * from './generated/BlockerStatus';
export type * from './generated/ConstellationName';
export type * from './generated/EndReason';
export type * from './generated/GateVisitRecord';
export type * from './generated/ImportReport';
export type * from './generated/InstalledApp';
export type * from './generated/KnownBrowser';
export type * from './generated/PairingState';
export type * from './generated/ProfileSummary';
export type * from './generated/Satellite';
export type * from './generated/SatelliteAction';
export type * from './generated/SatelliteRecord';
export type * from './generated/SatelliteState';
export type * from './generated/SavedBackup';
export type * from './generated/SessionGoal';
export type * from './generated/SessionRecord';
export type * from './generated/Settings';
export type * from './generated/SkyState';
export type * from './generated/Snapshot';
export type * from './generated/Strictness';
export type * from './generated/SyncDevice';
export type * from './generated/SyncStatus';
export type * from './generated/TimerEnd';
export type * from './generated/TimerSettings';
export type * from './generated/UpdateStatus';
export { backupDay, importSummary, maxBackupBytes } from './backup';
export { matchesPattern, matchPatternToRegex, parseHttpUrl } from './match';
export {
  allTheTimeToggles,
  buildRecipeCss,
  cssForPage,
  findRecipe,
  isSafeSelector,
  pagePathAttribute,
  resolveToggles,
} from './recipe';
export {
  maxHiddenLabelLength,
  maxHiddenParts,
  maxSelectorLength,
  siteSettingsFor,
  withAllTheTime,
  withAllTheTimeToggle,
  withHiddenPart,
  withHiddenPartRemoved,
} from './siteSettings';
export {
  findRule,
  maxProfileNameLength,
  withNewProfile,
  withProfileDuplicated,
  withProfileMoved,
  withProfileRemoved,
  withProfilesRemoved,
  withProfileRenamed,
  withPathRuleRemoved,
  withPathRules,
  withProfileToggle,
  withRedirectSettings,
  withSessionOverridesSaved,
  withSessionToggle,
  withRulesEdited,
  withSiteLevel,
  withStarColor,
  withStarterProfilesReset,
} from './edits';
export {
  attemptsToday,
  checkOpenPage,
  hasPass,
  maxIntentionLength,
  nextPassExpiry,
  redirectRules,
  siteAccess,
  withGateVisit,
  withGateVisitContinued,
  withGateVisitWhy,
  withPass,
  type OpenPageCheck,
  type RedirectRule,
} from './redirect';
export {
  findPathRule,
  levelFor,
  maxPathLength,
  maxPathRules,
  normalizePath,
  pathMatches,
  pathRegexSource,
} from './paths';
export {
  defaultRedirectSettings,
  defaultWaits,
  maxGoalLength,
  maxPlanLength,
  pageWaitFor,
  redirectSettingsFor,
  waitLabel,
  waitSecondsFor,
  waitSecondsLeft,
} from './settings';
export {
  maxOwnQuotes,
  maxQuoteAuthorLength,
  maxQuoteLength,
  pickQuote,
  quoteForDay,
  withOwnQuote,
  withOwnQuoteRemoved,
} from './quotes';
export { sessionProfile, starterProfiles } from './profiles';
export {
  hasRoomForOwnSite,
  isLooser,
  maxOwnSites,
  ownSiteDomain,
  ownSiteId,
  ownSitePattern,
  ownSiteRecipe,
  siteDomain,
  siteFor,
  siteLevels,
  siteName,
  siteRecipes,
  sitesLooser,
  withSiteRemoved,
} from './sites';
export { glowColors, glowFor, nextGlow, unlockedGlows, type GlowColor } from './glow';
export { hotkeyFromPress, hotkeyLabel, type KeyPress } from './hotkey';
export {
  isStarColor,
  lifetimeFocusMs,
  milkyWay,
  newStarColor,
  seeded,
  sessionSoFar,
  skyClarity,
  smallestStarMinutes,
  starColorFor,
  starColors,
  stardust,
  starMagnitude,
  starPalette,
  stars,
  starSparkle,
  type Star,
} from './sky';
export {
  cleanedConstellationName,
  constellationNames,
  constellations,
  maxConstellationNameLength,
  skyPlaces,
  weekMonth,
  type Constellation,
} from './constellations';
export {
  oddShapes,
  planetKinds,
  planets,
  type Hsl,
  type Moon,
  type Planet,
  type PlanetEvent,
  type PlanetForm,
  type PlanetKind,
  type PlanetRing,
  type PlanetShape,
  type RingStyle,
  type SurfaceFeature,
} from './planets';
export {
  comets,
  planetEventKinds,
  rareEvents,
  type Comet,
  type PlanetEventKind,
  type RareEvent,
  type RareEventKind,
  type SkyEventKind,
} from './events';
export { dayStart, starDays, streaks, type StarDay, type Streaks } from './streaks';
export { endLock, formatClock, timerStatus, unlockText, type TimerStatus } from './timer';
export { maxSatelliteLength } from './satellites';
export {
  addWeeks,
  formatDuration,
  formatHour,
  formatHourRange,
  weeklyReport,
  weekStart,
  type DistractionSummary,
  type FocusSummary,
  type PlanIdea,
  type SatelliteSummary,
  type SiteDistractions,
  type WeeklyReport,
} from './insights';
export { desktopHostName, linkProtocol, type FromDesktop, type Synced, type ToDesktop } from './link';
