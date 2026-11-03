<script setup lang="ts">
// Thoughts go round the session's star on a tilted ellipse, nearer ones brighter. A launched one
// lifts off from the box and joins them; ending the session brings them all back down as a
// checklist. Placed by script each frame, only while the section is on screen.
interface Satellite {
  id: number;
  text: string;
  angle: number;
  speed: number;
  lift: number;
  done: boolean;
  gone: boolean;
  what: string;
}

const COLORS = ['var(--deep)', 'var(--glow)', 'var(--lavender)', '#9fe3c9'];
const STARTING = ['Reply to Sam about Friday', 'Look up that bookshelf video', 'Book the dentist'];

const launcher = ref<HTMLElement>();
const stage = ref<HTMLElement>();
const satellites = ref<Satellite[]>([]);
const thought = ref('');
const ended = ref(false);
const landed = ref(false);
const els = new Map<number, HTMLElement>();
let nextId = 0;
let reduce = false;

function add(text: string, lift: boolean) {
  const n = satellites.value.length;
  satellites.value.push({
    id: nextId++,
    text,
    angle: n * 2.1 + 0.6,
    speed: 0.22 + (n % 3) * 0.05,
    lift: lift ? 0 : 1,
    done: false,
    gone: false,
    what: '',
  });
}
STARTING.forEach((t) => add(t, false));

function place(sat: Satellite, el: HTMLElement | null) {
  if (el) {
    els.set(sat.id, el);
    placeAll(0);
  } else els.delete(sat.id);
}

function placeAll(dt: number) {
  const box = stage.value;
  if (!box) return;
  const w = box.clientWidth;
  const h = box.clientHeight;
  const rx = Math.min(w * 0.39, 180);
  const ry = rx / 2.6;
  const tilt = (-8 * Math.PI) / 180;
  for (const sat of satellites.value) {
    const el = els.get(sat.id);
    if (!el) continue;
    if (!reduce) sat.angle += sat.speed * dt;
    sat.lift = Math.min(1, sat.lift + dt / 1.4);
    const ex = Math.cos(sat.angle) * rx;
    const ey = Math.sin(sat.angle) * ry;
    let x = w / 2 + ex * Math.cos(tilt) - ey * Math.sin(tilt);
    let y = h / 2 + ex * Math.sin(tilt) + ey * Math.cos(tilt);
    // Lifting off: from the launch box below the stage, curving up into the orbit.
    const k = 1 - (1 - sat.lift) ** 3;
    x = w / 2 + (x - w / 2) * k;
    y = h + 40 + (y - h - 40) * k;
    const near = (Math.sin(sat.angle) + 1) / 2;
    el.style.transform = `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px) scale(${(0.75 + near * 0.4).toFixed(2)})`;
    el.style.opacity = String((0.45 + near * 0.55) * Math.min(1, sat.lift * 2));
    // Behind the star on the far side of the orbit.
    el.style.zIndex = near > 0.5 ? '2' : '0';
  }
}

let orbiting = false;
let last = 0;
function frame(now: number) {
  if (!orbiting) return;
  placeAll(last ? Math.min(0.05, (now - last) / 1000) : 0);
  last = now;
  requestAnimationFrame(frame);
}

function launch() {
  const text = thought.value.trim();
  if (!text) return;
  add(text, !reduce);
  thought.value = '';
}

function endSession() {
  ended.value = true;
  landed.value = false;
  // Two frames, so the list is on the page before it drifts down into place.
  requestAnimationFrame(() => requestAnimationFrame(() => (landed.value = true)));
}

function startSession() {
  ended.value = false;
  satellites.value = [];
  STARTING.forEach((t) => add(t, false));
}

let watcher: IntersectionObserver | undefined;
onMounted(() => {
  reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  watcher = new IntersectionObserver(
    ([entry]) => {
      orbiting = !!entry?.isIntersecting && !reduce;
      last = 0;
      placeAll(0);
      if (orbiting) requestAnimationFrame(frame);
    },
    { root: document.getElementById('scroller') },
  );
  watcher.observe(launcher.value!);
});

