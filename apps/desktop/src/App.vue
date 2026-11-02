<script setup lang="ts">
import type {
  AppRule,
  Constellation,
  GateVisitRecord,
  SatelliteAction,
  SatelliteState,
  SessionRecord,
  Settings,
  SiteRule,
  SkyState,
  TimerSettings,
} from '@lock-in/core';
import {
  addWeeks,
  comets,
  constellationNames,
  constellations,
  dayStart,
  quoteForDay,
  formatDuration,
  glowFor,
  hotkeyLabel,
  lifetimeFocusMs,
  milkyWay,
  planets,
  rareEvents,
  sessionSoFar,
  skyClarity,
  starColorFor,
  stardust,
  starMagnitude,
  stars,
  starSparkle,
  streaks,
  weekMonth,
  weekStart,
} from '@lock-in/core';
import { computed, nextTick, onMounted, onUnmounted, ref, watch, watchEffect } from 'vue';
import {
  actOnSatellite,
  addOwnQuote,
  applyGlow,
  addSampleData,
  answerGoal,
  constellationNames as savedNames,
  endSession,
  firstDayOfWeek,
  forgetBrowser,
  launchSatellite,
  mac,
  markTourOffered,
  nameConstellation,
  removeOwnQuote,
  removeSampleData,
  sessionLog,
  setProfileApps,
  setProfileAsksGoal,
  setProfilePlan,
  setProfileSites,
  setProfileStarColor,
  setProfileTimer,
  setSettings,
  skySeen,
  skyState,
  spendEmergencyPass,
  startSession,
  turnedBack,
  useNow,
  useSnapshot,
} from './app';
import { quotes } from '@lock-in/quotes';
import NightSky, {
  type SkyComet,
  type SkyConstellation,
  type SkyEvent,
  type SkyPlanet,
  type SkySatellite,
  type SkyStar,
} from './components/NightSky.vue';
import ProfileList from './components/ProfileList.vue';
import SeizaWordmark from './components/SeizaWordmark.vue';
import SettingsView from './components/SettingsView.vue';
import SkyPage from './components/SkyPage.vue';
import TourGuide, { type TourStep } from './components/TourGuide.vue';
import WeekReport from './components/WeekReport.vue';

const snapshot = useSnapshot();
const error = ref('');
// The Sky is home (PLAN §5). Profiles only edits them; sessions start from the Sky.
type View = 'sky' | 'profiles' | 'week' | 'settings';
const view = ref<View>('sky');
// Explore clears everything off the Sky page, so the sky is on its own.
const exploring = ref(false);
const nightSky = ref<InstanceType<typeof NightSky>>();
// What Explore's readouts show: how far in it's zoomed and the constellation in the crosshair.
const skyView = ref<{ zoom: number; aimed?: string }>({ zoom: 1 });
watch(view, (to) => to !== 'sky' && (exploring.value = false));
// The week the report is showing, so the sky can line its constellations up behind it.
const reportWeek = ref<number>();
watch(view, (to) => to !== 'week' && (reportWeek.value = undefined));
const tabs: { view: View; label: string }[] = [
  { view: 'sky', label: 'Sky' },
  { view: 'week', label: 'Your week' },
  { view: 'profiles', label: 'Profiles' },
  { view: 'settings', label: 'Settings' },
];
// Pages slide in from the side their tab is on.
const slide = ref<'slide-forward' | 'slide-back'>('slide-forward');
const tabIndex = (of: View) => tabs.findIndex((tab) => tab.view === of);
watch(view, (to, from) => (slide.value = tabIndex(to) >= tabIndex(from) ? 'slide-forward' : 'slide-back'));

// A glowing dot that glides to the selected tab.
const tabBar = ref<HTMLElement>();
const dotLeft = ref(0);
function placeDot() {
  const selected = tabBar.value?.querySelector<HTMLElement>('[aria-current="page"]');
  if (selected) dotLeft.value = selected.offsetLeft;
}
watch(view, () => void nextTick(placeDot));
const tabBarResized = new ResizeObserver(placeDot);
onMounted(() => tabBar.value && tabBarResized.observe(tabBar.value));
onUnmounted(() => tabBarResized.disconnect());
// The report waits between sessions, so starting one goes back to the Sky.
watch(
  () => snapshot.value?.session?.id,
  (id) => {
    if (id && view.value === 'week') view.value = 'sky';
  },
);

