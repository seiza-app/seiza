<script setup lang="ts">
import { endLock, formatClock, timerStatus, type ActiveSession, type Strictness } from '@lock-in/core';
import { computed, ref } from 'vue';
import { useNow } from '../app';
import EndEarly from './EndEarly.vue';

// The running session over the clear sky, as an instrument readout (PLAN Phase 5): a big clock
// over a ruler from 0 to the timer's length with a marker at now, and End early with a line on
// what the profile's strictness will ask.
const props = defineProps<{ session: ActiveSession; passesLeft: number; color: string }>();
const emit = defineEmits<{ end: []; pass: [] }>();

const now = useNow();
const status = computed(() => timerStatus(props.session, now.value));

// Ending before the timer is done goes through the profile's strictness first.
const lock = computed(() => endLock(props.session, now.value));
const endingEarly = ref(false);
function requestEnd() {
  if (lock.value) endingEarly.value = true;
  else emit('end');
}
const minutes = computed(() => props.session.timer?.minutes ?? 0);
const length = computed(() => formatClock(minutes.value * 60_000));

// The quiet timer: once it's done the session keeps going, and it only says "Done",
// unless the profile asks to show how long ago.
const clock = computed(() => {
  const s = status.value;
  if (s.state === 'running') return formatClock(s.remainingMs);
  if (s.state === 'none') return formatClock(s.elapsedMs);
  if (!props.session.timer?.showSinceDone) return 'Done';
  const since = Math.floor(s.sinceMs / 60_000);
  return since < 1 ? 'Done · just now' : `Done · ${since} min ago`;
});

const started = computed(() =>
  new Date(props.session.startedAt).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }),
);
const detail = computed(() => {
  if (status.value.state === 'running') return `Left of ${length.value}`;
  return status.value.state === 'none' ? 'No timer' : `Timer done at ${length.value}`;
});

// The ruler: a long tick every `step` minutes, at most six of them, with four short ones between.
const steps = [5, 10, 15, 30, 60, 120, 240];
const step = computed(() => steps.find((s) => minutes.value / s <= 6) ?? Math.ceil(minutes.value / 6));
const share = computed(() => Math.min(1, (now.value - props.session.startedAt) / (minutes.value * 60_000)));
const marks = computed(() => {
  const list: number[] = [];
  for (let at = 0; at < minutes.value; at += step.value) list.push(at);
  // The last long tick's label makes way for the length's if they'd crowd each other.
  if (minutes.value - list[list.length - 1]! < step.value * 0.4) list.pop();
  return [...list, minutes.value];
});

const strictnessName: Record<Strictness, string> = { gentle: 'Gentle', medium: 'Medium', hard: 'Hard' };
const lockHelp = computed(() => {
  const until = `before ${length.value}`;
  if (lock.value === 'gentle') return `Gentle: ending ${until} asks you to pause a moment first.`;
  if (lock.value === 'medium') return `Medium: ending ${until} asks you to type some random text first.`;
  if (lock.value === 'hard')
    return props.passesLeft > 0
      ? `Hard: it can't end ${until} without an emergency pass.`
      : `Hard: it can't end ${until}, and there are no emergency passes left this week.`;
  return '';
});
</script>

<template>
  <section class="card session" :style="{ '--c': color }">
    <p class="mono head">
      <span class="dot" aria-hidden="true" />Locked in — <b>{{ session.profileName }}</b>
      <template v-if="session.timer && minutes > 0"> · {{ strictnessName[session.timer.strictness] }}</template>
    </p>
    <p class="clock" :class="{ done: status.state === 'done' }">{{ clock }}</p>
    <div v-if="minutes > 0" class="ruler" aria-hidden="true">
      <div
        class="ticks"
        :style="{ '--step': `${(step / minutes) * 100}%`, '--short': `${(step / 5 / minutes) * 100}%` }"
      />
      <div class="past" :style="{ scale: `${share} 1` }" />
      <div class="now" :style="{ left: `${share * 100}%` }" />
      <div class="scale mono">
        <span v-for="mark in marks" :key="mark" :style="{ left: `${(mark / minutes) * 100}%` }">{{ mark }}</span>
      </div>
    </div>
    <p v-if="session.goal" class="goal"><span class="mono">Goal ·</span> {{ session.goal }}</p>
    <p class="mono">
      {{ detail }} · Started <b>{{ started }}</b>
    </p>
    <EndEarly
      v-if="endingEarly && lock"
      class="end-early"
      :lock="lock"
      :passes-left="passesLeft"
      @end="emit('end')"
      @pass="emit('pass')"
      @cancel="endingEarly = false"
    />
    <template v-else>
      <button class="card end mono" type="button" @click="requestEnd">{{ lock ? 'End early' : 'End session' }}</button>
      <p v-if="lockHelp" class="help">{{ lockHelp }}</p>
    </template>
  </section>
