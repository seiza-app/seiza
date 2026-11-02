<script setup lang="ts">
import {
  addWeeks,
  formatDuration,
  formatHour,
  formatHourRange,
  maxConstellationNameLength,
  maxPlanLength,
  siteName as nameOfSite,
  starColorFor,
  weeklyReport,
  weekStart,
  type Comet,
  type ProfileSummary,
  type RareEvent,
  type SiteDistractions,
  type Streaks,
  type WeeklyReport,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';
import { computed, nextTick, ref, watch } from 'vue';
import { activity } from '../app';
import Unfold from './Unfold.vue';
import WeekPicker from './WeekPicker.vue';

// The weekly report: focus, distractions and satellites for one calendar week, worked out on
// this device from what the app logged, as one long numbered readout (the Observatory HUD, PLAN
// Phase 5). It opens on this week; the arrows go back to earlier
// ones, as far as the log goes. Weeks start on the first day of the week set on the computer.
// The week's constellation shows softly behind the header (drawn by the sky), named beneath it.
// Anything rare that happened in the sky that week is noted quietly at the end. Streaks are only
// ever counted up: the one running now, while it is, and the longest by the week's end.

const props = defineProps<{
  /** The first day of the week, 0 being Sunday. */
  firstDay: number;
  /** The constellation of the week being shown, if it has one. */
  constellation?: { id: string; name: string; named: boolean };
  /** Every rare event and comet, oldest first. */
  events: RareEvent[];
  comets: Comet[];
  /** Days in a row with a star by the end of the week being shown, `current` only for this week. */
  streaks: Streaks;
  /** For each profile's star color. Deleted profiles get theirs from their id. */
  profiles: ProfileSummary[];
}>();
const emit = defineEmits<{
  /** The start of the week being shown, whenever it changes. */
  week: [start: number];
  rename: [id: string, name: string];
  'set-plan': [profileId: string, plan: string];
}>();

const thisWeek = computed(() => weekStart(Date.now(), props.firstDay));
const start = ref(thisWeek.value);
const report = ref<WeeklyReport>();
const firstLoggedAt = ref<number | null>(null);
const error = ref('');
// Whether the calendar for jumping to a week is open.
const picking = ref(false);
const slide = ref<'week-forward' | 'week-back'>('week-forward');
watch(start, (to, from) => (slide.value = to > from ? 'week-forward' : 'week-back'));

const siteName = (recipeId: string) => nameOfSite(recipes, recipeId);

watch(start, (week) => emit('week', week), { immediate: true });

watch(
  start,
  async (week) => {
    error.value = '';
    try {
      // The week before too, for the trends.
      const logged = await activity(addWeeks(week, -1), addWeeks(week, 1));
      // Skip it if the arrows moved on while it loaded.
      if (week !== start.value) return;
      firstLoggedAt.value = logged.firstLoggedAt;
      report.value = weeklyReport(logged, week, siteName);
    } catch (reason) {
      error.value = `Couldn't load the week: ${String(reason)}`;
    }
  },
  { immediate: true },
);

// Renaming the week's constellation, in place.
const renaming = ref(false);
const draft = ref('');
const nameInput = ref<HTMLInputElement>();
async function startRenaming() {
  draft.value = props.constellation?.named ? props.constellation.name : '';
  renaming.value = true;
  await nextTick();
  nameInput.value?.focus();
}
function saveName() {
  if (!renaming.value) return;
  renaming.value = false;
  const constellation = props.constellation;
  if (constellation && draft.value.trim() !== (constellation.named ? constellation.name : '')) {
    emit('rename', constellation.id, draft.value);
  }
}
watch(start, () => (renaming.value = false));

// The notes offer the distraction tip as an if-then plan, drafted from it, to change here and save
// as that profile's plan (PLAN §12). Only while the profile has none, and only in the report.
const planDraft = ref('');
const planSaved = ref(false);
const planProfile = computed(() => props.profiles.find((p) => p.id === report.value?.planIdea?.profileId));
watch(
  () => report.value?.planIdea,
  (idea) => {
    planDraft.value = idea?.plan ?? '';
    planSaved.value = false;
  },
);
function savePlan() {
  const profile = planProfile.value;
  if (!profile || !planDraft.value.trim()) return;
  planSaved.value = true;
  emit('set-plan', profile.id, planDraft.value.trim());
}

const firstWeek = computed(() =>
  firstLoggedAt.value === null ? thisWeek.value : weekStart(firstLoggedAt.value, props.firstDay),
);
// No trends against a week before anything was logged.
const hasLastWeek = computed(() => start.value > firstWeek.value);

const dateFormat = new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short' });
const range = computed(() => `${dateFormat.format(start.value)} – ${dateFormat.format(addWeeks(start.value, 1) - 1)}`);
const title = computed(() => {
  if (start.value === thisWeek.value) return 'This week';
  if (start.value === addWeeks(thisWeek.value, -1)) return 'Last week';
  return range.value;
});

const days = computed(() =>
  Array.from({ length: 7 }, (_, i) => {
    const day = new Date(start.value);
    day.setDate(day.getDate() + i);
    return {
      at: day.getTime(),
      short: day.toLocaleDateString(undefined, { weekday: 'short' }),
      long: day.toLocaleDateString(undefined, { weekday: 'long' }),
    };
  }),
);

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;

const focus = computed(() => report.value?.focus);

// The focus trace: this week as a line, as far as today, over the week before as a dashed line,
// with the busiest day marked.
const trace = { width: 368, height: 124 };
const focusTrace = computed(() => {
  const week = focus.value;
  if (!week) return undefined;
  const sofar = days.value.filter((day) => day.at <= Date.now()).length;
  const before = hasLastWeek.value ? week.lastWeekDays : undefined;
  const most = Math.max(1, ...week.days, ...(before ?? []));
  const at = (ms: number, i: number) => ({
    x: 12 + (i * (trace.width - 24)) / 6,
    y: trace.height - 20 - (ms / most) * (trace.height - 46),
  });
  const line = (list: number[]) =>
    list
      .map((ms, i) => at(ms, i))
      .map(({ x, y }) => `${x},${y}`)
      .join(' ');
  const shown = week.days.slice(0, Math.max(1, sofar));
  const peakMs = Math.max(0, ...week.days);
  const peakIndex = week.days.indexOf(peakMs);
  return {
    points: shown.map((ms, i) => ({ ...at(ms, i), title: `${days.value[i]!.long}: ${formatDuration(ms)}` })),
    line: line(shown),
    before: before && line(before),
    axis: days.value.map((day, i) => ({ x: at(0, i).x, label: day.short })),
    peak:
      peakMs > 0
        ? {
            ...at(peakMs, peakIndex),
            index: peakIndex,
            label: `Peak · ${days.value[peakIndex]!.short} · ${formatDuration(peakMs)}`,
          }
        : undefined,
  };
});
const traceLabel = computed(
  () =>
    `Focus by day: ${
      focus.value?.days.map((ms, i) => `${days.value[i]!.long} ${formatDuration(ms)}`).join(', ') ?? ''
    }`,
);

// Each profile's share of the week's focus, in its star color.
const colorOf = (id: string) => starColorFor(id, props.profiles.find((profile) => profile.id === id)?.starColor);
const focusTrend = computed(() => {
  if (!focus.value || !hasLastWeek.value) return '';
  const change = focus.value.focusedMs - focus.value.lastWeekMs;
  if (Math.abs(change) < 60_000) return focus.value.focusedMs ? 'About the same as last week' : '';
  return `${change > 0 ? '↑' : '↓'} ${formatDuration(Math.abs(change))} on last week`;
});
const sessionsLine = computed(() => {
  if (!focus.value) return '';
  const parts = [plural(focus.value.sessions, 'session'), `longest ${formatDuration(focus.value.longestMs)}`];
  if (focus.value.overtimeMs) parts.push(`${formatDuration(focus.value.overtimeMs)} past the timer`);
  return parts.join(' · ');
});
// Goals set at the start of sessions, and how "Did you do it?" was answered.
const goalsLine = computed(() => {
  const goals = focus.value?.goals;
  if (!goals?.set) return '';
  const unanswered = goals.set - goals.met - goals.missed;
  const done = `${goals.met} of ${plural(goals.set, 'goal')} done`;
  return unanswered ? `${done} · ${unanswered} unanswered` : done;
});
// Only from two days in a row, since one day isn't a streak yet.
const streakLine = computed(() => {
  const { current, longest } = props.streaks;
  if (current >= 2) {
    return current === longest
      ? `On a ${current}-day streak, your longest yet`
      : `On a ${current}-day streak · longest ${longest} days`;
  }
  return longest >= 2 ? `Longest streak: ${longest} days` : '';
});
const endings = computed(() => {
  if (!focus.value) return '';
  const { emergencyPasses, idle, broken } = focus.value;
  return [
    emergencyPasses && `${emergencyPasses} ended early with an emergency pass`,
    idle && `${idle} ended while you were away`,
    broken && `${broken} broken by a browser without the extension`,
  ]
    .filter((part) => typeof part === 'string')
    .join(' · ');
});

const distractions = computed(() => report.value?.distractions);
const visitsTrend = computed(() => {
  if (!distractions.value || !hasLastWeek.value) return '';
  const change = distractions.value.visits - distractions.value.lastWeekVisits;
  if (!change) return distractions.value.visits ? 'The same as last week' : '';
  return `${Math.abs(change)} ${change > 0 ? 'more' : 'fewer'} than last week`;
});
const siteLine = (site: SiteDistractions) =>
  [
    plural(site.visits, 'trip'),
    site.why && `mostly when ${site.why}`,
    site.peakHour !== undefined && formatHourRange(site.peakHour),
  ]
    .filter((part) => typeof part === 'string')
    .join(' · ');

const satellites = computed(() => report.value?.satellites);
const outcomes = computed(() => {
  if (!satellites.value) return '';
  const { done, tasks, waiting, deleted, snoozes } = satellites.value;
  return [
    done && `${done} done`,
    tasks && `${tasks} on the task list`,
    waiting && `${waiting} still waiting`,
    deleted && `${deleted} deleted`,
    snoozes && `snoozed ${plural(snoozes, 'time')}`,
  ]
    .filter((part) => typeof part === 'string')
    .join(' · ');
});

const weekday = new Intl.DateTimeFormat(undefined, { weekday: 'long' });
const month = new Intl.DateTimeFormat(undefined, { month: 'long' });
const inWeek = (at: number) => at >= start.value && at < addWeeks(start.value, 1);
/** What happened and on which day, e.g. "An aurora, for your longest session yet" on "Friday". */
function eventLine(event: RareEvent) {
  const planet = `${month.format(event.at)}'s planet`;
  const what = {
    shootingStar: 'a shooting star',
    meteorShower: 'a meteor shower',
    aurora: 'an aurora',
    supernova: 'a supernova',
    moonCrash: `a moon crashed into ${planet}`,
    split: `a chunk broke off ${planet}`,
    shatter: `${planet} shattered into a ring of debris`,
  }[event.kind];
  const why =
    event.reason === 'longestSession'
      ? ', for your longest session yet'
      : event.reason === 'longestStreak'
        ? `, for your longest streak yet: ${event.streakDays} days in a row`
        : '';
  return { id: event.id, what: `${what[0]!.toUpperCase()}${what.slice(1)}${why}`, day: weekday.format(event.at) };
}
const skyLines = computed(() => props.events.filter((event) => inWeek(event.at)).map(eventLine));
const weekComets = computed(() => props.comets.filter((comet) => inWeek(comet.at)).length);

const mostTrips = computed(() => Math.max(1, ...(distractions.value?.hours ?? [])));

// The readout's sections are numbered in order, counting only those shown.
const sections = computed(() => {
  if (!report.value) return [];
  return [
    'focus',
    focus.value?.profiles.length && 'profiles',
    'distractions',
    'satellites',
    (skyLines.value.length || weekComets.value) && 'sky',
    report.value.suggestions.length && 'notes',
  ].filter((section) => typeof section === 'string');
});
const numbered = (section: string) => String(sections.value.indexOf(section) + 1).padStart(2, '0');

const empty = computed(() => !focus.value?.sessions && !distractions.value?.visits && !satellites.value?.launched);
</script>

<template>
  <section class="week" aria-labelledby="week-title">
    <nav class="weeks" aria-label="Weeks">
      <button
        type="button"
        class="card arrow"
        aria-label="Week before"
        :disabled="start <= firstWeek"
        @click="start = addWeeks(start, -1)"
      >
        ‹
      </button>
      <h2 id="week-title" class="heading">
        <button
          type="button"
          class="pick mono"
          aria-haspopup="dialog"
          :aria-expanded="picking"
          title="Pick a week"
          @click="picking = !picking"
        >
          <span>{{ title !== range ? title : 'Week of' }} <span class="caret" aria-hidden="true">▾</span></span>
          <b>{{ range }}</b>
        </button>
      </h2>
      <button
        type="button"
        class="card arrow"
        aria-label="Week after"
        :disabled="start >= thisWeek"
        @click="start = addWeeks(start, 1)"
      >
        ›
      </button>
    </nav>
    <Transition name="fade" mode="out-in">
      <div v-if="constellation" :key="constellation.id" class="constellation">
        <form v-if="renaming" class="rename" @submit.prevent="saveName">
          <input
            ref="nameInput"
            v-model="draft"
            class="card mono"
            :maxlength="maxConstellationNameLength"
            :placeholder="constellation.named ? 'A made-up name' : constellation.name"
            aria-label="This week's constellation"
            @keydown.esc.prevent="renaming = false"
            @blur="saveName"
          />
        </form>
        <button
          v-else
          type="button"
          class="card name mono"
          title="Name this week's constellation"
          @click="startRenaming"
        >
          ✦ {{ constellation.name }} <span class="edit">· Rename</span>
        </button>
      </div>
    </Transition>
    <Unfold>
      <div v-if="picking" class="picker-space">
        <WeekPicker
          :selected="start"
          :first-day="firstDay"
          :earliest="firstWeek"
          :latest="thisWeek"
          @pick="
            (week: number) => {
              start = week;
              picking = false;
            }
          "
          @close="picking = false"
        />
      </div>
    </Unfold>

    <!-- Weeks slide in from the side of the arrow that was pressed, in a dark frame like Settings'
         so the reading stays clear over the sky. -->
    <Transition :name="slide" mode="out-in">
      <div :key="report?.start ?? 'loading'" :class="{ 'card panel report': report || error }">
        <p v-if="error" class="error" role="alert">{{ error }}</p>
        <p v-else-if="report && empty" class="mono empty">
          Nothing logged
          {{ start === thisWeek ? 'yet this week. Start a session and it shows up here.' : 'that week.' }}
        </p>

        <template v-else-if="report && focus && distractions && satellites">
          <section class="read" aria-labelledby="focus">
            <h3 id="focus" class="mono">{{ numbered('focus') }} — Focus</h3>
            <p class="figure">
              <b>{{ formatDuration(focus.focusedMs) }}</b>
              <span v-if="focusTrend" class="mono">{{ focusTrend }}</span>
            </p>
            <svg
              v-if="focusTrace"
              class="trace"
              :viewBox="`0 0 ${trace.width} ${trace.height}`"
              role="img"
              :aria-label="traceLabel"
            >
              <polyline v-if="focusTrace.before" class="before" :points="focusTrace.before" />
              <template v-if="focusTrace.peak">
                <line
                  class="peak-line"
                  :x1="focusTrace.peak.x"
                  :x2="focusTrace.peak.x"
                  :y1="focusTrace.peak.y - 6"
                  y2="12"
                />
                <text
                  class="peak"
                  :x="focusTrace.peak.index > 3 ? focusTrace.peak.x - 6 : focusTrace.peak.x + 6"
                  y="14"
                  :text-anchor="focusTrace.peak.index > 3 ? 'end' : 'start'"
                >
                  {{ focusTrace.peak.label.toUpperCase() }}
                </text>
              </template>
              <polyline class="this" :points="focusTrace.line" />
              <circle
                v-for="(point, i) in focusTrace.points"
                :key="i"
                :class="{ top: i === focusTrace.peak?.index }"
                :cx="point.x"
                :cy="point.y"
                :r="i === focusTrace.peak?.index ? 3.5 : 2"
              >
                <title>{{ point.title }}</title>
              </circle>
              <text v-for="day in focusTrace.axis" :key="day.x" class="axis" :x="day.x" :y="trace.height - 2">
                {{ day.label.toUpperCase() }}
              </text>
            </svg>
            <p v-if="focusTrace?.before" class="mono legend">— This week &nbsp; ┄ Week before</p>
            <p v-if="streakLine" class="note">{{ streakLine }}</p>
            <template v-if="focus.sessions">
              <p class="note">{{ sessionsLine }}</p>
              <p v-if="goalsLine" class="note">{{ goalsLine }}</p>
              <p v-if="endings" class="note">{{ endings }}</p>
            </template>
          </section>

          <section v-if="focus.profiles.length" class="read" aria-labelledby="profiles">
            <h3 id="profiles" class="mono">{{ numbered('profiles') }} — Profiles</h3>
            <div class="stack" aria-hidden="true">
              <i
                v-for="profile in focus.profiles"
                :key="profile.id"
                :style="{ '--c': colorOf(profile.id), flex: profile.focusedMs || 1 }"
              />
            </div>
            <ul class="rows">
              <li v-for="profile in focus.profiles" :key="profile.id">
                <span class="dot" :style="{ '--c': colorOf(profile.id) }" aria-hidden="true" />{{ profile.name }}
                <span class="value mono">
                  <b>{{ formatDuration(profile.focusedMs) }}</b> · {{ plural(profile.sessions, 'session') }}
                </span>
              </li>
            </ul>
          </section>

          <section class="read" aria-labelledby="distractions">
            <h3 id="distractions" class="mono">{{ numbered('distractions') }} — Distractions</h3>
            <p class="figure">
              <b>{{ distractions.visits }}</b>
              <span class="mono">{{ distractions.visits === 1 ? 'trip' : 'trips' }} to redirected sites</span>
            </p>
            <p v-if="visitsTrend" class="mono">{{ visitsTrend }}</p>
            <template v-if="distractions.visits">
              <ol class="scope" aria-label="Trips by time of day">
                <li
                  v-for="(count, hour) in distractions.hours"
                  :key="hour"
                  :class="{ zero: !count }"
                  :style="{ height: `${(count / mostTrips) * 100}%` }"
                  :title="`${formatHour(hour)}: ${plural(count, 'trip')}`"
                >
                  <span class="hidden">{{ formatHour(hour) }}: {{ plural(count, 'trip') }}</span>
                </li>
              </ol>
              <p class="hours mono" aria-hidden="true">
                <span>12am</span><span>6am</span><span>12pm</span><span>6pm</span><span>12am</span>
              </p>
              <p class="note">
                You went back to focus {{ distractions.turnedBack }} of {{ distractions.visits }} times.
              </p>
              <ul class="rows">
                <li v-for="site in distractions.sites" :key="site.recipeId">
                  {{ siteName(site.recipeId) }}
                  <span class="value mono">{{ siteLine(site) }}</span>
                </li>
              </ul>
              <p v-if="distractions.whys.length" class="note">
                Why: {{ distractions.whys.map((w) => `${w.why} ${w.visits}`).join(' · ') }}
              </p>
              <template v-if="distractions.intentions.length">
                <p class="mono sub">What you came for</p>
                <ul class="rows">
                  <li v-for="intention in distractions.intentions" :key="intention.text">
                    <span class="intention">{{ intention.text }}</span>
                    <span v-if="intention.count > 1" class="value mono">×{{ intention.count }}</span>
                  </li>
                </ul>
              </template>
            </template>
          </section>

          <section class="read" aria-labelledby="satellites">
            <h3 id="satellites" class="mono">{{ numbered('satellites') }} — Satellites</h3>
            <p class="figure">
              <b>{{ satellites.launched }}</b
              ><span class="mono">launched</span>
            </p>
            <p v-if="outcomes" class="note">{{ outcomes }}</p>
          </section>

          <section v-if="skyLines.length || weekComets" class="read" aria-labelledby="in-the-sky">
            <h3 id="in-the-sky" class="mono">{{ numbered('sky') }} — In the sky</h3>
            <ul class="rows">
              <li v-for="line in skyLines" :key="line.id">
                <span class="dot sky" aria-hidden="true" />{{ line.what }}
                <span class="value mono">{{ line.day }}</span>
              </li>
            </ul>
            <p v-if="weekComets" class="note">{{ plural(weekComets, 'comet') }} for going back to focus</p>
          </section>

          <section v-if="report.suggestions.length" class="read" aria-labelledby="suggestions">
            <h3 id="suggestions" class="mono">{{ numbered('notes') }} — Notes</h3>
            <p v-for="tip in report.suggestions" :key="tip" class="tip">{{ tip }}</p>
            <form v-if="planProfile && !planProfile.plan" class="plan-idea" @submit.prevent="savePlan">
              <p class="mono">
                An if-then plan for <b>{{ planProfile.name }}</b>
              </p>
              <p class="note">
                Decide now what you'll do when the urge comes, and the redirect page shows it to you each time. Planning
                "when this happens, I'll do that" ahead of time is one of the best-tested ways to change a habit.
              </p>
              <!-- A text box can't draw the frame's corner ticks itself, so a label around it does. -->
              <label class="card box">
                <textarea
                  v-model="planDraft"
                  aria-label="If-then plan"
                  rows="2"
                  :maxlength="maxPlanLength"
                  @keydown.enter.exact.prevent="savePlan"
                />
              </label>
              <button class="card mono" type="submit" :disabled="!planDraft.trim()">
                Save as {{ planProfile.name }}'s plan
              </button>
            </form>
            <p v-else-if="planProfile && planSaved" class="note saved">
              Saved as {{ planProfile.name }}'s plan. Change it any time in Profiles › Plan.
            </p>
          </section>
        </template>
      </div>
    </Transition>
  </section>
</template>

<style scoped>
.weeks {
  display: flex;
  align-items: center;
  gap: 8px;
}

.arrow {
  width: 30px;
  height: 30px;
  padding: 0;
  background: none;
  color: var(--dim);
  font: 13px var(--mono);
}

.arrow:hover:not(:disabled) {
  color: var(--starlight);
}

.arrow:disabled {
  opacity: 0.3;
  cursor: default;
}

.heading {
  flex: 1;
  margin: 0;
  font: inherit;
}

.pick {
  display: grid;
  width: 100%;
  padding: 2px 8px;
  border-radius: 3px;
  background: none;
  transition: color 300ms;
}

.pick b {
  color: var(--starlight);
  font-size: 13px;
  letter-spacing: 0.06em;
}

.pick:hover,
.pick[aria-expanded='true'] {
  color: var(--dim);
}

.caret {
  display: inline-block;
  transition: rotate 400ms var(--ease);
}

[aria-expanded='true'] .caret {
  rotate: 180deg;
}

/* The week's constellation shows behind the header, named here beneath it. */
.constellation {
  display: flex;
  justify-content: center;
  margin-top: 8px;
}

.name {
  padding: 4px 9px;
  background: none;
  color: var(--dim);
  transition: color 300ms;
}

.edit {
  color: var(--faint);
  transition: color 300ms;
}

.name:hover,
.name:focus-visible {
  color: var(--starlight);
}

.name:hover .edit,
.name:focus-visible .edit {
  color: var(--dim);
}

.rename input {
  width: 220px;
  padding: 4px 9px;
  border: 0;
  outline: none;
  background: none;
  color: var(--starlight);
  text-align: center;
  text-transform: none;
}

.rename input:focus {
  --t: var(--tick-bright);
}

.picker-space {
  padding-top: 12px;
}

/* One long readout, with hairlines between its numbered sections. */
.report {
  margin-top: 12px;
  padding: 0 14px;
}

.read {
  padding: 14px 0;
  border-bottom: 1px solid var(--line);
}

.read:last-child {
  border-bottom: 0;
}

h3 {
  margin: 0 0 6px;
  font-weight: 400;
}

p {
  margin: 0;
}

.figure {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 4px 10px;
}

.figure b {
  font-size: 40px;
  font-weight: 200;
  letter-spacing: -0.03em;
  line-height: 1.1;
}

.note {
  margin-top: 8px;
  color: var(--dim);
  font-size: 12px;
}

.sub {
  margin-top: 12px;
}

.trace {
  display: block;
  width: 100%;
  height: auto;
  margin-top: 8px;
  overflow: visible;
}

.trace polyline {
  fill: none;
}

.trace .this {
  stroke: var(--glow);
  stroke-width: 1.4;
  filter: drop-shadow(0 0 4px rgb(245 200 107 / 0.6));
}

.trace .before {
  stroke: rgb(220 228 255 / 0.3);
  stroke-dasharray: 3 4;
}

.trace circle {
  fill: #0b1020;
  stroke: var(--glow);
}

.trace circle.top {
  fill: var(--glow);
}

.trace .peak-line {
  stroke: rgb(245 200 107 / 0.5);
}

.trace text {
  font: 9px var(--mono);
  letter-spacing: 0.08em;
}

.trace .peak {
  fill: var(--glow);
}

.trace .axis {
  fill: var(--faint);
  text-anchor: middle;
}

.legend {
  margin-top: 6px;
}

.stack {
  display: flex;
  gap: 2px;
  height: 4px;
  margin: 8px 0 10px;
}

.stack i {
  border-radius: 2px;
  background: var(--c);
  box-shadow: 0 0 8px -1px var(--c);
}

.rows {
  display: grid;
  gap: 6px;
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
}

.rows li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}