// The night sky, worked out from the session log plus the running session, whose star forms
// as you focus.
const log = ref<SessionRecord[]>([]);
const now = useNow();
const minute = computed(() => Math.floor(now.value / 60_000));
const loadLog = async () => void (log.value = await sessionLog());
onMounted(() => run(loadLog));
// A session that just ended is in the log now, with any trips to sites during it.
watch(
  () => snapshot.value?.session?.id,
  (id, previous) => {
    if (previous && !id) void run(loadLog).then(() => run(loadSky));
  },
);
const sessions = computed(() => {
  const running = snapshot.value?.session;
  return running ? [...log.value, sessionSoFar(running, minute.value * 60_000)] : log.value;
});
// Weeks start on the computer's first day of the week, as in the weekly report. Nothing is shown
// until it's known, so constellations don't jump from one grouping to another.
const firstDay = ref<number>();
onMounted(
  () =>
    void firstDayOfWeek().then(
      (day) => (firstDay.value = day),
      () => (firstDay.value = 1),
    ),
);
const names = ref(new Map<string, string>());
const loadNames = async () => {
  names.value = new Map((await savedNames()).map((saved) => [saved.id, saved.name]));
};
onMounted(() => run(loadNames));
const skyStars = computed(() => stars(sessions.value));
// While a session goes on this is worked out again every minute, so weeks that haven't changed
// are handed back as they were, along with their stars as drawn (until a star color changes).
let lastWeeks: Constellation[] = [];
const weeks = computed(() =>
  firstDay.value === undefined ? [] : (lastWeeks = constellations(skyStars.value, firstDay.value, lastWeeks)),
);
let drawnStars = { colors: '', of: new WeakMap<Constellation, SkyStar[]>() };
const skyConstellations = computed<(SkyConstellation & { named: boolean })[]>(() => {
  const colors = new Map(snapshot.value?.profiles.map((p) => [p.id, p.starColor]));
  const colorsKey = JSON.stringify([...colors]);
  if (colorsKey !== drawnStars.colors) drawnStars = { colors: colorsKey, of: new WeakMap() };
  const named = constellationNames(weeks.value, names.value);
  return weeks.value.map((constellation, i) => {
    let drawn = drawnStars.of.get(constellation);
    if (!drawn) {
      drawn = constellation.stars.map((star, j) => ({
        id: star.id,
        ...constellation.points[j]!,
        color: starColorFor(star.profileId, colors.get(star.profileId)),
        magnitude: starMagnitude(star),
        sparkle: starSparkle(star),
      }));
      drawnStars.of.set(constellation, drawn);
    }
    return {
      id: constellation.id,
      name: named[i]!,
      named: names.value.has(constellation.id),
      weekStart: constellation.weekStart,
      month: weekMonth(constellation.weekStart),
      lines: constellation.lines,
      stars: drawn,
    };
  });
});
// Comets and rare events, worked out from the log, trips that went back to focus and this
// device's secret.
const sky = ref<SkyState>();
const resisted = ref<GateVisitRecord[]>([]);
const loadSky = async () => {
  // When the Sky last showed new things is kept here as it changes, so it's only read once.
  sky.value ??= await skyState();
  resisted.value = await turnedBack();
};
onMounted(() => run(loadSky));
const rare = computed(() => (sky.value ? rareEvents(log.value, sky.value.secret) : []));
const allComets = computed(() => (sky.value ? comets(resisted.value, sky.value.secret) : []));
// The Sky shows a month's weeks at a time: the latest, unless you step back to an earlier one.
// It's back on the latest whenever you leave the Sky or start a session.
const skyMonths = computed(() => [...new Set(skyConstellations.value.map((c) => c.month))]);
const steppedMonth = ref<number>();
const skyMonth = computed(() => (skyMonths.value.includes(steppedMonth.value!) ? steppedMonth.value : undefined));
watch(view, (to) => to !== 'sky' && (steppedMonth.value = undefined));
watch(
  () => snapshot.value?.session?.id,
  (id) => id && (steppedMonth.value = undefined),
);
const constellationOfWeek = computed(() => new Map(skyConstellations.value.map((c) => [c.weekStart, c.id])));
const weekConstellation = (at: number) =>
  firstDay.value === undefined ? undefined : constellationOfWeek.value.get(weekStart(at, firstDay.value));
