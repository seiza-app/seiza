<script lang="ts" setup>
import { maxProfileNameLength, starColorFor } from '@lock-in/core';
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import SeizaWordmark from '@/components/SeizaWordmark.vue';
import StarColorPicker from '@/components/StarColorPicker.vue';
import NoSessionSection from './components/NoSessionSection.vue';
import RedirectSection from './components/RedirectSection.vue';
import SitesSection from './components/SitesSection.vue';
import TourGuide, { type TourStep } from './components/TourGuide.vue';

const profileList = useStorageItem(profiles);
const session = useStorageItem(activeSession);

const selectedId = ref<string>();
watch(
  profileList,
  (list) => {
    if (list && !list.some((p) => p.id === selectedId.value)) selectedId.value = list[0]?.id;
  },
  { immediate: true },
);

const profile = computed(() => profileList.value?.find((p) => p.id === selectedId.value));
const position = computed(() => profileList.value?.findIndex((p) => p.id === selectedId.value) ?? -1);
const isRunning = computed(() => !!profile.value && session.value?.profileId === profile.value.id);
const canDelete = computed(() => !isRunning.value && (profileList.value?.length ?? 0) > 1);

const nameInput = ref<HTMLInputElement>();
const confirmingDelete = ref(false);
const confirmingReset = ref(false);
watch(selectedId, () => (confirmingDelete.value = false));

async function select(id: string, focusName = false) {
  selectedId.value = id;
  if (!focusName) return;
  await nextTick();
  nameInput.value?.select();
}

async function rename(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!profile.value) return;
  await renameProfile(profile.value.id, input.value);
  // A blank name is ignored, so put the saved one back in the field.
  input.value = profile.value.name;
}

async function remove() {
  if (!profile.value) return;
  await deleteProfile(profile.value.id);
  confirmingDelete.value = false;
}

async function reset() {
  await resetStarterProfiles();
  confirmingReset.value = false;
}

// Manage mode: tick several profiles and delete them in one go.
const managing = ref(false);
const picked = ref<string[]>([]);
const confirmingBulk = ref(false);
const deletable = computed(() => (profileList.value ?? []).filter((p) => p.id !== session.value?.profileId));
// At least one profile always stays.
const keepsOne = computed(() => picked.value.length < (profileList.value?.length ?? 0));

function startManaging() {
  managing.value = true;
  picked.value = [];
  confirmingBulk.value = false;
}

async function deletePicked() {
  await deleteProfiles(picked.value);
  picked.value = [];
  confirmingBulk.value = false;
}

// The tour for new users (PLAN Phase 5). The popup offers it once and opens this page at #tour;
// after that it's here, at the top.
const touring = ref<number>();
const tourSteps: TourStep[] = [
  {
    target: 'profiles',
    title: 'Profiles',
    text: "A profile is a set of rules for a focus session. Start one from Seiza's button in your browser's toolbar (pin it to keep it a click away), and pick one here to change its rules.",
  },
  {
    target: 'sites',
    title: 'Sites',
    text: 'Each site gets a level for this profile. Allowed leaves it alone, Tamed hides the distracting parts, Redirected sends you to a Seiza page first, and Restricted keeps it closed until the session ends.',
  },
  {
    target: 'redirect',
    title: 'The redirect page',
    text: 'A redirected site shows a Seiza page first: focus for a few minutes, and if you still want the site after that, go right ahead. Your if-then plan, written here, shows on that page.',
  },
  {
    title: 'The Seiza app',
    text: 'With the Seiza app on your computer too, the extension links to it by itself, and the popup says App linked. Sessions and profiles stay in step, and the app adds a timer, a sky of your focus, and satellites: a hotkey to set a passing thought aside from any app. The tour is at the top of this page whenever you want it again.',
  },
];
function takeTour() {
  managing.value = false;
  touring.value = 0;
  void tourOffered.setValue(true);
}
onMounted(() => {
  if (location.hash !== '#tour') return;
  history.replaceState(null, '', location.pathname);
  takeTour();
});
</script>