.value {
  margin-left: auto;
  text-align: right;
}

.dot.sky {
  --c: #b9a7f5;
}

.intention {
  overflow-wrap: anywhere;
}

/* Trips in each hour of the day, as thin bars. */
.scope {
  display: grid;
  grid-template-columns: repeat(24, 1fr);
  align-items: end;
  gap: 3px;
  height: 44px;
  margin: 10px 0 4px;
  padding: 0;
  border-bottom: 1px solid var(--line);
  list-style: none;
}

.scope li {
  min-height: 1px;
  border-radius: 1px 1px 0 0;
  background: #f2a7c3;
  box-shadow: 0 0 6px -1px #f2a7c3;
}

.scope li.zero {
  background: rgb(220 228 255 / 0.15);
  box-shadow: none;
}

.hours {
  display: flex;
  justify-content: space-between;
}

.hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.tip {
  margin-top: 8px;
  font-size: 12.5px;
}

/* The distraction tip as a plan to save, set off below the notes. */
.plan-idea {
  display: grid;
  gap: 8px;
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px dashed var(--line);
}

.plan-idea .note {
  margin: 0;
}

.box {
  display: flex;
}

.box:focus-within {
  --t: var(--tick-bright);
}

.plan-idea textarea {
  flex: 1;
  min-width: 0;
  padding: 8px 10px;
  border: 0;
  outline: 0;
  background: none;
  color: var(--starlight);
  font:
    12.5px/1.5 system-ui,
    sans-serif;
  resize: none;
}