// By when its session started, like its star, though it's dated by when the session ended.
const skyEvents = computed<SkyEvent[]>(() => {
  const started = new Map(log.value.map((session) => [session.id, session.startedAt]));
  return rare.value.map((event) => ({
    ...event,
    constellationId: weekConstellation(started.get(event.id) ?? event.at),
  }));
});
/** The most comets a constellation shows, the latest. */
const cometsPerWeek = 3;
const thisWeek = computed(() => (firstDay.value === undefined ? 0 : weekStart(minute.value * 60_000, firstDay.value)));
// The Sky shows this week's comets (or a month's, stepped back to one); exploring and the
// weekly report show each week's.
const skyComets = computed<SkyComet[]>(() => {
  const byWeek = new Map<string, SkyComet[]>();
  for (const comet of allComets.value) {
    const constellationId = weekConstellation(comet.at);
    if (!constellationId) continue;
    byWeek.set(constellationId, [...(byWeek.get(constellationId) ?? []), { ...comet, constellationId }]);
  }
  const all = [...byWeek.values()].flatMap((list) => list.slice(-cometsPerWeek));
  if (exploring.value || reportWeek.value !== undefined) return all;
  const month = skyMonth.value;
  if (month === undefined) return all.filter((comet) => comet.at >= thisWeek.value);
  const months = new Map(skyConstellations.value.map((c) => [c.id, c.month]));
  return all.filter((comet) => months.get(comet.constellationId) === month);
});
// Satellites circle the sky: in orbit, or back and waiting to be sorted, the latest launched. One
// that goes onto the task list leaves orbit: it stays until it has drifted away, the next time
// the Sky shows, as tasks belong to the list (and one day to Reminders or Todoist), not the sky.
const mostSatellites = 20;
const playNow = ref<string[]>([]);
const skySatellites = computed<SkySatellite[]>(() => {
  const seenAt = sky.value?.seenAt;
  const leaving = (id: string, stateAt: number) =>
    seenAt !== undefined && (stateAt > seenAt || playNow.value.includes(`satellite:${id}`));
  return (snapshot.value?.satellites ?? [])
    .filter((s) => ['orbiting', 'back'].includes(s.state) || (s.state === 'task' && leaving(s.id, s.stateAt)))
    .sort((a, b) => a.launchedAt - b.launchedAt)
    .slice(-mostSatellites)
    .map((s) => ({ id: s.id, text: s.text, tasked: s.state === 'task' }));
});
// Each plays once, the next time the Sky is shown between sessions, then counts as seen.
const visible = ref(document.visibilityState === 'visible');
const onVisibility = () => (visible.value = document.visibilityState === 'visible');
onMounted(() => document.addEventListener('visibilitychange', onVisibility));
onUnmounted(() => document.removeEventListener('visibilitychange', onVisibility));
const unseen = computed(() => {
  const seenAt = sky.value?.seenAt;
  if (seenAt === undefined || view.value !== 'sky' || !visible.value || snapshot.value?.session) return [];
  return [
    ...skyEvents.value.map((event) => ({ key: `event:${event.id}`, at: event.at })),
    ...skyComets.value.map((comet) => ({ key: `comet:${comet.id}`, at: comet.at })),
    ...(snapshot.value?.satellites ?? [])
      .filter((s) => s.state === 'task' && skySatellites.value.some((shown) => shown.id === s.id))
      .map((s) => ({ key: `satellite:${s.id}`, at: s.stateAt })),
  ]
    .filter((item) => item.at > seenAt)
    .sort((a, b) => a.at - b.at);
});
watch(unseen, (items) => {
  const latest = items[items.length - 1]?.at;
  if (latest === undefined || !sky.value) return;
  // A few at most, the latest. What played before stays, so a satellite drifting away isn't cut short.
  playNow.value = [...playNow.value, ...items.slice(-4).map((item) => item.key)];
  sky.value = { ...sky.value, seenAt: latest };
  void run(() => skySeen(latest));
});

const monthName = new Intl.DateTimeFormat(undefined, { month: 'long', year: 'numeric' });
const skyPlanets = computed<SkyPlanet[]>(() => {
  const colors = new Map(snapshot.value?.profiles.map((p) => [p.id, p.starColor]));
  return planets(skyStars.value, rare.value).map((planet) => ({
    ...planet,
    label: `${monthName.format(planet.monthStart)} · ${formatDuration(planet.focusMs)}`,
    tint: starColorFor(planet.topProfileId, colors.get(planet.topProfileId)),
  }));
});
// How many stars each profile has, for the Profiles page.
const profileStars = computed(() => {
  const counts: Record<string, number> = {};
  for (const star of skyStars.value) counts[star.profileId] = (counts[star.profileId] ?? 0) + 1;
  return counts;
});
const aimedConstellation = computed(() => skyConstellations.value.find((c) => c.id === skyView.value.aimed));
const starCount = computed(() => skyConstellations.value.reduce((count, c) => count + c.stars.length, 0));
const reportConstellation = computed(() => skyConstellations.value.find((c) => c.weekStart === reportWeek.value));
// Streaks by the end of the week the report shows. Only this week's has one running.
const reportStreaks = computed(() => {
  if (reportWeek.value === undefined) return { current: 0, longest: 0 };
  const end = addWeeks(reportWeek.value, 1);
  const at = Math.min(minute.value * 60_000, end - 1);
  const counted = streaks(skyStars.value, at);
  return at < end - 1 ? counted : { ...counted, current: 0 };
});
const rename = (id: string, name: string) =>
  run(async () => {
    await nameConstellation(id, name);
    await loadNames();
  });
