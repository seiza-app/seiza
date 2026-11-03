<script setup lang="ts">
import type { SyncDevice } from '@lock-in/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { computed, ref, watch } from 'vue';
import {
  addSyncDevice,
  answerSyncDevice,
  joinSyncGroup,
  leaveSync,
  moveSyncServer,
  removeSyncDevice,
  startSyncGroup,
  stopPairing,
  syncDevices,
  usePairing,
  useNow,
  useSyncStatus,
} from '../app';

// Settings › Sync (PLAN §8, Sync): joining a group on a server you run, adding your other
// computers to it with a pairing code, and the group's devices. The core does the work
// (sync/setup.rs); this shows where it's got to.

defineProps<{
  /** The section's number in Settings, like "05". */
  number: string;
}>();

const status = useSyncStatus();
const pairing = usePairing();
const now = useNow();
const syncing = computed(() => !!status.value?.server);
const serverGuide = 'https://github.com/seiza-app/seiza/tree/main/apps/sync-server#readme';

const busy = ref(false);
const problem = ref('');
const note = ref('');

async function attempt(action: () => Promise<void>) {
  busy.value = true;
  problem.value = '';
  note.value = '';
  // The last pairing's outcome makes way for this one's.
  if (pairing.value.phase === 'failed' || pairing.value.phase === 'added') pairing.value = { phase: 'idle' };
  try {
    await action();
  } catch (reason) {
    problem.value = sentence(String(reason));
    // A server that hands out invites asks for one before it starts a group.
    if (problem.value.includes('invite code')) askInvite.value = true;
  } finally {
    busy.value = false;
  }
}

/** The core's problems are written to follow "That didn't work:", so they start lowercase. */
const sentence = (text: string) => `${text.charAt(0).toUpperCase()}${text.slice(1)}${/[.!?]$/.test(text) ? '' : '.'}`;

// Before joining: the server's address, this computer's name, and an invite if the server asks.
const address = ref('');
const deviceName = ref('');
const invite = ref('');
const askInvite = ref(false);
watch(
  () => status.value?.deviceName,
  (name) => (deviceName.value ||= name ?? ''),
  { immediate: true },
);

const startGroup = () =>
  attempt(() => startSyncGroup(address.value, deviceName.value, askInvite.value ? invite.value : undefined));

// Joining with a code. A computer with its own sessions takes the group's sky, so it says what
// that means first.
const joining = ref(false);
const code = ref('');
const confirmingSky = ref(false);
function join() {
  if (status.value?.ownSessions && !confirmingSky.value) {
    confirmingSky.value = true;
    return;
  }
  confirmingSky.value = false;
  void attempt(async () => {
    await joinSyncGroup(address.value, code.value, deviceName.value);
  });
}
// What Join still waits for. The code doesn't carry the server, so it needs the address too.
const joinNeeds = computed(() => {
  if (!address.value.trim()) return "Type your sync server's address above, the same one your other computer uses.";
  if (!code.value.trim()) return 'Type the code your other computer shows.';
  return '';
});
function cancelJoin() {
  confirmingSky.value = false;
  joining.value = false;
  code.value = '';
}

// The group's devices, asked of the server when the section opens and after each change.
const devices = ref<SyncDevice[]>();
const devicesProblem = ref('');
async function loadDevices() {
  if (!syncing.value) return (devices.value = undefined);
  try {
    devices.value = await syncDevices();
    devicesProblem.value = '';
  } catch (reason) {
    devicesProblem.value = sentence(String(reason));
  }
}
watch(
  syncing,
  () => {
    problem.value = '';
    void loadDevices();
  },
  { immediate: true },
);
watch(
  () => pairing.value.phase,
  (phase) => {
    if (phase === 'added') void loadDevices();
    if (phase === 'joined') {
      joining.value = false;
      code.value = '';
    }
  },
);

