<script setup lang="ts">
import type { BlockerStatus, KnownBrowser, OwnQuote, SavedBackup, Settings, UpdateStatus } from '@lock-in/core';
import {
  backupDay,
  glowFor,
  importSummary,
  maxBackupBytes,
  maxOwnQuotes,
  maxQuoteAuthorLength,
  maxQuoteLength,
  nextGlow,
  unlockedGlows,
} from '@lock-in/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed, onMounted, ref } from 'vue';
import {
  blockerStatus,
  checkForUpdates,
  exportBackup,
  importBackup,
  installBlocker,
  installUpdate,
  mac,
  releasePage,
  removeBlocker,
  showBackup,
} from '../app';
import GlowPicker from './GlowPicker.vue';
import HotkeyField from './HotkeyField.vue';
import NumberField from './NumberField.vue';
import ToggleSwitch from './ToggleSwitch.vue';

const props = defineProps<{
  settings: Settings;
  passesLeft: number;
  browsers: KnownBrowser[];
  sessionRunning: boolean;
  /** All the time ever focused, which unlocks glow colors. */
  lifetimeFocusMs: number;
  ownQuotes: OwnQuote[];
  updates?: UpdateStatus;
}>();
const emit = defineEmits<{
  'update:settings': [settings: Settings];
  'forget-browser': [path: string];
  'sample-data': [action: 'add' | 'remove'];
  'add-quote': [text: string, author: string];
  'remove-quote': [id: string];
  /** Take the tour for new users again. */
  tour: [];
  /** A backup was brought in, so the log is read again. */
  imported: [];
  close: [];
}>();
// Sample data is for trying the app while developing it, so release builds don't offer it.
const development = import.meta.env.DEV;

// The sections are numbered in order, like the weekly report's. Updates come first while a new
// version is waiting, so it's easy to find from the dot on the Settings tab.
const blocker = ref<BlockerStatus>();
const newVersion = computed(() => props.updates?.available ?? undefined);
const sections = computed(() => [
  ...(newVersion.value ? ['updates'] : []),
  'sessions',
  'satellites',
  'browsers',
  ...(blocker.value?.supported ? ['blocking'] : []),
  'sky',
  'quotes',
  'glow',
  'tour',
  'backup',
  ...(newVersion.value ? [] : ['updates']),
  ...(development ? ['sample'] : []),
]);
const numbered = (section: string) => String(sections.value.indexOf(section) + 1).padStart(2, '0');

// Blocking without the extension needs a small helper, installed only when it's turned on. macOS
// asks for an administrator's password then, in its own dialog.
const blockerBusy = ref(false);
const blockerProblem = ref('');
const checkBlocker = async () => (blocker.value = await blockerStatus());
onMounted(() => void checkBlocker());

async function withBlocker(action: () => Promise<void>) {
  blockerBusy.value = true;
  blockerProblem.value = '';
  try {
    await action();
  } catch (reason) {
    const message = String(reason);
    blockerProblem.value = message.includes('canceled')
      ? 'Nothing changed: the password prompt was canceled.'
      : `That didn't work: ${message}`;
  } finally {
    blockerBusy.value = false;
    await checkBlocker();
  }
}

function setBlocking(on: boolean) {
  if (!on) return save({ blockWithoutExtension: false });
  void withBlocker(async () => {
    const status = blocker.value;
    if (!status?.installed || !status.upToDate || !status.running) await installBlocker();
    save({ blockWithoutExtension: true });
  });
}

function save(patch: Partial<Settings>) {
  emit('update:settings', { ...props.settings, ...patch });
}

// Quotes of the user's own, for the Sky's quote for the day.
const quoteText = ref('');
const quoteAuthor = ref('');
const quotesFull = computed(() => props.ownQuotes.length >= maxOwnQuotes);
function addQuote() {
  emit('add-quote', quoteText.value, quoteAuthor.value);
  quoteText.value = '';
  quoteAuthor.value = '';
}

// Backups of everything, saved to the Downloads folder and brought in from a file picked here.
// What happened stays on one line under the keys until the next try.
const saved = ref<SavedBackup>();
const backupBusy = ref(false);
const backupNote = ref('');
const backupProblem = ref('');
const picker = ref<HTMLInputElement>();