// At the top of the Sky: the date, a quote for the day (the same one all day), the time locked in
// today (counting the session going on, and time past the timer) and the streak running now. A
// session counts on the day it started, as in the weekly report.
const dateFormat = new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
const glance = computed(() => {
  const at = minute.value * 60_000;
  const today = dayStart(at);
  const lockedInMs = sessions.value
    .filter((session) => session.startedAt >= today)
    .reduce((sum, session) => sum + session.focusedMs + session.overtimeMs, 0);
  const quote = quoteForDay(quotes, snapshot.value?.ownQuotes ?? [], String(today));
  return { date: dateFormat.format(at), quote, lockedInMs, streak: streaks(skyStars.value, at).current };
});
const totalStardust = computed(() => stardust(sessions.value));
const milkyWayFill = computed(() => milkyWay(sessions.value));
// The glow color picked in Settings, if lifetime focus has unlocked it, for the frames' corner
// ticks and the tabs' dot.
const lifetimeFocus = computed(() => lifetimeFocusMs(sessions.value));
const glow = computed(() => glowFor(snapshot.value?.settings.glow ?? '', lifetimeFocus.value));
watchEffect(() => applyGlow(glow.value));
// Exploring shows it clear; otherwise it's hazy between sessions.
const clarity = computed(() =>
  exploring.value ? 1 : skyClarity(now.value, !!snapshot.value?.session, log.value[log.value.length - 1]),
);

const stopExploring = (event: KeyboardEvent) => {
  if (event.key === 'Escape') exploring.value = false;
};
onMounted(() => window.addEventListener('keydown', stopExploring));
onUnmounted(() => window.removeEventListener('keydown', stopExploring));

const sampleData = (action: 'add' | 'remove') =>
  run(async () => {
    await (action === 'add' ? addSampleData() : removeSampleData());
    await loadLog();
    await loadNames();
    await loadSky();
  });

// A backup brings in more of the log, and maybe the sky's secret (on a computer with no sky of its
// own yet), so the sky is read again.
const imported = () =>
  run(async () => {
    sky.value = undefined;
    await loadLog();
    await loadNames();
    await loadSky();
  });

const inState = (state: SatelliteState) => snapshot.value?.satellites.filter((s) => s.state === state) ?? [];
const satellites = computed(() => ({
  back: inState('back'),
  orbiting: inState('orbiting'),
  tasks: inState('task'),
  // The latest first. The core only sends tasks ticked off in the last day.
  done: inState('done').sort((a, b) => b.stateAt - a.stateAt),
}));
const launch = (text: string) => run(async () => void (await launchSatellite(text)));

// The tour for new users (PLAN Phase 5): the Sky offers it once, and Settings has it after that.
// Each step goes to the page it's about.
const touring = ref<number>();
const tourSteps = computed<(TourStep & { view: View })[]>(() => {
  const hotkey = snapshot.value?.settings.satelliteHotkey;
  return [
    {
      view: 'sky',
      target: 'sky',
      title: 'The Sky',
      text: "This is home. Each focus session adds a star to your sky, a week's stars join up into a constellation, and every focused minute earns stardust.",
    },
    {
      view: 'sky',
      target: 'start',
      title: 'Start a session',
      text: 'Pick a profile to lock in. Its timer floats here while it runs, and your first star comes after 5 minutes of focus.',
    },
    {
      view: 'profiles',
      target: 'profiles',
      title: 'Profiles',
      text: 'Each profile has its own timer, how strict it is (Gentle, Medium or Hard), apps to close or hide, and a star color. The sites it quiets are set in the browser extension: each one Allowed, Tamed, Redirected or Restricted.',
    },
    {
      view: 'sky',
      target: 'satellites',
      title: 'Satellites',
      text: `When a thought pulls at you, launch it here${hotkey ? ` (or with ${hotkeyLabel(hotkey, mac)} from any app)` : ''} and keep going. It waits in orbit while you focus, then comes back later to turn into a task or leave for later.`,
    },
    {
      view: 'settings',
      target: 'browsers',
      title: 'Your browser',
      text: 'Add the Seiza extension to your browser and it links to this app by itself, showing up here. Redirected sites then show a calm Seiza page instead, with a few minutes through if you need them. The tour is in Settings whenever you want it again.',
    },
  ];
});
watch(touring, (at) => {
  if (at === undefined) return;
  exploring.value = false;
  view.value = tourSteps.value[at]!.view;
});
function takeTour(take = true) {
  if (take) touring.value = 0;
  if (!snapshot.value?.tourOffered) void run(markTourOffered);
}
const act = (id: string, action: SatelliteAction) => run(() => actOnSatellite(id, action));