const removing = ref<SyncDevice>();
const removeDevice = (device: SyncDevice) =>
  attempt(async () => {
    await removeSyncDevice(device.id);
    removing.value = undefined;
    await loadDevices();
  });

// Moving and leaving each ask once more before they do anything.
const moving = ref(false);
const newAddress = ref('');
const moveServer = () =>
  attempt(async () => {
    await moveSyncServer(newAddress.value, askInvite.value ? invite.value : undefined);
    moving.value = false;
    newAddress.value = '';
    note.value = 'Moved. Your other computers join the new group with Add a device here.';
    await loadDevices();
  });

const leaving = ref(false);
const leave = () =>
  attempt(async () => {
    const heard = await leaveSync();
    leaving.value = false;
    note.value = heard
      ? 'This computer has left sync. Everything on it stays here.'
      : "This computer has left sync, but the server couldn't be reached, so it still lists this computer until another one removes it.";
  });

const deviceLabel = (device: SyncDevice) => device.name ?? 'A device whose name can’t be read';

const ago = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
function since(at: number) {
  const minutes = Math.round((now.value - at) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return ago.format(-minutes, 'minute');
  const hours = Math.round(minutes / 60);
  if (hours < 24) return ago.format(-hours, 'hour');
  return ago.format(-Math.round(hours / 24), 'day');
}

const left = computed(() => {
  if (pairing.value.phase !== 'showing') return '';
  const seconds = Math.max(0, Math.round((pairing.value.expiresAt - now.value) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
});
</script>

<template>
  <section class="read" aria-labelledby="sync">
    <h2 id="sync" class="mono">{{ number }} — Sync</h2>

    <template v-if="status && !syncing">
      <div class="field">
        <div class="line">Use your own server</div>
        <label class="card box">
          <input
            v-model="address"
            placeholder="https://sync.example.com"
            aria-label="Your sync server's address"
            spellcheck="false"
            autocapitalize="off"
          />
        </label>
        <p class="help">
          Seiza syncs your computers through a small server you run yourself. Everything is encrypted here first, so it
          only sees when and how much.
          <a class="mono" :href="serverGuide" @click.prevent="openUrl(serverGuide)">How to run one ↗</a>
        </p>
      </div>
      <div class="field">
        <div class="line">This computer's name</div>
        <label class="card box">
          <input v-model="deviceName" :maxlength="100" aria-label="This computer's name" />
        </label>
        <p class="help">How your other computers list this one.</p>
      </div>
      <div v-if="askInvite" class="field">
        <div class="line">Invite code</div>
        <label class="card box">
          <input v-model="invite" aria-label="Invite code" spellcheck="false" autocapitalize="off" />
        </label>
        <p class="help">This server asks for one to start a group. Its owner makes them with seiza-sync invite.</p>
      </div>

      <div v-if="pairing.phase === 'checking' || pairing.phase === 'waiting'" class="field">
        <p class="mono status-line" role="status">
          <span class="dot" aria-hidden="true" />
          {{ pairing.phase === 'checking' ? 'Checking the code…' : 'On your other computer, choose Add' }}
        </p>
        <div class="keys">
          <button class="key" type="button" @click="stopPairing()">Stop</button>
        </div>
      </div>
      <div v-else-if="confirmingSky" class="field">
        <p class="help note">
          This computer has sessions of its own. Joining takes your group's sky, so their rare events and comets are
          drawn again, once. Stars, constellations and planets stay as they are.
        </p>
        <div class="keys">
          <button class="key next" type="button" :disabled="busy" @click="join">Join and match the sky</button>
          <button class="key" type="button" @click="cancelJoin">Cancel</button>
        </div>
      </div>
      <div v-else-if="joining" class="field">
        <div class="line">Pairing code</div>
        <form class="row" @submit.prevent="join">
          <label class="card box">
            <input
              v-model="code"
              placeholder="4821-orbit-lamp-quote-river"
              aria-label="Pairing code"
              spellcheck="false"
              autocapitalize="off"
            />
          </label>
          <button class="key next" type="submit" :disabled="busy || !!joinNeeds">Join</button>
          <button class="key" type="button" @click="cancelJoin">Cancel</button>
        </form>
        <p v-if="joinNeeds" class="help note">{{ joinNeeds }}</p>
        <p class="help">On a computer that already syncs, choose Add a device in Settings › Sync for a code.</p>
      </div>
      <div v-else class="field">
        <div class="keys">
          <button class="key next" type="button" :disabled="busy || !address.trim()" @click="startGroup">
            Start a new sync group
          </button>
          <button class="key" type="button" :disabled="busy" @click="joining = true">Join with a pairing code</button>
        </div>
        <p class="help">
          Start a group on your first computer, then join it from the others.
          {{ address.trim() ? '' : "Both need your sync server's address above." }}
        </p>
      </div>
      <p v-if="pairing.phase === 'failed'" class="help problem" role="alert">{{ sentence(pairing.problem) }}</p>
    </template>

    <template v-else-if="status && syncing">
      <div class="field">
        <p class="mono status-line">
          <span class="dot" aria-hidden="true" />
          <span class="server">Syncing with {{ status.server }}</span>
        </p>
        <p class="help">
          {{ status.lastSyncedAt ? `Last synced ${since(status.lastSyncedAt)}.` : 'Not synced yet.' }}
        </p>
        <p v-if="status.problem" class="help problem" role="alert">{{ sentence(status.problem) }}</p>
      </div>

      <div class="field">
        <div class="line">Devices</div>
        <ul v-if="devices" class="devices">
          <li v-for="device in devices" :key="device.id">
            <span class="name"
              >{{ deviceLabel(device) }}
              <span class="faint mono">{{
                device.thisDevice ? '· this computer' : `· seen ${since(device.lastSeenAt)}`
              }}</span></span
            >
            <template v-if="!device.thisDevice">
              <span v-if="removing?.id === device.id" class="buttons">
                <button class="key next" type="button" :disabled="busy" @click="removeDevice(device)">Remove</button>
                <button class="key" type="button" @click="removing = undefined">Cancel</button>
              </span>
              <button
                v-else
                class="key"
                type="button"
                :aria-label="`Remove ${deviceLabel(device)}`"
                @click="removing = device"
              >
                Remove
              </button>
            </template>
          </li>
        </ul>
        <p v-if="removing" class="help">
          {{ deviceLabel(removing) }} stops syncing, and what's on it stays there. For a lost or old computer.
        </p>
        <p v-if="devicesProblem" class="help problem" role="alert">{{ devicesProblem }}</p>
      </div>

      <div class="field">
        <template v-if="pairing.phase === 'showing'">
          <div class="line">Type this on the new computer</div>
          <p class="code mono" aria-live="polite">{{ pairing.code }}</p>
          <p class="help">
            In its Settings › Sync, choose Join with a pairing code. The code works once, for {{ left }} more.
          </p>
          <div class="keys">
            <button class="key" type="button" @click="stopPairing()">Stop</button>
          </div>
        </template>
        <template v-else-if="pairing.phase === 'asking'">
          <div class="line">Add {{ pairing.deviceName }}?</div>
          <p class="help">It typed the code right. Once added, it gets everything this group syncs.</p>
          <div class="keys">
            <button class="key next" type="button" @click="answerSyncDevice(true)">Add</button>
            <button class="key" type="button" @click="answerSyncDevice(false)">Don't add</button>
          </div>
        </template>
        <template v-else>
          <div class="line">
            Another computer
            <button
              class="key"
              type="button"
              :disabled="busy"
              @click="attempt(async () => void (await addSyncDevice()))"
            >
              Add a device
            </button>
          </div>
          <p v-if="pairing.phase === 'added'" class="help note" role="status">{{ pairing.deviceName }} was added.</p>
          <p v-else-if="pairing.phase === 'failed'" class="help problem" role="alert">
            {{ sentence(pairing.problem) }}
          </p>
          <p v-else class="help">Shows a short code to type on your other computer.</p>
        </template>
      </div>

      <div class="field">
        <template v-if="moving">
          <div class="line">Move to another server</div>
          <form class="row" @submit.prevent="moveServer">
            <label class="card box">
              <input
                v-model="newAddress"
                placeholder="https://sync.example.com"
                aria-label="The new server's address"
                spellcheck="false"
                autocapitalize="off"
              />
            </label>
            <button class="key next" type="submit" :disabled="busy || !newAddress.trim()">Move</button>
            <button class="key" type="button" @click="moving = false">Cancel</button>
          </form>
          <label v-if="askInvite" class="card box">
            <input v-model="invite" placeholder="Invite code" aria-label="Invite code" spellcheck="false" />
          </label>
          <p class="help">
            Starts a new group there, sends everything on this computer to it, and leaves this one. Your other computers
            then join it with Add a device here.
          </p>
        </template>
        <div v-else class="line">
          Move to another server
          <button class="key" type="button" :disabled="busy" @click="moving = true">Move…</button>
        </div>
      </div>

      <div class="field">
        <template v-if="leaving">
          <div class="line">Leave sync?</div>
          <p class="help">This computer stops syncing and keeps everything on it. The others carry on.</p>
          <div class="keys">
            <button class="key next" type="button" :disabled="busy" @click="leave">Leave sync</button>
            <button class="key" type="button" @click="leaving = false">Cancel</button>
          </div>
        </template>
        <div v-else class="line">
          Stop syncing this computer
          <button class="key" type="button" :disabled="busy" @click="leaving = true">Leave sync…</button>
        </div>
      </div>
    </template>

    <p v-if="note" class="help note" role="status">{{ note }}</p>
    <p v-if="problem" class="help problem" role="alert">{{ problem }}</p>
  </section>
</template>

<style scoped>
/* Laid out like the other sections in Settings (SettingsView.vue). */
.read {
  padding: 22px 0 4px;
}

h2 {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 0 4px;
  color: var(--dim);
  font-weight: 400;
}

h2::after {
  content: '';
  flex: 1;
  height: 1px;
  background: linear-gradient(90deg, var(--tick), transparent);
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

.help a {
  color: var(--dim);
  text-decoration: underline;
  text-decoration-color: var(--tick);
  text-underline-offset: 3px;
}

.note {
  color: var(--dim);
}

.problem {
  margin-bottom: 10px;
  color: var(--glint);
}

.status-line {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 2px 0 0;
}

.server {
  min-width: 0;
  text-transform: none;
  overflow-wrap: anywhere;
}

.keys,
.buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.keys .next,
.buttons .next {
  color: var(--starlight);
  border-color: var(--tick);
}

.key:disabled {
  opacity: 0.4;
  cursor: default;
}

/* An input can't draw the frame's corner ticks itself, so a label around it does. */
.box {
  display: flex;
  flex: 1;
  min-width: 0;
}

.box:focus-within {
  --t: var(--tick-bright);
}

.box input {
  flex: 1;
  min-width: 0;
  padding: 8px 10px;
  border: 0;
  outline: 0;
  background: none;
  color: var(--starlight);
  font: 12px var(--mono);
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.devices {
  margin: 0;
  padding: 0;
  list-style: none;
}

.devices li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 0;
  border-bottom: 1px solid var(--line);
  font-size: 12.5px;
}

.name {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}

/* The pairing code, big enough to read across a desk. It keeps its own lowercase. */
.code {
  margin: 4px 0;
  color: var(--starlight);
  font-size: 17px;
  letter-spacing: 0.04em;
  text-transform: none;
  user-select: all;
}
</style>
