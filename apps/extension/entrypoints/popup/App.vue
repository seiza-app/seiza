<script lang="ts" setup>
import {
  endLock,
  starColorFor,
  findRecipe,
  formatClock,
  siteFor,
  siteRecipes,
  timerStatus,
  type Profile,
  type SiteRecipe,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';
import { computed, onUnmounted, ref } from 'vue';
import EndEarly from '@/components/EndEarly.vue';
import SeizaWordmark from '@/components/SeizaWordmark.vue';
import SiteToggles from '@/components/SiteToggles.vue';

const session = useStorageItem(activeSession);
const profileList = useStorageItem(profiles);
const linked = useStorageItem(desktopLinked);
const offeredTour = useStorageItem(tourOffered);

const activeProfile = computed(() => profileList.value?.find((p) => p.id === session.value?.profileId));

// activeTab lets the popup read the current tab's URL when the user opens it. Any site on the
// web can be set here: a built-in one, one already added to a profile, or one to add.
const tab = ref<{ id?: number; url: string }>();
void browser.tabs.query({ active: true, currentWindow: true }).then(([current]) => {
  if (current?.url) tab.value = { id: current.id, url: current.url };
});
const siteRecipe = computed<SiteRecipe | undefined>(() => {
  if (!tab.value || !profileList.value) return undefined;
  return findRecipe(siteRecipes(recipes, profileList.value), tab.value.url) ?? siteFor(recipes, tab.value.url);
});
const sitePath = computed(() => (tab.value ? new URL(tab.value.url).pathname : '/'));

// Ticks once a second, only while the popup is open.
const now = ref(Date.now());
const tick = setInterval(() => (now.value = Date.now()), 1000);
onUnmounted(() => clearInterval(tick));

// Sessions run by the desktop app have its quiet timer; the rest just count up.
const elapsed = computed(() => {
  if (!session.value) return '';
  const status = timerStatus(session.value, now.value);
  if (status.state === 'running') return formatClock(status.remainingMs);
  if (status.state === 'none') return formatClock(status.elapsedMs);
  const since = Math.floor(status.sinceMs / 60_000);
  return session.value.timer?.showSinceDone && since >= 1 ? `Done · ${since} min ago` : 'Done';
});

// Sessions from the desktop app can have a strictness, which applies until the timer is done.
const lock = computed(() => (session.value ? endLock(session.value, now.value) : undefined));
const endingEarly = ref(false);
function requestEnd() {
  if (lock.value) endingEarly.value = true;
  else void endSession();
}

function openSettings() {
  void browser.runtime.openOptionsPage();
  window.close();
}

// The tour for new users runs on the settings page, which it opens. The popup offers it once.
async function takeTour() {
  await tourOffered.setValue(true);
  await browser.tabs.create({ url: `${browser.runtime.getURL('/options.html')}#tour` });
  window.close();
}

// Each profile's star color, as in the app's sky.
const color = (profile: Profile | undefined) => (profile ? starColorFor(profile.id, profile.starColor) : undefined);

function start(profile: Profile) {
  now.value = Date.now();
  void startSession(profile.id);
}
</script>

<template>
  <main>
    <header>
      <h1><SeizaWordmark /></h1>
      <span v-if="linked" class="linked mono" title="Sessions stay in step with the Seiza app">
        <span class="dot" aria-hidden="true" />App linked
      </span>
      <button class="key settings" type="button" @click="openSettings">Settings</button>
    </header>

    <section v-if="session" class="card active" :style="{ '--c': color(activeProfile) }">
      <p class="mono label">
        <span class="dot" aria-hidden="true" /><span
          >Locked in · <b>{{ activeProfile?.name ?? 'Unknown profile' }}</b></span
        >
      </p>
      <p class="elapsed" :class="{ done: elapsed.startsWith('Done') }">{{ elapsed }}</p>
      <EndEarly v-if="endingEarly && lock" :lock="lock" @end="endSession()" @cancel="endingEarly = false" />
      <button v-else class="key end" type="button" @click="requestEnd">End session</button>
    </section>

    <!-- Only once storage says there's no session, so a running one doesn't flash this first. -->
    <template v-else-if="session === null && profileList">
      <div v-if="offeredTour === false" class="card offer">
        <p><span class="star" aria-hidden="true">✦</span> New here? A quick tour of what Seiza does.</p>
        <span class="offer-buttons">
          <button class="key lit" type="button" @click="takeTour">Take the tour</button>
          <button class="key" type="button" @click="tourOffered.setValue(true)">No thanks</button>
        </span>
      </div>
      <section>
        <p class="mono label">▶ Start a session</p>
        <ul>
          <li v-for="profile in profileList" :key="profile.id">
            <button
              class="card profile"
              type="button"
              :style="{ '--c': color(profile) }"
              :title="`Start a ${profile.name} session`"
              @click="start(profile)"
            >
              <span class="name"><span class="dot" aria-hidden="true" />{{ profile.name }}</span>
              <span class="summary">{{ profileSummary(profile) }}</span>
              <span class="start mono" aria-hidden="true">▶ Start</span>
            </button>
          </li>
        </ul>
      </section>
    </template>

    <SiteToggles
      v-if="siteRecipe && profileList && session !== undefined"
      :recipe="siteRecipe"
      :path="sitePath"
      :tab-id="tab?.id"
      :profiles="profileList"
      :session="session"
    />
  </main>
</template>

<style>
body {
  /* Browsers allow up to 800px. A little over the usual 300px leaves room for the level switch. */
  width: 360px;
  min-height: 0;
}
</style>

<style scoped>
main {
  padding: 14px 14px 16px;
}

header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--line);
}

h1 {
  margin: 0;
  font-size: 22px;
}

.star {
  color: var(--glow);
}

.linked {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-left: auto;
}

.linked + .settings {
  margin-left: 0;
}

.settings {
  margin-left: auto;
}

.label {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 0 8px;
  color: var(--dim);
}

ul {
  display: grid;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* A profile to start, like the app's: its color dot, what it does, and a spelled-out Start. */
.profile {
  display: grid;
  gap: 3px;
  width: 100%;
  padding: 11px 12px 8px;
}

.profile:active {
  scale: 0.98;
}

.name {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13px;
  font-weight: 600;
}

.summary {
  color: var(--faint);
  font-size: 11.5px;
  line-height: 1.4;
}

.start {
  margin-top: 5px;
  padding-top: 5px;
  border-top: 1px dashed var(--line);
  color: var(--dim);
  transition:
    color 300ms,
    text-shadow 300ms;
}

.profile:hover .start,
.profile:focus-visible .start {
  color: var(--c);
  text-shadow: 0 0 8px color-mix(in srgb, var(--c) 60%, transparent);
}

/* The running session: its profile, then a big clock. */
.active {
  padding: 12px 14px 14px;
  text-align: center;
}

.active .label {
  justify-content: center;
}

.elapsed {
  margin: 6px 0 14px;
  font-size: 40px;
  font-weight: 200;
  letter-spacing: 0.02em;
  font-variant-numeric: tabular-nums;
  text-shadow: 0 0 18px color-mix(in srgb, var(--c, var(--glint)) 30%, transparent);
}

.elapsed.done {
  font-size: 26px;
}

.end {
  width: 100%;
  padding: 8px;
}

.offer {
  margin-bottom: 14px;
  padding: 10px 12px;
}

.offer p {
  margin: 0;
  color: var(--dim);
  font-size: 12px;
}

.offer-buttons {
  display: flex;
  gap: 6px;
  margin-top: 8px;
}
</style>
