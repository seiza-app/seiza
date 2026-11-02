<script setup lang="ts">
import type { ActiveSession, ProfileSummary, Satellite, SatelliteAction, SessionGoal } from '@lock-in/core';
import { formatDuration, starColorFor } from '@lock-in/core';
import { computed, ref, watch } from 'vue';
import ExploreView from './ExploreView.vue';
import GoalCheck from './GoalCheck.vue';
import LaunchDock from './LaunchDock.vue';
import SatelliteLaunch from './SatelliteLaunch.vue';
import SatelliteOrbits from './SatelliteOrbits.vue';
import SatellitesBack from './SatellitesBack.vue';
import SatellitesView from './SatellitesView.vue';
import SessionView from './SessionView.vue';
import Unfold from './Unfold.vue';

// The home page (PLAN §5): the sky, with the profiles along the bottom to lock in with, the
// running session floating over it, and satellites, in the Observatory HUD's hairline frames and
// small readouts (PLAN Phase 5). Explore clears everything away so the sky
// is on its own; it all drifts toward you as it fades, and comes back the same way.
const props = defineProps<{
  profiles: ProfileSummary[];
  session: ActiveSession | null;
  /** The last session's goal, until "Did you do it?" is answered. */
  goalToAnswer: SessionGoal | null;
  passesLeft: number;
  satellites: { back: Satellite[]; orbiting: Satellite[]; tasks: Satellite[]; done: Satellite[] };
  /** The satellite box's hotkey, for the launch box's hint. */
  satelliteHotkey: string;
  /** Whether to offer the tour for new users, which the Sky does once. */
  offerTour: boolean;
  /** A new version of the app to offer, which the Sky does once for each, and whether it's
   * installing now or can be installed here at all. */
  offerUpdate?: { version: string; installing: boolean; canInstall: boolean };
  starCount: number;
  /** What the top of the page shows: the date, a quote for the day, time locked in today, and the streak running now. */
  glance: { date: string; quote?: { text: string; author?: string }; lockedInMs: number; streak: number };
  stardust: number;
  /** For Explore's readouts: how many weeks and planets there are, how far in it's zoomed, and the
   * constellation in the crosshair. */
  weekCount: number;
  planetCount: number;
  zoom: number;
  aimed?: { id: string; name: string; named: boolean; weekStart: number };
  thisWeek: number;
  /** The months with constellations, oldest first, and the one the Sky shows if stepped back (else the latest). */
  months: number[];
  month?: number;
}>();
const exploring = defineModel<boolean>('exploring', { required: true });
const emit = defineEmits<{
  start: [profileId: string, goal?: string];
  answer: [sessionId: string, met: boolean];
  end: [];
  pass: [];
  launch: [text: string];
  act: [id: string, action: SatelliteAction];
  /** Zoom the sky in (above 1) or out, while exploring. */
  zoom: [factor: number];
  /** Glide back to where exploring starts, around the latest weeks. */
  recenter: [];
  rename: [id: string, name: string];
  /** Show an earlier month's constellations on the Sky, or the latest (undefined). */
  month: [month: number | undefined];
  /** The answer to the tour's offer: take it, or not. */
  tour: [take: boolean];
  /** The answer to a new version's offer, or a look at what's new in it first. */
  update: [action: 'install' | 'later' | 'notes'];
}>();

// Stepping through the months, a month's constellations at a time.
const shownIndex = computed(() =>
  props.month === undefined ? props.months.length - 1 : props.months.indexOf(props.month),
);
const monthName = new Intl.DateTimeFormat(undefined, { month: 'short' });
const monthAndYear = new Intl.DateTimeFormat(undefined, { month: 'short', year: 'numeric' });
const monthLabel = computed(() => {
  const month = props.months[shownIndex.value];
  if (month === undefined) return '';
  return new Date(month).getFullYear() === new Date().getFullYear()
    ? monthName.format(month)
    : monthAndYear.format(month);
});
function step(by: number) {
  const index = shownIndex.value + by;
  if (index < 0 || index >= props.months.length) return;
  emit('month', index === props.months.length - 1 ? undefined : props.months[index]);
}

/** A count as a readout shows it, e.g. 03. */
const counted = (count: number) => String(count).padStart(2, '0');

// The running session glows in its profile's star color.
const sessionColor = computed(() => {
  const id = props.session?.profileId ?? '';
  return starColorFor(id, props.profiles.find((profile) => profile.id === id)?.starColor);
});
const goalProfile = computed(() => props.profiles.find((profile) => profile.id === props.goalToAnswer?.profileId));

const orbitingIds = computed(() => props.satellites.orbiting.map((satellite) => satellite.id));

const showingTasks = ref(false);
const taskCount = computed(() => props.satellites.orbiting.length + props.satellites.tasks.length);
const hasTasks = computed(() => taskCount.value > 0 || props.satellites.done.length > 0);
// Tasks wait while you focus, so they don't pull you back in.
watch(
  () => props.session,
  (session) => session && (showingTasks.value = false),
);
</script>