onBeforeUnmount(() => {
  orbiting = false;
  watcher?.disconnect();
});
</script>

<template>
  <section id="satellites" data-name="Satellites">
    <div class="two">
      <div>
        <p class="kicker mono reveal"><b>04</b> · Satellites · thoughts for later</p>
        <h2 class="reveal" style="--d: 80ms">Launch the thought. Keep the focus.</h2>
        <p class="lede reveal" style="--d: 160ms">
          Mid-focus thoughts like "I should check this real quick" turn into distractions. Press
          <kbd>Alt</kbd>+<kbd>P</kbd> (<kbd>Option</kbd>+<kbd>P</kbd> on a Mac) from anywhere, type it, and press
          Return. It goes into orbit, and you're straight back where you were.
        </p>
        <ul class="facts sat-facts">
          <li class="hud glass reveal" style="--d: 220ms">
            <h3>Out of sight</h3>
            <p>During a session you only see how many are in orbit, not what they say.</p>
          </li>
          <li class="hud glass reveal" style="--d: 280ms">
            <h3>Back when you're done</h3>
            <p>Nothing pops up. When the session ends, they wait for you: "3 satellites came back."</p>
          </li>
          <li class="hud glass reveal" style="--d: 340ms">
            <h3>A quick checklist</h3>
            <p>Tick each one Done, Snooze it into orbit again, make it a Task, or Delete it.</p>
          </li>
        </ul>
        <p class="source reveal" style="--d: 400ms">
          <b>Why it works.</b> Writing down a plan for an unfinished task stops it from pulling at your attention.
          (Masicampo &amp; Baumeister, 2011. A set of lab studies, so a modest finding.)
        </p>
      </div>

      <div ref="launcher" class="launcher hud glass reveal" :class="{ ended }" style="--d: 200ms">
        <div class="launcher-top">
          <span class="mono">Satellites · thoughts for later</span>
          <span class="mono sat-session">{{ ended ? 'Session ended' : 'Deep Work · in a session' }}</span>
        </div>

        <div ref="stage" class="orbit-stage" aria-hidden="true">
          <div class="orbit-path"></div>
          <div class="sun"></div>
          <i
            v-for="(sat, i) in satellites"
            :key="sat.id"
            :ref="(el) => place(sat, el as HTMLElement | null)"
            class="satellite"
            :style="{ '--c': COLORS[i % COLORS.length] }"
          ></i>
        </div>
        <p class="orbit-count mono" aria-live="polite">{{ satellites.length }} in orbit</p>

        <form class="launch" @submit.prevent="launch">
          <label class="visually-hidden" for="thought">A distracting thought</label>
          <input
            id="thought"
            v-model="thought"
            maxlength="80"
            autocomplete="off"
            placeholder="Distracting thought? Launch it — it'll come back after your session."
          />
          <button class="btn primary" type="submit">Launch</button>
        </form>
        <p class="launch-hint mono"><kbd>Alt</kbd>+<kbd>P</kbd> opens this box from any app.</p>

        <div v-if="ended" class="came-back">
          <p class="came-title">
            {{ satellites.length === 1 ? '1 satellite came back' : `${satellites.length} satellites came back` }}
          </p>
          <ul>
            <li
              v-for="(sat, i) in satellites"
              :key="sat.id"
              :class="{ landed, done: sat.done, gone: sat.gone }"
              :style="{ '--d': `${i * 120}ms` }"
            >
              <span>{{ sat.text }}</span>
              <span class="what mono">{{ sat.what }}</span>
              <span class="acts">
                <button type="button" @click="sat.done = !sat.done">Done</button>
                <button type="button" @click="sat.what = 'Back in orbit'">Snooze</button>
                <button type="button" @click="sat.what = 'On your tasks'">Task</button>
                <button type="button" @click="sat.gone = true">Delete</button>
              </span>
            </li>
          </ul>
        </div>

        <button class="btn ghost session-end" type="button" @click="ended ? startSession() : endSession()">
          {{ ended ? 'Start a new session ›' : 'End the session ›' }}
        </button>
      </div>
    </div>
  </section>
</template>
