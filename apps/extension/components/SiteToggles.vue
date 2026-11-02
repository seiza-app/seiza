<script lang="ts" setup>
import {
  allTheTimeToggles,
  findPathRule,
  findRule,
  isLooser,
  otherSitesLevel,
  ownSiteDomain,
  resolveToggles,
  sessionProfile,
  siteLevels,
  siteSettingsFor,
  type Profile,
  type Session,
  type SiteLevel,
  type SiteRecipe,
} from '@lock-in/core';
import { computed, ref, watch } from 'vue';
import LevelPicker from './LevelPicker.vue';
import ToggleSwitch from './ToggleSwitch.vue';

const props = defineProps<{
  recipe: SiteRecipe;
  profiles: Profile[];
  session: Session | null;
  /** The current tab's path, for page rules. */
  path: string;
  /** The current tab, reloaded once it's kept away so the change shows straight away. */
  tabId?: number;
}>();

// Two tabs, by when settings apply: with no session running, where a site can be tamed all the
// time, belonging to no profile; and in a session, where each profile has its own level for it.
// It opens on the one that applies right now.
const when = ref<'none' | 'session'>(props.session ? 'session' : 'none');

// Outside a session there's no "current" profile, so let the user pick which one to edit.
// Default to the first profile that has a rule for this site.
const editingId = ref(props.profiles.find((p) => findRule(p, props.recipe.id))?.id ?? props.profiles[0]?.id);
watch(
  () => props.profiles,
  (list) => {
    if (!list.some((p) => p.id === editingId.value)) editingId.value = list[0]?.id;
  },
);

const profile = computed(() =>
  props.session ? sessionProfile(props.profiles, props.session) : props.profiles.find((p) => p.id === editingId.value),
);
const rule = computed(() => findRule(profile.value, props.recipe.id));
// A site the profile doesn't list is treated like every other site.
const level = computed(() => rule.value?.level ?? otherSitesLevel(profile.value));
const pageRule = computed(() => findPathRule(rule.value?.paths, props.path));
// Toggles show whenever this page, or the site as a whole, is tamed.
const showsToggles = computed(
  () => (pageRule.value?.level ?? rule.value?.level) === 'tamed' || rule.value?.level === 'tamed',
);
const values = computed(() => resolveToggles(props.recipe, rule.value, props.session?.overrides?.[props.recipe.id]));
const passMinutesLeft = computed(() => {
  const until = props.session?.passes?.[props.recipe.id] ?? 0;
  return until > Date.now() ? Math.ceil((until - Date.now()) / 60_000) : 0;
});
const hasSessionChanges = computed(() => Object.keys(props.session?.overrides?.[props.recipe.id] ?? {}).length > 0);

// What's set for the site outside any profile: taming it all the time, and the parts of its page
// picked to hide, which are hidden whenever it's tamed, in either tab.
const sites = useStorageItem(siteSettings);
const settings = computed(() => siteSettingsFor(sites.value ?? [], props.recipe.id));
const allTheTime = computed(() => !!settings.value?.allTheTime);
const allTheTimeValues = computed(() => allTheTimeToggles(props.recipe, settings.value));
const hiddenCount = computed(() => settings.value?.hidden?.length ?? 0);

// A site someone added needs the browser's OK before Seiza can reach it. The browser only
// asks straight from a click, so asking comes before anything else in one.
const own = computed(() => !!ownSiteDomain(props.recipe.id));
const reach = useReachable(() => (own.value ? [props.recipe.name] : []));
const needsReach = computed(() => own.value && !!reach.value && !reach.value.has(props.recipe.name));
function askIfNeeded(level: SiteLevel) {
  if (level !== 'allowed' && needsReach.value) void askToReach([props.recipe.name]);
}

function setLevel(level: SiteLevel) {
  if (!profile.value) return;
  askIfNeeded(level);
  void setSiteLevel(profile.value.id, props.recipe.id, level);
}

function setAlways(on: boolean) {
  if (on) askIfNeeded('tamed');
  void setAllTheTime(props.recipe.id, on);
}

// What the picked level does, spelled out under the picker.
const levelDetail = computed(() => {
  if (level.value === 'tamed' && own.value) {
    return hiddenCount.value
      ? `Stays open, with the ${hiddenCount.value === 1 ? 'part' : `${hiddenCount.value} parts`} you picked hidden.`
      : `Stays open. Pick the parts of ${props.recipe.name} to hide below.`;
  }
  return siteLevels.find((l) => l.id === level.value)?.detail;
});