<template>
  <div class="sky-page">
    <Transition name="puff">
      <div v-show="!exploring" class="layer">
        <!-- A landing page: the day at the top, then the sky, then starting a session, then the
             thoughts and tasks that wait for after it. -->
        <header class="today">
          <h2 class="date">{{ glance.date }}</h2>
          <!-- A quote for the day, between sessions. It folds away while the task list is open, or
               "Did you do it?" waits, to make room for them. -->
          <Unfold>
            <figure v-if="!session && !showingTasks && !goalToAnswer && glance.quote" class="quote">
              <blockquote>{{ glance.quote.text }}</blockquote>
              <figcaption v-if="glance.quote.author" class="mono">{{ glance.quote.author }}</figcaption>
            </figure>
          </Unfold>
          <p class="glance mono">
            <span title="Time locked in today, including any past the timer"
              >Today <b>{{ formatDuration(glance.lockedInMs) }}</b></span
            >
            <span v-if="glance.streak >= 2" title="Days in a row with a star">
              · Streak <b>{{ glance.streak }} days</b></span
            >
          </p>
          <Unfold>
            <GoalCheck
              v-if="!session && goalToAnswer"
              :goal="goalToAnswer"
              :profile-name="goalProfile?.name ?? 'Last session'"
              :color="starColorFor(goalToAnswer.profileId, goalProfile?.starColor)"
              @answer="emit('answer', goalToAnswer.sessionId, $event)"
            />
          </Unfold>
          <!-- Offered once, the first time the Sky shows, and in Settings after that. -->
          <Unfold>
            <div v-if="offerTour && !session" class="card offer">
              <span class="mono">✦ New here? · <b>1-min tour</b></span>
              <span class="keys">
                <button class="key next" type="button" @click="emit('tour', true)">Take the tour</button>
                <button class="key" type="button" @click="emit('tour', false)">No thanks</button>
              </span>
            </div>
          </Unfold>
          <!-- Each new version is offered once, and stays in Settings after that. -->
          <Unfold>
            <div v-if="offerUpdate && !session" class="card offer update" role="status">
              <span class="mono"
                >✦ {{ offerUpdate.installing ? 'Updating to' : 'New version' }} ·
                <b>Seiza {{ offerUpdate.version }}</b></span
              >
              <span v-if="!offerUpdate.installing" class="keys">
                <button class="key" type="button" @click="emit('update', 'notes')">What's new ↗</button>
                <button v-if="offerUpdate.canInstall" class="key next" type="button" @click="emit('update', 'install')">
                  Update and restart
                </button>
                <button class="key" type="button" @click="emit('update', 'later')">Later</button>
              </span>
            </div>
          </Unfold>
        </header>

        <div class="middle">
          <SessionView
            v-if="session"
            :session="session"
            :passes-left="passesLeft"
            :color="sessionColor"
            @end="emit('end')"
            @pass="emit('pass')"
          />
        </div>

        <LaunchDock
          v-if="!session"
          :profiles="profiles"
          :first-star="starCount === 0"
          @start="(id, goal) => emit('start', id, goal)"
        />

        <!-- Thoughts for later, together: what came back after the last session, the task list
             (opened from Tasks below), and the launch box. -->
        <div class="card thoughts" data-tour="satellites">
          <!-- The thoughts in orbit circle the frame. The orbits only just reach past it, to stay
               in the window and clear of the profiles above. -->
          <SatelliteOrbits
            :ids="orbitingIds"
            :shape="{ x: [2, 9], y: [2, 6], tilt: [-2, 2], period: [36, 64] }"
            :max="12"
          />
          <Unfold>
            <SatellitesBack
              v-if="!session && satellites.back.length"
              class="part"
              :satellites="satellites.back"
              @act="(id, action) => emit('act', id, action)"
            />
          </Unfold>
          <Unfold>
            <div v-if="showingTasks" id="tasks" class="part tasks">
              <SatellitesView
                :orbiting="satellites.orbiting"
                :tasks="satellites.tasks"
                :done="satellites.done"
                @act="(id, action) => emit('act', id, action)"
              />
            </div>
          </Unfold>
          <SatelliteLaunch :session-running="!!session" :hotkey="satelliteHotkey" @launch="emit('launch', $event)" />
        </div>

        <div class="foot">
          <button class="mono" type="button" title="Clear the screen and look around the sky" @click="exploring = true">
            ⌖ Explore the sky
          </button>
          <span v-if="!session && months.length > 1" class="months mono">
            <button type="button" aria-label="Earlier month" :disabled="shownIndex <= 0" @click="step(-1)">‹</button>
            <b aria-live="polite" title="The month whose constellations the sky shows">{{ monthLabel }}</b>
            <button type="button" aria-label="Later month" :disabled="shownIndex >= months.length - 1" @click="step(1)">
              ›
            </button>
          </span>
          <span
            v-if="session?.timer?.strictness === 'hard' && session.timer.minutes > 0"
            class="mono"
            title="Emergency passes end a Hard session early. Each one comes back 7 days after it's used."
          >
            Passes left <b>{{ counted(passesLeft) }}</b>
          </span>
          <!-- During a session, only how many are in orbit, not what they say. -->
          <span v-if="session" class="end mono" title="Thoughts you parked. They come back after this session.">
            In orbit <b>{{ counted(satellites.orbiting.length) }}</b>
          </span>
          <button
            v-else-if="hasTasks"
            class="end mono"
            type="button"
            aria-controls="tasks"
            :aria-expanded="showingTasks"
            @click="showingTasks = !showingTasks"
          >
            Tasks <b>{{ counted(taskCount) }}</b> <span class="caret" aria-hidden="true">▾</span>
          </button>
        </div>
      </div>
    </Transition>

    <Transition name="fade">
      <ExploreView
        v-if="exploring"
        :star-count="starCount"
        :stardust="stardust"
        :week-count="weekCount"
        :planet-count="planetCount"
        :zoom="zoom"
        :aimed="aimed"
        :this-week="thisWeek"
        @zoom="emit('zoom', $event)"
        @recenter="emit('recenter')"
        @rename="(id, name) => emit('rename', id, name)"
        @done="exploring = false"
      />
    </Transition>
  </div>