async function withBackup(action: () => Promise<void>) {
  backupBusy.value = true;
  backupNote.value = '';
  backupProblem.value = '';
  try {
    await action();
  } catch (reason) {
    backupProblem.value = `That didn't work: ${String(reason)}`;
  } finally {
    backupBusy.value = false;
  }
}

const saveBackup = () =>
  withBackup(async () => {
    saved.value = await exportBackup(backupDay(new Date()));
  });

function bringIn(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  // Cleared, so picking the same file again still counts as a change.
  input.value = '';
  if (!file) return;
  void withBackup(async () => {
    if (file.size > maxBackupBytes) throw 'that file is too big to be a Seiza backup';
    backupNote.value = importSummary(await importBackup(await file.text()));
    emit('imported');
  });
}

// New versions (updates.rs): the app checks once a day unless this is off, and Check now asks
// straight away. Installing restarts the app, so it waits until a session ends.
const updateBusy = ref(false);
const updateProblem = ref('');
const checkedAt = new Intl.DateTimeFormat(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' });

async function withUpdates(action: () => Promise<unknown>) {
  updateBusy.value = true;
  updateProblem.value = '';
  try {
    await action();
  } catch (reason) {
    updateProblem.value = `That didn't work: ${String(reason)}`;
  } finally {
    updateBusy.value = false;
  }
}

function setChecking(on: boolean) {
  save({ checkForUpdates: on });
  if (on) void withUpdates(checkForUpdates);
}

// The one place the app asks for support: a quiet line at the very bottom (PLAN.md §8).
const supportUrl = 'https://ko-fi.com/sortelone';

// Glow colors come quietly with lifetime focus: only the ones unlocked are shown, with when the
// next one comes, in hours.
const glows = computed(() => unlockedGlows(props.lifetimeFocusMs));
const glow = computed(() => glowFor(props.settings.glow, props.lifetimeFocusMs));
const hoursSoFar = computed(() => Math.floor(props.lifetimeFocusMs / 3_600_000).toLocaleString());
const glowHelp = computed(() => {
  const next = nextGlow(props.lifetimeFocusMs);
  const soFar = `${hoursSoFar.value} ${hoursSoFar.value === '1' ? 'hour' : 'hours'}`;
  return next
    ? `For the frames' corners and the tabs' dot. More colors come with lifetime focus: ${soFar} so far, and the next at ${next.hours.toLocaleString()} hours.`
    : `For the frames' corners and the tabs' dot. Every color is unlocked, with ${soFar} of focus.`;
});
</script>

<template>
  <section class="settings card panel">
    <section class="read" aria-labelledby="sessions">
      <h2 id="sessions" class="mono">{{ numbered('sessions') }} — Sessions</h2>
      <div class="field">
        <div class="line">
          Emergency passes per week
          <NumberField
            label="Emergency passes per week"
            unit="passes"
            :min="0"
            :max="14"
            :model-value="settings.passesPerWeek"
            @update:model-value="save({ passesPerWeek: $event })"
          />
        </div>
        <span class="mono status"
          ><span class="dot pass" aria-hidden="true" />Left now <b>{{ String(passesLeft).padStart(2, '0') }}</b></span
        >
        <p class="help">
          For ending a Hard session before its timer is done. Each one comes back 7 days after it's used. During a Hard
          session this can only go down.
        </p>
      </div>
      <div class="field">
        <ToggleSwitch
          label="End a session when you're away"
          :model-value="settings.idleMinutes > 0"
          @update:model-value="save({ idleMinutes: $event ? 15 : 0 })"
        />
        <div v-if="settings.idleMinutes > 0" class="line">
          After
          <NumberField
            label="Minutes away before a session ends"
            unit="min away"
            :min="1"
            :max="120"
            :model-value="settings.idleMinutes"
            @update:model-value="save({ idleMinutes: $event })"
          />
        </div>
        <p class="help">
          For if you tend to forget to end sessions. With no keyboard or mouse input for a while, a session ends by
          itself, and the time away isn't counted.
        </p>
      </div>
    </section>

    <section class="read" aria-labelledby="satellites">
      <h2 id="satellites" class="mono">{{ numbered('satellites') }} — Satellites</h2>
      <div class="field">
        <HotkeyField :hotkey="settings.satelliteHotkey" />
        <p class="help">
          Opens the satellite box from any app, to park a thought without leaving what you're doing. To change it, press
          Change and then the new keys together. It needs {{ mac ? 'Option' : 'Alt' }}, or two of
          {{ mac ? 'Control, Shift and Command' : 'Control, Shift and the Windows key' }}, so it doesn't take a shortcut
          other apps use. The menu bar can always open the box too.
        </p>
      </div>
    </section>

    <section class="read" aria-labelledby="browsers" data-tour="browsers">
      <h2 id="browsers" class="mono">{{ numbered('browsers') }} — Browsers</h2>
      <ul v-if="browsers.length" class="browsers">
        <li v-for="browser in browsers" :key="browser.path">
          <span class="name" :title="browser.path">{{ browser.name }}</span>
          <span class="mono status"><span class="dot" aria-hidden="true" />Linked</span>
          <button
            class="key"
            type="button"
            :disabled="sessionRunning"
            :aria-label="`Forget ${browser.name}`"
            @click="emit('forget-browser', browser.path)"
          >
            Forget
          </button>
        </li>
      </ul>
      <p v-else class="none">None yet. Browsers show up here once the Seiza extension links to this app.</p>
      <p class="help">
        During a session whose profile tames sites, if one of these runs without the extension, Seiza warns you.
        {{
          sessionRunning
            ? 'Browsers can be forgotten once this session ends.'
            : "Forget a browser you've stopped using."
        }}
      </p>
      <div class="field">
        <ToggleSwitch
          label="Close browsers without the extension"
          :model-value="settings.closeBrowsers"
          @update:model-value="save({ closeBrowsers: $event })"
        />
        <p class="help">
          In a Medium or Hard session, the browser closes a minute after the warning. During a Hard session this can't
          be turned off.
        </p>
      </div>
    </section>

    <section v-if="blocker?.supported" class="read" aria-labelledby="blocking">
      <h2 id="blocking" class="mono">{{ numbered('blocking') }} — Blocking without the extension</h2>
      <div class="field">
        <ToggleSwitch
          label="Block restricted sites in every browser and app"
          :disabled="blockerBusy"
          :model-value="settings.blockWithoutExtension"
          @update:model-value="setBlocking"
        />
        <p class="help">
          For when you just want sites shut, with or without the extension. During a session, the profile's Restricted
          sites can't be reached from any browser or app, Safari included. Whole sites only: Tamed and Redirected sites,
          page rules and Seiza's own pages still need the extension. Tabs already open can keep working for a minute.
        </p>
        <p class="help">
          Turning it on installs a small helper, so macOS asks for your password once. It only edits Seiza's own lines
          in the system's list of sites, and if Seiza stops checking in for five minutes, it unblocks everything by
          itself.<template v-if="settings.blockWithoutExtension">
            During a Hard session this can't be turned off.</template
          >
        </p>
      </div>
      <p v-if="blocker.installed" class="mono status-line">
        <span class="dot" :class="{ off: !blocker.running }" aria-hidden="true" />
        Helper
        {{ blocker.running ? (blocker.upToDate ? 'running' : 'running an older version') : 'installed, not running' }}
      </p>
      <div v-if="blocker.installed" class="keys">
        <button
          v-if="!blocker.upToDate || !blocker.running"
          class="key"
          type="button"
          :disabled="blockerBusy"
          @click="withBlocker(installBlocker)"
        >
          {{ blocker.running ? 'Update the helper' : 'Reinstall the helper' }}
        </button>
        <button class="key" type="button" :disabled="blockerBusy" @click="withBlocker(removeBlocker)">
          Remove the helper
        </button>
      </div>
      <p v-if="blockerBusy" class="help">Waiting for macOS…</p>
      <p v-if="blockerProblem" class="help problem" role="alert">{{ blockerProblem }}</p>
    </section>

    <section class="read" aria-labelledby="sky">
      <h2 id="sky" class="mono">{{ numbered('sky') }} — Sky</h2>
      <div class="field">
        <ToggleSwitch
          label="Battery saver"
          :model-value="settings.batterySaver"
          @update:model-value="save({ batterySaver: $event })"
        />
        <p class="help">
          The stars twinkle 30 times a second instead of 60, so they're a little less smooth. Either way, the sky pauses
          while this window isn't the one you're using, and stops when it's closed.
        </p>
      </div>
    </section>

    <section class="read" aria-labelledby="quotes">
      <h2 id="quotes" class="mono">{{ numbered('quotes') }} — Your quotes</h2>
      <ul v-if="ownQuotes.length" class="quotes">
        <li v-for="quote in ownQuotes" :key="quote.id">
          <span class="quote"
            >{{ quote.text }}<span v-if="quote.author" class="faint mono"> · {{ quote.author }}</span></span
          >
          <button
            class="key"
            type="button"
            :aria-label="`Remove the quote “${quote.text}”`"
            @click="emit('remove-quote', quote.id)"
          >
            Remove
          </button>
        </li>
      </ul>
      <p v-else class="none">None yet. Seiza's own quotes take turns until you add some.</p>
      <form class="add-quote" @submit.prevent="addQuote">
        <!-- An input can't draw the frame's corner ticks itself, so a label around it does. -->
        <label class="card box">
          <input
            v-model="quoteText"
            :maxlength="maxQuoteLength"
            placeholder="A line that gets you back to work"
            aria-label="Quote"
          />
        </label>
        <span class="add-row">
          <label class="card box">
            <input
              v-model="quoteAuthor"
              :maxlength="maxQuoteAuthorLength"
              placeholder="Who said it (optional)"
              aria-label="Who said it"
            />
          </label>
          <button class="card mono" type="submit" :disabled="!quoteText.trim() || quotesFull">+ Add quote</button>
        </span>
      </form>
      <p class="help">
        {{
          quotesFull
            ? `That's the most quotes Seiza keeps (${maxOwnQuotes}). Remove one to add another.`
            : "Mixed in with Seiza's own for the Sky's quote of the day, and picked more often."
        }}
      </p>
    </section>

    <section class="read" aria-labelledby="glow">
      <h2 id="glow" class="mono">{{ numbered('glow') }} — Glow color</h2>
      <div class="field">
        <div class="line">
          <span
            >{{ glow.name }}<span v-if="glow.hours === 0" class="faint mono"> · default</span
            ><span v-else class="faint mono"> · {{ glow.hours.toLocaleString() }} hours</span></span
          >
          <GlowPicker :glows="glows" :picked="glow.id" @pick="save({ glow: $event })" />
        </div>
        <p class="help">{{ glowHelp }}</p>
      </div>
    </section>

    <section class="read" aria-labelledby="tour">
      <h2 id="tour" class="mono">{{ numbered('tour') }} — Tour</h2>
      <div class="field">
        <div class="line">
          A quick look around
          <button class="key" type="button" @click="emit('tour')">Take the tour</button>
        </div>
        <p class="help">Five short steps: the sky, starting a session, profiles, satellites and your browser.</p>
      </div>
    </section>

    <section class="read" aria-labelledby="backup">
      <h2 id="backup" class="mono">{{ numbered('backup') }} — Backup</h2>
      <div class="field">
        <div class="line">
          Everything in one file
          <button class="key" type="button" :disabled="backupBusy" @click="saveBackup">Export</button>
        </div>
        <p class="help">
          Your profiles, settings and quotes, and everything your sky and weekly report are made from: sessions, goals,
          trips to sites and satellites, with what they say. Keep it somewhere only you can open.
        </p>
        <p v-if="saved" class="status-line">
          <span class="dot" aria-hidden="true" />
          <span class="saved"
            >Saved in {{ saved.folder || 'your home folder' }} as <b>{{ saved.fileName }}</b></span
          >
          <button class="key" type="button" @click="withBackup(showBackup)">
            Show in {{ mac ? 'Finder' : 'its folder' }}
          </button>
        </p>
      </div>
      <div class="field">
        <div class="line">
          Bring in a backup
          <button class="key" type="button" :disabled="backupBusy || sessionRunning" @click="picker?.click()">
            Import…
          </button>
          <input ref="picker" type="file" accept=".json,application/json" hidden @change="bringIn" />
        </div>
        <p class="help">
          Adds what's in the file to what's here, so nothing here is lost. Profiles in it replace the same ones here,
          and its settings replace these, but the satellite hotkey and blocking stay as they're set on this computer.
          {{ sessionRunning ? 'Importing waits until this session ends.' : '' }}
        </p>
      </div>
      <p v-if="backupNote" class="help note" role="status">{{ backupNote }}</p>
      <p v-if="backupProblem" class="help problem" role="alert">{{ backupProblem }}</p>
    </section>

    <section class="read" :class="{ first: newVersion }" aria-labelledby="updates">
      <h2 id="updates" class="mono">{{ numbered('updates') }} — Updates</h2>
      <div class="field">
        <p v-if="newVersion" class="status-line">
          <span class="dot new" aria-hidden="true" />
          <span class="version"
            >Seiza <b>{{ newVersion }}</b> is out
            <span class="faint mono">· you have {{ updates?.currentVersion }}</span></span
          >
        </p>
        <p v-else class="mono status-line">
          <span class="dot" :class="{ off: !updates?.checkedAt }" aria-hidden="true" />
          Seiza {{ updates?.currentVersion }}
          {{ updates?.checkedAt ? `· up to date as of ${checkedAt.format(updates.checkedAt)}` : '· not checked yet' }}
        </p>
        <div class="keys">
          <button
            v-if="newVersion && updates?.canInstall"
            class="key next"
            type="button"
            :disabled="updateBusy || sessionRunning || updates.installing"
            @click="withUpdates(installUpdate)"
          >
            Update and restart
          </button>
          <button v-if="newVersion" class="key" type="button" @click="openUrl(releasePage(newVersion))">
            {{ updates?.canInstall ? "What's new ↗" : 'Download ↗' }}
          </button>
          <button
            class="key"
            type="button"
            :disabled="updateBusy || updates?.checking || updates?.installing"
            @click="withUpdates(checkForUpdates)"
          >
            Check now
          </button>
        </div>
        <p v-if="updates?.installing" class="help" role="status">
          Downloading Seiza {{ newVersion }}. It restarts by itself once it's installed.
        </p>
        <p v-else-if="newVersion && sessionRunning" class="help">Updating waits until this session ends.</p>
        <p v-else-if="newVersion && !updates?.canInstall" class="help">
          This copy wasn't installed from a release, so it can't update itself. The new version's page has its
          downloads.
        </p>
        <p v-if="updateProblem" class="help problem" role="alert">{{ updateProblem }}</p>
      </div>
      <div class="field">
        <ToggleSwitch
          label="Check for updates automatically"
          :model-value="settings.checkForUpdates"
          @update:model-value="setChecking"
        />
        <p class="help">
          Once a day, Seiza asks GitHub whether there's a new version. That's one request, carrying nothing about you
          beyond what any web request does. A new version is offered once on the Sky, then waits here.
        </p>
      </div>
    </section>

    <section v-if="development" class="read" aria-labelledby="sample">
      <h2 id="sample" class="mono">{{ numbered('sample') }} — Sample data <span class="faint">· dev builds</span></h2>
      <div class="field">
        <div class="line">
          Six weeks of made-up sessions
          <span class="buttons">
            <button class="key" type="button" @click="emit('sample-data', 'add')">Add</button>
            <button class="key" type="button" @click="emit('sample-data', 'remove')">Remove</button>
          </span>
        </div>
        <p class="help">
          For trying the sky and the weekly report. Sample sessions are marked, so Remove takes them all out and leaves
          your own.
        </p>
      </div>
    </section>

    <button class="card done mono" type="button" @click="emit('close')">Done · Back to the sky</button>
    <p class="colophon mono">Seiza · AGPL-3.0 · Stays on this computer</p>
    <p class="support">
      <a class="mono" :href="supportUrl" @click.prevent="openUrl(supportUrl)">Support Seiza on Ko-fi ↗</a>
    </p>
  </section>
</template>

<style scoped>
/* Everything sits in one dark frame, so it reads well over a busy part of the sky. */
.settings {
  display: flex;
  flex-direction: column;
  padding: 0 14px 12px;
}

/* Updates move to the top while a new version is waiting. */
.first {
  order: -1;
}

/* Numbered sections with hairlines between them, like the weekly report. */
.read {
  padding: 14px 0 4px;
  border-bottom: 1px solid var(--line);
}

h2 {
  margin: 0 0 2px;
  font-weight: 400;
}

.faint {
  color: var(--faint);
}

.field {
  display: grid;
  gap: 6px;
  padding: 10px 0;
}

.field + .field {
  border-top: 1px dashed var(--line);
}

.line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 12.5px;
}