.plan-idea button {
  justify-self: end;
  padding: 6px 10px;
  background: none;
  color: var(--dim);
  transition: color 300ms;
}

.plan-idea button:not(:disabled):hover {
  color: var(--starlight);
}

.plan-idea button:disabled {
  cursor: default;
  opacity: 0.5;
}

.saved {
  margin-top: 16px;
}

.empty {
  padding: 18px 0;
  text-align: center;
}

.error {
  padding: 14px 0;
  color: #f0a3a3;
  font-size: 12px;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 400ms ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.week-forward-enter-active,
.week-back-enter-active {
  transition:
    opacity 360ms ease,
    transform 460ms cubic-bezier(0.2, 0.8, 0.2, 1),
    filter 360ms ease;
}

.week-forward-leave-active,
.week-back-leave-active {
  transition:
    opacity 180ms ease,
    transform 200ms ease-in,
    filter 180ms ease;
}

.week-forward-enter-from,
.week-back-leave-to {
  opacity: 0;
  transform: translateX(28px);
  filter: blur(3px);
}

.week-forward-leave-to,
.week-back-enter-from {
  opacity: 0;
  transform: translateX(-28px);
  filter: blur(3px);
}

@media (prefers-reduced-motion: reduce) {
  .week-forward-enter-from,
  .week-forward-leave-to,
  .week-back-enter-from,
  .week-back-leave-to {
    transform: none;
    filter: none;
  }

  .caret {
    transition: none;
  }
}
</style>