async function run(action: () => Promise<void>) {
  error.value = '';
  try {
    await action();
  } catch (reason) {
    error.value = `Something went wrong: ${String(reason)}`;
  }
}
</script>

<template>
  <NightSky
    ref="nightSky"
    :constellations="skyConstellations"
    :month="skyMonth"
    :planets="skyPlanets"
    :milky-way="milkyWayFill"
    :clarity="clarity"
    :drift="exploring"
    :names="view === 'sky' && exploring"
    :week="reportWeek"
    :comets="skyComets"
    :events="skyEvents"
    :satellites="skySatellites"
    :satellite-text="view === 'sky' && exploring && !snapshot?.session"
    :battery-saver="snapshot?.settings.batterySaver"
    :play="playNow"
    @view="(seen: { zoom: number; aimed?: string }) => (skyView = seen)"
  />
  <!-- The Sky fits the window, so its task list scrolls inside rather than pushing the page. -->
  <main :class="{ fitted: view === 'sky' }">
    <!-- Explore clears the header away too, so the sky is on its own. -->
    <header :class="{ away: exploring }" :inert="exploring">
      <div class="row">
        <h1><SeizaWordmark /></h1>
        <span class="mono" title="Each session adds a star, and every focused minute earns stardust" data-tour="sky">
          Stars <b>{{ String(starCount).padStart(3, '0') }}</b> · Stardust <b>{{ totalStardust.toLocaleString() }}</b>
        </span>
      </div>
      <nav ref="tabBar" class="tabs" aria-label="Pages">
        <span class="here" :style="{ translate: `${dotLeft}px 0` }" aria-hidden="true" />
        <!-- The report waits between sessions. -->
        <button
          v-for="tab in tabs"
          :key="tab.view"
          class="tab mono"
          type="button"
          :aria-current="view === tab.view ? 'page' : undefined"
          :disabled="tab.view === 'week' && !!snapshot?.session"
          :title="tab.view === 'week' && snapshot?.session ? 'Your week opens again when this session ends' : undefined"
          @click="view = tab.view"
        >
          {{ tab.label }}
        </button>
      </nav>
    </header>

    <Transition :name="slide" mode="out-in">
      <div v-if="snapshot && firstDay !== undefined" :key="view" class="page">
        <SkyPage
          v-if="view === 'sky'"
          v-model:exploring="exploring"
          :profiles="snapshot.profiles"
          :session="snapshot.session"
          :goal-to-answer="snapshot.goalToAnswer"
          :passes-left="snapshot.passesLeft"
          :satellites="satellites"
          :satellite-hotkey="snapshot.settings.satelliteHotkey"
          :offer-tour="!snapshot.tourOffered && touring === undefined"
          :star-count="starCount"
          :glance="glance"
          :stardust="totalStardust"
          :week-count="skyConstellations.length"
          :planet-count="skyPlanets.length"
          :zoom="skyView.zoom"
          :aimed="aimedConstellation"
          :this-week="thisWeek"
          :months="skyMonths"
          :month="skyMonth"
          @month="(month: number | undefined) => (steppedMonth = month)"
          @start="(id: string, goal?: string) => run(() => startSession(id, goal))"
          @answer="(id: string, met: boolean) => run(() => answerGoal(id, met))"
          @end="run(endSession)"
          @pass="run(spendEmergencyPass)"
          @launch="launch"
          @act="act"
          @zoom="(factor: number) => nightSky?.zoom(factor)"
          @recenter="nightSky?.recenter()"
          @rename="rename"
          @tour="takeTour"
        />
        <ProfileList
          v-else-if="view === 'profiles'"
          :profiles="snapshot.profiles"
          :can-hide-apps="snapshot.canHideApps"
          :star-counts="profileStars"
          @set-ask-goal="(id: string, on: boolean) => run(() => setProfileAsksGoal(id, on))"
          @set-timer="(id: string, timer: TimerSettings) => run(() => setProfileTimer(id, timer))"
          @set-apps="(id: string, apps: AppRule[]) => run(() => setProfileApps(id, apps))"
          @set-sites="(id: string, sites: SiteRule[]) => run(() => setProfileSites(id, sites))"
          @set-plan="(id: string, plan: string) => run(() => setProfilePlan(id, plan))"
          @set-star-color="(id: string, color: string) => run(() => setProfileStarColor(id, color))"
        />
        <WeekReport
          v-else-if="view === 'week'"
          :first-day="firstDay"
          :constellation="reportConstellation"
          :events="rare"
          :comets="allComets"
          :streaks="reportStreaks"
          :profiles="snapshot.profiles"
          @week="(week: number) => (reportWeek = week)"
          @rename="rename"
          @set-plan="(id: string, plan: string) => run(() => setProfilePlan(id, plan))"
        />
        <SettingsView
          v-else
          :settings="snapshot.settings"
          :passes-left="snapshot.passesLeft"
          :browsers="snapshot.browsers"
          :session-running="!!snapshot.session"
          :lifetime-focus-ms="lifetimeFocus"
          :own-quotes="snapshot.ownQuotes"
          @add-quote="(text: string, author: string) => run(() => addOwnQuote(text, author))"
          @remove-quote="(id: string) => run(() => removeOwnQuote(id))"
          @update:settings="(settings: Settings) => run(() => setSettings(settings))"
          @forget-browser="(path: string) => run(() => forgetBrowser(path))"
          @sample-data="sampleData"
          @imported="imported"
          @tour="takeTour()"
          @close="view = 'sky'"
        />
      </div>
    </Transition>

    <p v-if="error" class="error" role="alert">{{ error }}</p>
  </main>
  <TourGuide v-if="touring !== undefined" v-model:step="touring" :steps="tourSteps" @end="touring = undefined" />