.help {
  margin: 0;
  color: var(--faint);
  font-size: 11.5px;
}

.status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.dot.pass {
  --c: var(--glow);
}

.browsers {
  margin: 4px 0 8px;
  padding: 0;
  list-style: none;
}

.browsers li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 0;
  border-bottom: 1px solid var(--line);
  font-size: 12.5px;
}

.name {
  flex: 1;
}

.key:disabled {
  opacity: 0.4;
  cursor: default;
}

/* The blocker's helper: whether it's running, and keys to update or remove it. */
.status-line {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 2px 0 8px;
}

.dot.off {
  --c: var(--faint);
}

.dot.new {
  --c: var(--glow);
}

.version {
  font-size: 12.5px;
}

.keys .next {
  color: var(--starlight);
  border-color: var(--tick);
}

.keys {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 10px;
}

.problem {
  margin-bottom: 10px;
  color: var(--glint);
}

/* What the last export or import did. The file's name keeps its own capitals, unlike mono text. */
.saved {
  flex: 1;
  font-size: 12px;
  min-width: 0;
  overflow-wrap: anywhere;
}

.note {
  margin-bottom: 10px;
  color: var(--dim);
}

.none {
  margin: 6px 0 8px;
  font-size: 12.5px;
}

.buttons {
  display: flex;
  gap: 6px;
}