<template>
  <main>
    <header>
      <p class="brand"><SeizaWordmark /></p>
      <h1>Settings</h1>
      <p class="help intro">
        A profile is a set of rules for a focus session: which sites stay as they are, which are tamed, and which are
        redirected or restricted. Pick a profile to edit it. Changes save as you go, and apply straight away if the
        profile is running.
      </p>
      <button type="button" class="key lit take-tour" @click="takeTour">Take the tour</button>
    </header>

    <section v-if="profileList && profile">
      <div v-if="!managing" class="profiles" data-tour="profiles">
        <div class="tabs" role="tablist" aria-label="Profiles">
          <button
            v-for="p in profileList"
            :key="p.id"
            type="button"
            role="tab"
            :aria-selected="p.id === selectedId"
            @click="select(p.id)"
          >
            {{ p.name }}
            <span v-if="session?.profileId === p.id" class="running" title="Running now" aria-label="(running now)" />
          </button>
        </div>
        <button type="button" class="key new" @click="createProfile().then((id) => select(id, true))">
          + New profile
        </button>
        <button v-if="!managing" type="button" class="key" @click="startManaging">Manage</button>
      </div>

      <div v-if="managing" class="manage">
        <h2>Manage profiles</h2>
        <p class="help">
          Tick the profiles you don't use any more to delete them together. The running profile can't be deleted, and at
          least one profile always stays.
        </p>
        <ul class="list items">
          <li v-for="p in profileList" :key="p.id">
            <label class="pick">
              <input
                v-model="picked"
                type="checkbox"
                :value="p.id"
                :disabled="!deletable.includes(p)"
                :aria-label="`Select ${p.name}`"
              />
              <span>
                <span class="name">{{ p.name }}</span>
                <span class="detail">{{ deletable.includes(p) ? profileSummary(p) : 'Running now' }}</span>
              </span>
            </label>
          </li>
        </ul>
        <div v-if="!confirmingBulk" class="actions">
          <button type="button" class="text-button" @click="picked = deletable.map((p) => p.id)">Select all</button>
          <button type="button" class="text-button" :disabled="!picked.length" @click="picked = []">Clear</button>
          <button
            type="button"
            class="text-button danger"
            :disabled="!picked.length || !keepsOne"
            :title="keepsOne ? undefined : 'At least one profile has to stay'"
            @click="confirmingBulk = true"
          >
            Delete {{ picked.length || '' }}
          </button>
          <button type="button" class="text-button done" @click="managing = false">Done</button>
        </div>
        <div v-else class="actions confirm" role="alert">
          <span>Delete {{ picked.length }} profile{{ picked.length === 1 ? '' : 's' }}? This can't be undone.</span>
          <button type="button" class="text-button" @click="confirmingBulk = false">Cancel</button>
          <button type="button" class="text-button danger confirm-delete" @click="deletePicked">Delete</button>
        </div>
        <p v-if="!keepsOne" class="help small">At least one profile has to stay. Untick one to delete the rest.</p>
      </div>

      <div v-if="!managing" class="profile">
        <label class="name-field">
          <span>Name</span>
          <input
            ref="nameInput"
            :key="profile.id"
            class="field"
            :value="profile.name"
            :maxlength="maxProfileNameLength"
            aria-label="Profile name"
            @change="rename"
            @keydown.enter="($event.target as HTMLInputElement).blur()"
          />
        </label>
        <div class="star-field">
          <span aria-hidden="true">Star</span>
          <StarColorPicker
            :color="starColorFor(profile.id, profile.starColor)"
            :profile-name="profile.name"
            @update:color="(color: string) => setStarColor(profile!.id, color)"
          />
        </div>

        <div v-if="!confirmingDelete" class="actions">
          <button type="button" class="text-button" :disabled="position <= 0" @click="moveProfile(profile.id, -1)">
            Move earlier
          </button>
          <button
            type="button"
            class="text-button"
            :disabled="position >= profileList.length - 1"
            @click="moveProfile(profile.id, 1)"
          >
            Move later
          </button>
          <button
            type="button"
            class="text-button"
            @click="duplicateProfile(profile.id).then((id) => select(id, true))"
          >
            Duplicate
          </button>
          <button
            type="button"
            class="text-button danger"
            :disabled="!canDelete"
            :title="isRunning ? 'End the session to delete this profile' : undefined"
            @click="confirmingDelete = true"
          >
            Delete
          </button>
        </div>
        <div v-else class="actions confirm" role="alert">
          <span>Delete {{ profile.name }}?</span>
          <button type="button" class="text-button" @click="confirmingDelete = false">Cancel</button>
          <button type="button" class="text-button danger confirm-delete" @click="remove">Delete</button>
        </div>
      </div>

      <template v-if="!managing">
        <p v-if="isRunning" class="help small">{{ profile.name }} is running now, so changes apply right away.</p>

        <SitesSection :profile="profile" />
        <RedirectSection :profile="profile" />
      </template>
    </section>

    <section v-if="profileList && !managing" class="no-session">
      <NoSessionSection />
    </section>

    <footer>
      <template v-if="!confirmingReset">
        <button type="button" class="text-button reset" @click="confirmingReset = true">Reset starter profiles</button>
        <p class="help small">
          Puts Writing, Deep Work and Wind Down back the way they shipped. Your own profiles are kept.
        </p>
      </template>
      <div v-else class="actions confirm" role="alert">
        <span>Reset the starter profiles? Changes you made to them will be lost.</span>
        <button type="button" class="text-button" @click="confirmingReset = false">Cancel</button>
        <button type="button" class="text-button danger confirm-reset" @click="reset">Reset</button>
      </div>
      <!-- The one place the extension asks for support, like the app's Settings (PLAN.md §8). -->
      <p class="support">
        <a class="mono" href="https://ko-fi.com/sortelone" target="_blank" rel="noopener noreferrer"
          >Support Seiza on Ko-fi ↗</a
        >
      </p>
    </footer>
    <TourGuide v-if="touring !== undefined" v-model:step="touring" :steps="tourSteps" @end="touring = undefined" />
  </main>