</template>

<style scoped>
.sky-page {
  position: relative;
  flex: 1;
  display: flex;
  min-height: 0;
}

.layer {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
  transform-origin: 50% 60%;
}

/* The day, big, with a quote for it and today's readouts beneath. */
.today {
  display: grid;
  gap: 6px;
  margin-top: 4px;
}

.date {
  margin: 0;
  font-size: 26px;
  font-weight: 200;
  letter-spacing: -0.01em;
  line-height: 1.15;
}

.quote {
  margin: 0;
}

blockquote {
  margin: 0;
  color: var(--dim);
  font-family: 'Iowan Old Style', 'Palatino Linotype', Palatino, Georgia, serif;
  font-size: 13.5px;
  font-style: italic;
  line-height: 1.4;
  text-wrap: pretty;
}

figcaption {
  margin-top: 2px;
}

figcaption::before {
  content: '— ';
}

.glance {
  margin: 0;
}

/* It gives up room to the window first, its task list scrolling. */
.thoughts {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 5px 11px;
}

.thoughts:focus-within {
  --t: var(--tick-bright);
}

/* Above the satellites passing behind the frame, and below those passing in front. */
.orbits {
  --behind: -2;
  --in-front: 1;
  inset: 0;
}

/* Hairlines between what came back, the task list and the launch box. */
.part {
  padding: 4px 0 6px;
  border-bottom: 1px solid var(--line);
}

.middle {
  flex: 1;
  min-height: 0;
  display: grid;
  place-items: center;
}

.offer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  margin-top: 4px;
  padding: 6px 8px 6px 11px;
  white-space: nowrap;
}

.offer .keys {
  display: flex;
  gap: 6px;
}

/* A new version's offer has three keys, so they go under the line when the window is narrow. */
.offer.update {
  flex-wrap: wrap;
  row-gap: 6px;
}

.offer .next {
  color: var(--starlight);
  border-color: var(--tick);
}

/* Explore on the left, the month in the middle and the tasks on the right. */
.foot {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
}

.foot button {
  padding: 0;
  border-radius: 0;
  background: none;
  transition: color 300ms;
}

.foot > button:first-child {
  justify-self: start;
}

.foot button:hover,
.foot button[aria-expanded='true'] {
  color: var(--starlight);
}

.end {
  grid-column: 3;
  justify-self: end;
}

.caret {
  display: inline-block;
  transition: rotate 400ms var(--ease);
}

[aria-expanded='true'] .caret {
  rotate: 180deg;
}

.months {
  display: flex;
  align-items: center;
  gap: 4px;
}

.months b {
  min-width: 64px;
  text-align: center;
}

.months button {
  width: 18px;
  font-size: 13px;
  line-height: 1;
}

.months button:disabled {
  opacity: 0.3;
  pointer-events: none;
}

/* Long lists scroll inside the drawer, so the rest stays in the window. */
.tasks {
  min-height: 0;
  max-height: 34vh;
  /* Fading out at the bottom, so it's clear there's more to scroll to. */
  -webkit-mask-image: linear-gradient(#000 calc(100% - 14px), transparent);
  mask-image: linear-gradient(#000 calc(100% - 14px), transparent);
  overflow-y: auto;
  scrollbar-width: none;
}

.puff-enter-active,
.puff-leave-active {
  transition:
    opacity 700ms ease,
    transform 900ms cubic-bezier(0.2, 0.7, 0.2, 1),
    filter 700ms ease;
}

.puff-enter-from,
.puff-leave-to {
  opacity: 0;
  transform: scale(1.12);
  filter: blur(6px);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 700ms ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .puff-enter-from,
  .puff-leave-to {
    transform: none;
    filter: none;
  }

  .caret {
    transition: none;
  }
}
</style>