</template>

<style scoped>
.session {
  width: 100%;
  box-sizing: border-box;
  padding: 16px 16px 14px;
  text-align: center;
}

/*
 * Glassier than the other frames: the same dark tint, so the clock stays easy to read, with a
 * sheen across the top, a faint glare, lit edges and a little of the session's color pooling at
 * the bottom. It's drawn with gradients rather than by blurring the sky behind it, which would be
 * redone every frame the sky moves (theme.css).
 */
.session::before {
  box-shadow:
    inset 0 1px 0 rgb(220 228 255 / 0.14),
    inset 0 -1px 0 rgb(220 228 255 / 0.04),
    inset 0 0 28px rgb(220 228 255 / 0.04),
    0 12px 32px rgb(0 0 0 / 0.25);
}

.session::after {
  content: '';
  position: absolute;
  inset: 0;
  z-index: -1;
  border-radius: inherit;
  pointer-events: none;
  background:
    linear-gradient(180deg, rgb(220 228 255 / 0.08), transparent 40%),
    linear-gradient(115deg, transparent 32%, rgb(220 228 255 / 0.045) 44%, transparent 56%),
    radial-gradient(120% 70% at 50% 125%, color-mix(in srgb, var(--c) 12%, transparent), transparent 70%);
}

p {
  margin: 0;
}

.head {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 7px;
}

.head .dot {
  animation: breathe 4s ease-in-out infinite;
}

/* It rests while the window isn't focused, to save battery (app.ts). */
:root.resting .head .dot {
  animation-play-state: paused;
}

@keyframes breathe {
  50% {
    opacity: 0.4;
  }
}

.clock {
  margin: 6px 0 8px;
  font-size: 62px;
  font-weight: 200;
  letter-spacing: -0.03em;
  line-height: 1.05;
  font-variant-numeric: tabular-nums;
}

.clock.done {
  font-size: 40px;
}

/* What the session is for, if it was given a goal. */
.goal {
  margin: 0 8px 8px;
  font-size: 13px;
  overflow-wrap: anywhere;
}

.goal .mono {
  color: var(--c);
}

/* A ruler from 0 to the timer's length, with what's gone by lit up to a marker at now. */
.ruler {
  position: relative;
  height: 18px;
  margin: 0 8px 20px;
}

.ticks {
  position: absolute;
  inset: 0;
  background:
    linear-gradient(rgb(220 228 255 / 0.5), rgb(220 228 255 / 0.5)) right / 1px 100% no-repeat,
    repeating-linear-gradient(90deg, rgb(220 228 255 / 0.5) 0 1px, transparent 1px var(--step)) top / 100% 100%
      no-repeat,
    repeating-linear-gradient(90deg, rgb(220 228 255 / 0.25) 0 1px, transparent 1px var(--short)) bottom / 100% 45%
      no-repeat;
}

.past {
  position: absolute;
  bottom: 0;
  left: 0;
  width: 100%;
  height: 3px;
  background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--c) 60%, transparent));
  transform-origin: left;
  transition: scale 1s linear;
}

.now {
  position: absolute;
  top: -4px;
  width: 1px;
  height: 24px;
  background: var(--c);
  box-shadow: 0 0 8px var(--c);
  transition: left 1s linear;
}

.scale span {
  position: absolute;
  top: 22px;
  translate: -50% 0;
}

.help {
  margin-top: 8px;
  color: var(--faint);
  font-size: 11.5px;
}

.end {
  margin-top: 12px;
  padding: 6px 12px;
  background: none;
  color: var(--dim);
  transition: color 300ms;
}

.end:hover {
  color: var(--starlight);
}

.end-early {
  margin-top: 16px;
}

@media (prefers-reduced-motion: reduce) {
  .head .dot {
    animation: none;
  }

  .past,
  .now {
    transition: none;
  }
}
</style>