// The element picker hides parts of the page whenever the site is tamed. It runs on the page
// itself, through the content script, which a tab opened before Seiza could reach the site
// doesn't have yet, so it's added then.
const canPick = computed(
  () =>
    props.tabId !== undefined &&
    !needsReach.value &&
    (when.value === 'none' ? allTheTime.value : !!profile.value && level.value === 'tamed'),
);
async function pickParts() {
  if (props.tabId === undefined) return;
  const message = { type: 'lock-in:pick', recipeId: props.recipe.id };
  try {
    await browser.tabs.sendMessage(props.tabId, message);
  } catch {
    await browser.scripting.executeScript({ target: { tabId: props.tabId }, files: ['/content-scripts/content.js'] });
    await browser.tabs.sendMessage(props.tabId, message);
  }
  window.close();
}

// During a session, a site that's open can be kept away for the rest of it, and later ones.
const isOpen = computed(() => !isLooser('tamed', level.value));
const canKeepAway = computed(() => !!props.session && !!profile.value && !pageRule.value && isOpen.value);
async function keepAway(level: 'redirected' | 'restricted') {
  if (!profile.value) return;
  askIfNeeded(level);
  await setSiteLevel(profile.value.id, props.recipe.id, level);
  await syncRedirectsNow();
  if (props.tabId !== undefined) await browser.tabs.reload(props.tabId);
  window.close();
}

function set(toggleId: string, value: boolean) {
  if (props.session) {
    void setSessionToggle(props.recipe, toggleId, value);
  } else if (profile.value) {
    void setProfileToggle(profile.value.id, props.recipe.id, toggleId, value);
  }
}
</script>

<template>
  <section class="card site">
    <p class="mono heading">
      On this site · <b>{{ recipe.name }}</b>
    </p>

    <div class="when" role="tablist" :aria-label="`When ${recipe.name}'s settings apply`">
      <button
        v-for="option in [
          { id: 'none', label: 'No session', now: !session },
          { id: 'session', label: 'In a session', now: !!session },
        ] as const"
        :key="option.id"
        type="button"
        role="tab"
        :class="option.id"
        :aria-selected="when === option.id"
        @click="when = option.id"
      >
        <span v-if="option.now" class="dot" title="Applies right now" aria-label="(applies right now)" />
        {{ option.label }}
      </button>
    </div>

    <!-- Drawn once the site settings have loaded, so the switch doesn't slide into place. -->
    <template v-if="when === 'none' && sites">
      <ToggleSwitch
        class="all-the-time"
        :label="`Tame ${recipe.name} all the time`"
        :model-value="allTheTime"
        @update:model-value="setAlways"
      />
      <p class="note" :class="{ apart: allTheTime && recipe.toggles.length }">
        {{ allTheTimeNote(recipe.name, allTheTime) }}
      </p>
      <p v-if="needsReach && allTheTime" class="reach">
        <span>Seiza can't reach {{ recipe.name }} yet, so it's left alone for now.</span>
        <button type="button" class="key lit save" @click="askToReach([recipe.name])">Let Seiza reach it</button>
      </p>
      <ToggleSwitch
        v-for="toggle in allTheTime ? recipe.toggles : []"
        :key="toggle.id"
        :label="toggle.label"
        :model-value="allTheTimeValues[toggle.id] ?? false"
        @update:model-value="setAllTheTimeToggle(recipe.id, toggle.id, $event)"
      />
    </template>

    <template v-else>
      <label v-if="!session && profiles.length > 1" class="picker">
        <span>Editing</span>
        <select v-model="editingId">
          <option v-for="p in profiles" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </label>

      <!-- Levels can be changed between sessions. Mid-session loosening waits for strictness levels. -->
      <LevelPicker
        v-if="!session && profile"
        class="levels"
        :site-name="recipe.name"
        :can-tame="recipe.toggles.length > 0 || own"
        :model-value="level"
        @update:model-value="setLevel"
      />
      <p v-if="!session && profile" class="note detail">{{ levelDetail }}</p>

      <p v-if="pageRule" class="note">
        This page follows the {{ pageRule.path === '/' ? 'home page' : pageRule.path }} rule: {{ pageRule.level }}.
      </p>

      <template v-if="session && !pageRule">
        <p v-if="level === 'redirected'" class="note">
          {{ recipe.name }} is redirected in {{ profile?.name ?? 'this profile'
          }}<template v-if="passMinutesLeft"> · pass ends in {{ passMinutesLeft }} min</template>.
        </p>
        <p v-else-if="level === 'restricted'" class="note">
          {{ recipe.name }} is restricted in {{ profile?.name ?? 'this profile' }}, with no way through until the
          session ends.
        </p>
        <p v-else-if="level !== 'tamed'" class="note">
          {{ recipe.name }} {{ own ? 'is left alone' : "isn't tamed" }} in {{ profile?.name ?? 'this profile' }}.
        </p>
      </template>

      <div v-if="canKeepAway" class="keep-away">
        <span>Keep it away from now on</span>
        <button class="key" type="button" @click="keepAway('redirected')">Redirect it</button>
        <button class="key" type="button" @click="keepAway('restricted')">Restrict it</button>
      </div>

      <p v-if="needsReach && level !== 'allowed'" class="reach">
        <span>Seiza can't reach {{ recipe.name }} yet, so it's left alone for now.</span>
        <button type="button" class="key lit save" @click="askToReach([recipe.name])">Let Seiza reach it</button>
      </p>

      <ToggleSwitch
        v-for="toggle in showsToggles ? recipe.toggles : []"
        :key="toggle.id"
        :label="toggle.label"
        :model-value="values[toggle.id] ?? false"
        @update:model-value="set(toggle.id, $event)"
      />
    </template>

    <div v-if="canPick" class="pick">
      <button type="button" class="key" @click="pickParts">Hide parts of this page</button>
      <span>
        <template v-if="hiddenCount">{{ hiddenCount }} {{ hiddenCount === 1 ? 'part' : 'parts' }} hidden. </template>
        Hidden whenever {{ recipe.name }} is tamed, in either tab.
      </span>
    </div>

    <template v-if="when === 'session'">
      <p v-if="session && hasSessionChanges" class="footer">
        <span>Changed for this session</span>
        <button type="button" class="key lit save" @click="saveSessionChanges(recipe.id)">
          Save to {{ profile?.name }}
        </button>
      </p>
      <p v-else-if="!session" class="footer">
        <span>Applies whenever {{ profile?.name }} is running</span>
      </p>
      <p v-else-if="canKeepAway" class="footer">
        <span>Either one is saved to {{ profile?.name }}, for later sessions too</span>
      </p>
    </template>
  </section>
