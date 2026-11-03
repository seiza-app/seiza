<script setup lang="ts">
// Everything round the pages: the canvas sky, the frame and its readouts, the constellation
// navigator, and the scroller the sections sit in. Each page's sections with a `data-name` become
// stars on the navigator.
import { createSky } from '~/utils/sky';

const route = useRoute();
const nuxtApp = useNuxtApp();
const { locked, lockOnce } = useLock();

const canvas = ref<HTMLCanvasElement>();
const scroller = ref<HTMLElement>();
const navigator = ref<HTMLElement>();
const names = ref<string[]>([]);
// Read from the page rather than template refs, which don't keep the list's order.
const starEls = () => [...(navigator.value?.querySelectorAll<HTMLElement>('button') ?? [])];
const segEls = () => [...(navigator.value?.querySelectorAll<HTMLElement>('.seg') ?? [])];

// How high each star sits in the navigator's band, as a share of it.
const HEIGHTS = [0.66, 0.36, 0.7, 0.32, 0.64, 0.38, 0.62];
const heights = computed(() => names.value.map((_, i) => HEIGHTS[i % HEIGHTS.length]!));

let reduce = false;
let sky: ReturnType<typeof createSky> | null = null;
let sections: HTMLElement[] = [];
let nebulae: HTMLElement | null = null;
let bodies: HTMLElement | null = null;
const narrow = () => innerWidth < 700;
// The constellation sits up and to the right, or across the top on a phone.
const placement = () => (narrow() ? { cx: 0.5, cy: 0.14, cs: 0.55 } : { cx: 0.76, cy: 0.2, cs: 0.28 });
const skyFor = (on: boolean) =>
  on ? { haze: 0, constellation: 1, milky: 1 } : { haze: 1, constellation: 0, milky: 0 };

watch(locked, (on) => {
  document.body.classList.toggle('locked', on);
  // The distractions pop in a little after the page loads; after the first switch they answer at once.
  document.body.style.setProperty('--lead', '0ms');
  sky?.set(skyFor(on));
});

// ── The constellation navigator ─────────────────────────────────────────────

function layoutNavigator() {
  const nav = navigator.value;
  if (!nav) return;
  const w = nav.clientWidth;
  const h = nav.clientHeight;
  const pad = 26;
  const count = starEls().length;
  const points = heights.value.map((y, i) => [pad + (i / Math.max(1, count - 1)) * (w - pad * 2), y * h] as const);
  starEls().forEach((star, i) => {
    star.style.left = `${points[i]![0]}px`;
    star.style.top = `${points[i]![1]}px`;
  });
  segEls().forEach((seg, i) => {
    const [x1, y1] = points[i]!;
    const [x2, y2] = points[i + 1]!;
    seg.style.left = `${x1}px`;
    seg.style.top = `${y1}px`;
    seg.style.width = `${Math.hypot(x2 - x1, y2 - y1)}px`;
    seg.style.transform = `rotate(${Math.atan2(y2 - y1, x2 - x1)}rad)`;
  });
}

function go(i: number, smooth = true) {
  const section = sections[i];
  if (!section || !scroller.value) return;
  scroller.value.scrollTo({ top: section.offsetTop, behavior: smooth && !reduce ? 'smooth' : 'auto' });
}

// As you scroll, each line draws toward the next star, and the stars light up as you reach their
// sections. The sky drifts a little slower than the page.
let queued = false;
// The section the navigator shows as current, so its stars change only when that does.
let shownActive = -1;
function onScroll() {
  queued = false;
  const el = scroller.value;
  if (!el) return;
  const top = el.scrollTop;
  const view = el.clientHeight;
  const atEnd = top + view >= el.scrollHeight - 4;
  const tops = sections.map((s) => s.offsetTop);
  const at = top + view * 0.4;
  let active = 0;
  tops.forEach((t, i) => {
    if (at >= t) active = i;
  });
  if (atEnd) active = sections.length - 1;
  if (active !== shownActive) {
    shownActive = active;
    starEls().forEach((star, i) => {
      star.classList.toggle('reached', i <= active);
      star.setAttribute('aria-current', String(i === active));
    });
  }
  segEls().forEach((seg, i) => {
    const p = atEnd || i < active ? 1 : Math.min(1, Math.max(0, (at - tops[i]!) / (tops[i + 1]! - tops[i]!)));
    (seg.firstElementChild as HTMLElement).style.transform = `scaleX(${p})`;
  });
  document.body.classList.toggle('past-hero', top > view * 0.6);
  // Scrolling on past the top locks in, once, so the rest sits under a clear sky.
  if (top > view * 0.45) lockOnce();
  if (!reduce) {
    sky?.set({ shift: top * 0.25 });
    // The nebulae and planets are nearer than the stars, so they move a little more.
    if (nebulae) nebulae.style.transform = `translateY(${(-Math.min(top * 0.06, innerHeight * 0.35)).toFixed(1)}px)`;
    if (bodies) bodies.style.transform = `translateY(${(-top * 0.1).toFixed(1)}px)`;
  }
}

function queueScroll() {
  if (queued) return;
  queued = true;
  requestAnimationFrame(onScroll);
}

// ── Each page's content rises in the first time you reach it ────────────────

let seen: IntersectionObserver | null = null;

// ── Glass that tilts under the pointer (only the glass layer, so the text stays crisp) ──

const tilting = new WeakSet<HTMLElement>();
const clamp = (v: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, v));