</template>

<style scoped>
main {
  position: relative;
  z-index: 1;
  max-width: 480px;
  min-height: 100vh;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  margin: 0 auto;
  padding: 12px 16px 14px;
}

header {
  margin-bottom: 10px;
  transition:
    opacity 700ms,
    filter 700ms;
}

header.away {
  opacity: 0;
  filter: blur(6px);
}

.row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}

h1 {
  margin: 0;
  font-size: 22px;
}

.tabs {
  position: relative;
  display: flex;
  gap: 14px;
  margin-top: 9px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--line);
}

.tab {
  padding: 0 0 0 10px;
  border-radius: 0;
  background: none;
  transition:
    color 400ms,
    opacity 400ms;
}

.tab:hover,
.tab[aria-current='page'] {
  color: var(--starlight);
}

.tab:disabled {
  opacity: 0.35;
  cursor: default;
}

/* The current page's glowing dot, which glides to each tab picked. */
.here {
  position: absolute;
  top: 6px;
  left: 0;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--dot);
  box-shadow: 0 0 6px var(--dot);
  transition: translate 500ms var(--ease);
}

main.fitted {
  height: 100vh;
}

.page {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.slide-forward-enter-active,
.slide-back-enter-active {
  transition:
    opacity 380ms ease,
    transform 480ms cubic-bezier(0.2, 0.8, 0.2, 1),
    filter 380ms ease;
}

.slide-forward-leave-active,
.slide-back-leave-active {
  transition:
    opacity 200ms ease,
    transform 220ms ease-in,
    filter 200ms ease;
}

.slide-forward-enter-from,
.slide-back-leave-to {
  opacity: 0;
  transform: translateX(28px);
  filter: blur(3px);
}

.slide-forward-leave-to,
.slide-back-enter-from {
  opacity: 0;
  transform: translateX(-28px);
  filter: blur(3px);
}

@media (prefers-reduced-motion: reduce) {
  .slide-forward-enter-from,
  .slide-forward-leave-to,
  .slide-back-enter-from,
  .slide-back-leave-to {
    transform: none;
    filter: none;
  }

  .here {
    transition: none;
  }

  header.away {
    filter: none;
  }
}

.error {
  color: #f0a3a3;
  font-size: 12px;
}
</style>