</template>

<style scoped>
/* The site the tab is on, in a frame of its own below the session. */
.site {
  margin-top: 14px;
  padding: 11px 12px 12px;
}

.heading {
  margin: 0 0 6px;
}

.picker {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 4px;
  color: var(--dim);
  font-size: 12px;
}

select {
  padding: 3px 6px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: rgb(8 11 22 / 0.7);
  color: var(--starlight);
  font: inherit;
}

.levels {
  margin: 8px 0 6px;
}

.detail {
  margin-bottom: 6px;
}

.note {
  margin: 0;
  line-height: 1.45;
  color: var(--dim);
  font-size: 12px;
}

.footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin: 10px 0 0;
  padding-top: 8px;
  border-top: 1px dashed var(--line);
  color: var(--faint);
  font-size: 11.5px;
}

/* Spelled out, two keys side by side, during a session on a site that's open. */
.keep-away {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  color: var(--dim);
  font-size: 12px;
}

.keep-away span {
  flex-basis: 100%;
}

.keep-away .key {
  flex: 1;
  padding: 7px 8px;
}

/* When the settings below apply: two tabs, like the level switch, the one shown lit. */
.when {
  display: grid;
  grid-template-columns: 1fr 1fr;
  margin: 2px 0 8px;
  padding: 2px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: rgb(8 11 22 / 0.55);
}

.when button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 7px 4px;
  border-radius: 2px;
  background: none;
  color: var(--faint);
  font: 9.5px var(--mono);
  letter-spacing: 0.08em;
  text-transform: uppercase;
  transition:
    color 300ms,
    background-color 300ms;
}

.when button:hover {
  color: var(--dim);
}

.when button[aria-selected='true'] {
  background: rgb(220 228 255 / 0.08);
  box-shadow: inset 0 -1px 0 #cfe0ff;
  color: var(--starlight);
}

/* Sets the switch apart from the site's own toggles below it. */
.apart {
  margin-bottom: 4px;
  padding-bottom: 8px;
  border-bottom: 1px dashed var(--line);
}

/* Opens the element picker on the page, under the toggles. */
.pick {
  display: grid;
  gap: 4px;
  margin-top: 8px;
  color: var(--faint);
  font-size: 11.5px;
}

.pick .key {
  padding: 7px 10px;
}

.reach {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 6px 8px;
  margin: 8px 0 0;
  color: var(--glow);
  font-size: 12px;
}
</style>