function attachTilt(card: HTMLElement) {
  if (tilting.has(card)) return;
  tilting.add(card);
  let box: DOMRect | null = null;
  let frame = 0;
  let px = 0;
  let py = 0;
  card.addEventListener('pointerenter', () => {
    box = card.getBoundingClientRect();
    card.classList.add('tilting');
    card.style.setProperty('--sheen', '1');
  });
  card.addEventListener(
    'pointermove',
    (event) => {
      px = event.clientX;
      py = event.clientY;
      if (frame || !box) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (!box) return;
        const x = clamp((px - box.left) / box.width);
        const y = clamp((py - box.top) / box.height);
        // Big frames tilt less, so their far edge doesn't swing far.
        const max = Math.min(4, 1600 / Math.max(box.width, box.height));
        card.style.setProperty('--ry', `${((x - 0.5) * max).toFixed(2)}deg`);
        card.style.setProperty('--rx', `${((0.5 - y) * max).toFixed(2)}deg`);
        card.style.setProperty('--mx', `${(x * 100).toFixed(1)}%`);
        card.style.setProperty('--my', `${(y * 100).toFixed(1)}%`);
      });
    },
    { passive: true },
  );
  card.addEventListener('pointerleave', () => {
    box = null;
    card.classList.remove('tilting');
    card.style.setProperty('--rx', '0deg');
    card.style.setProperty('--ry', '0deg');
    card.style.setProperty('--sheen', '0');
  });
}

// ── Setting up each page ─────────────────────────────────────────────────────

async function preparePage() {
  const el = scroller.value;
  if (!el) return;
  sections = [...el.querySelectorAll<HTMLElement>('section[data-name]')];
  // A new page has new navigator stars to light.
  shownActive = -1;
  names.value = sections.map((s) => s.dataset.name ?? '');

  seen?.disconnect();
  seen = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add('seen');
        seen?.unobserve(entry.target);
      }
    },
    // Once its top is a fifth of the way up the frame, however tall the section is.
    { root: el, rootMargin: '0px 0px -20% 0px' },
  );
  for (const s of el.querySelectorAll('section')) seen.observe(s);
  // The first section rises in straight away.
  requestAnimationFrame(() => sections[0]?.classList.add('seen'));

  if (matchMedia('(hover: hover) and (pointer: fine)').matches && !reduce) {
    el.querySelectorAll<HTMLElement>('.glass').forEach(attachTilt);
  }

  // Reading pages sit under a clear sky.
  if (route.meta.lockOnArrival) lockOnce();

  await nextTick();
  el.scrollTop = 0;
  const target = route.hash ? sections.findIndex((s) => `#${s.id}` === route.hash) : -1;
  if (target > 0) go(target, false);
  layoutNavigator();
  onScroll();
}

// Links to a section of the page you're on glide there inside the frame, instead of jumping.
function onClick(event: MouseEvent) {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey) return;
  const link = (event.target as Element).closest('a');
  if (!link || !link.hash || link.target) return;
  const url = new URL(link.href);
  if (url.origin !== location.origin || url.pathname !== route.path) return;
  const i = sections.findIndex((s) => `#${s.id}` === url.hash);
  if (i < 0) return;
  event.preventDefault();
  event.stopPropagation();
  go(i);
}

// Wheel over the frame's bands still scrolls the sections.
function onWheel(event: WheelEvent) {
  const el = scroller.value;
  if (el && !el.contains(event.target as Node)) el.scrollBy({ top: event.deltaY });
}

function onResize() {
  sky?.set(placement());
  layoutNavigator();
  onScroll();
}

let resizer: ResizeObserver | null = null;
let stopPageHook: (() => void) | undefined;

onMounted(() => {
  reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  sky = createSky(canvas.value!, {
    ...skyFor(locked.value),
    seed: 5,
    ...placement(),
  });
  document.body.classList.toggle('locked', locked.value);
  nebulae = document.querySelector('.nebulae');
  bodies = document.querySelector('.bodies');
  addEventListener('wheel', onWheel, { passive: true });
  // Capture, so this runs before NuxtLink's own handler.
  document.addEventListener('click', onClick, true);
  resizer = new ResizeObserver(onResize);
  resizer.observe(document.body);
  stopPageHook = nuxtApp.hook('page:finish', preparePage);
  preparePage();
  requestAnimationFrame(() => document.body.classList.add('ready'));
});

onBeforeUnmount(() => {
  removeEventListener('wheel', onWheel);
  document.removeEventListener('click', onClick, true);
  resizer?.disconnect();
  seen?.disconnect();
  stopPageHook?.();
  sky?.destroy();
  sky = null;
});
</script>

<template>
  <canvas id="sky" ref="canvas" aria-hidden="true"></canvas>
  <SkyBackdrop />

  <SiteFrame>
    <nav ref="navigator" class="navigator" aria-label="Sections, as a constellation">
      <div v-for="(_, i) in names.slice(1)" :key="`seg-${i}`" class="seg"><b></b></div>
      <button
        v-for="(name, i) in names"
        :key="`star-${i}`"
        type="button"
        :class="heights[i]! < 0.5 ? 'up' : 'down'"
        @click="go(i)"
      >
        <i></i><span>{{ name }}</span>
      </button>
    </nav>
  </SiteFrame>

  <main id="scroller" ref="scroller" @scroll.passive="queueScroll">
    <slot />
  </main>
</template>