/* Your quotes: each with Remove, then a box to add one. */
.quotes {
  margin: 6px 0 4px;
  padding: 0;
  list-style: none;
}

.quotes li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 0;
  border-bottom: 1px dashed var(--line);
  font-size: 12.5px;
}

.quote {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}

.add-quote {
  display: grid;
  gap: 8px;
  margin: 10px 0 8px;
}

.add-row {
  display: flex;
  gap: 8px;
}

.box {
  display: flex;
  flex: 1;
  min-width: 0;
}

.box:focus-within {
  --t: var(--tick-bright);
}

.add-quote input {
  flex: 1;
  min-width: 0;
  padding: 8px 10px;
  border: 0;
  outline: 0;
  background: none;
  color: var(--starlight);
  font: 12px var(--mono);
}

.add-quote button {
  padding: 6px 10px;
  background: none;
  color: var(--dim);
  transition: color 300ms;
}

.add-quote button:not(:disabled):hover {
  color: var(--starlight);
}

.add-quote button:disabled {
  cursor: default;
  opacity: 0.5;
}

.done {
  display: block;
  margin: 16px auto 0;
  padding: 6px 12px;
  background: none;
  color: var(--dim);
  transition: color 300ms;
}

.done:hover {
  color: var(--starlight);
}

.colophon {
  margin: 12px 0 4px;
  text-align: center;
}

/* Quiet, like the colophon above it: no nags anywhere else. */
.support {
  margin: 0 0 4px;
  text-align: center;
}

.support a {
  color: var(--faint);
  text-decoration: underline;
  text-decoration-color: var(--tick);
  text-underline-offset: 3px;
  transition: color 300ms;
}

.support a:hover,
.support a:focus-visible {
  color: var(--starlight);
}
</style>