</template>

<style scoped>
main {
  max-width: 600px;
  margin: 0 auto;
  padding: 48px 20px 96px;
}

.brand {
  margin: 0;
  font-size: 24px;
}

h1 {
  margin: 10px 0 8px;
  font-size: 30px;
  font-weight: 200;
  letter-spacing: 0.01em;
}

.intro {
  margin-bottom: 8px;
}

.take-tour {
  margin: 4px 0 24px;
}

.manage {
  margin-top: 8px;
}

.pick {
  display: flex;
  flex: 1;
  align-items: flex-start;
  gap: 10px;
  cursor: pointer;
}

.pick input {
  margin-top: 3px;
  accent-color: var(--glow);
}

.manage .actions {
  margin-top: 8px;
}

.done {
  margin-left: auto;
  color: var(--glow);
}

.profiles {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

/* The profiles, as a segmented switch like the app's, the picked one lit from below. */
.tabs {
  display: flex;
  flex: 1 1 auto;
  flex-wrap: wrap;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: var(--sky-raised);
}

.tabs button {
  flex: 1;
  padding: 9px 12px;
  border-radius: 2px;
  background: none;
  color: var(--faint);
  font-size: 13px;
  white-space: nowrap;
  transition:
    background 300ms,
    color 300ms,
    box-shadow 300ms;
}

.tabs button:hover {
  color: var(--dim);
}

.tabs button[aria-selected='true'] {
  background: var(--sky-hover);
  color: var(--starlight);
  box-shadow: inset 0 -1px 0 #cfe0ff;
}

.running {
  display: inline-block;
  width: 5px;
  height: 5px;
  margin-left: 6px;
  border-radius: 50%;
  background: var(--glow);
  box-shadow: 0 0 6px var(--glow);
  vertical-align: middle;
}

.new,
.profiles > .key {
  padding: 7px 10px;
}

.profile {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 16px;
  margin-top: 16px;
}

.name-field {
  display: flex;
  flex: 1 1 220px;
  align-items: center;
  gap: 10px;
  color: var(--dim);
}

.star-field {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--dim);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px;
}

.confirm span {
  margin-right: 6px;
  color: var(--starlight);
}

footer {
  margin-top: 48px;
  padding-top: 16px;
  border-top: 1px solid var(--line);
}

.reset {
  margin-left: -8px;
}

footer .help.small {
  margin-top: 4px;
}

/* Quiet, at the very bottom: no nags anywhere else. */
.support {
  margin: 40px 0 0;
  text-align: center;
}

.support a {
  color: var(--faint);
  text-decoration: underline;
  text-decoration-color: var(--line-bright);
  text-underline-offset: 3px;
  transition: color 300ms;
}

.support a:hover,
.support a:focus-visible {
  color: var(--starlight);
}
</style>
