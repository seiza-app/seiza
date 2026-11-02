<script lang="ts" setup>
import {
  endLock,
  findRule,
  hasRoomForOwnSite,
  maxOwnSites,
  otherSitesLevel,
  ownSiteDomain,
  ownSiteRecipe,
  resolveToggles,
  siteFor,
  siteLevels,
  type OtherSitesLevel,
  type Profile,
  type SiteLevel,
  type SiteRecipe,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';
import { computed, ref } from 'vue';
import LevelPicker from '@/components/LevelPicker.vue';
import RecipePreview from '@/components/preview/RecipePreview.vue';
import ToggleSwitch from '@/components/ToggleSwitch.vue';
import PageRules from './PageRules.vue';

const props = defineProps<{ profile: Profile }>();

// The built-in sites, then the ones added to this profile, by their address.
const ownSites = computed(() =>
  props.profile.sites.flatMap((site) => {
    const domain = ownSiteDomain(site.recipeId);
    return domain ? [ownSiteRecipe(domain)] : [];
  }),
);
const listed = computed(() => [...recipes, ...ownSites.value]);

// An added site needs the browser's OK before Seiza can reach it. One added in the desktop app
// hasn't had it yet.
const reach = useReachable(() => ownSites.value.map((site) => site.name));
const needsReach = (recipe: SiteRecipe) => !!ownSiteDomain(recipe.id) && !!reach.value && !reach.value.has(recipe.name);

// What happens to every site that isn't listed. Built-in sites are always listed, and one this
// profile has no rule for follows it too, so that's the level it shows.
const others = computed(() => otherSitesLevel(props.profile));
const levelOf = (recipe: SiteRecipe) => findRule(props.profile, recipe.id)?.level ?? others.value;

// Keeping them away needs the browser's OK to reach every site, asked for only then.
const reachesEverySite = useReachesEverySite();
const othersNeedReach = computed(() => others.value !== 'allowed' && reachesEverySite.value === false);
function setOthers(level: SiteLevel) {
  if (level === 'tamed') return;
  // The browser only asks straight from a click, so this comes first.
  if (level !== 'allowed' && !reachesEverySite.value) void askToReachEverySite();
  void setOtherSites(props.profile.id, level);
}
const othersDetail: Record<OtherSitesLevel, string> = {
  allowed: "Every site that isn't in the list is left alone. Restrict them to let through only the sites you allow.",
  redirected:
    "Every site that isn't in the list sends you to Seiza's page first, with a way through for a few minutes. Addresses on this computer or your network always load.",
  restricted:
    "Only the sites in the list that aren't restricted get through. Every other site is shut until the session ends. Addresses on this computer or your network always load.",
};

function setLevel(recipe: SiteRecipe, level: SiteLevel) {
  // The browser only asks straight from a click, so this comes first.
  if (level !== 'allowed' && needsReach(recipe)) void askToReach([recipe.name]);
  void setSiteLevel(props.profile.id, recipe.id, level);
}

// While a Hard session of this profile runs, sites can only get stricter.
const session = useStorageItem(activeSession);
const locked = computed(
  () => session.value?.profileId === props.profile.id && endLock(session.value, Date.now()) === 'hard',
);

// Adding a site by its address. One that's built in is already listed.
const draft = ref('');
const found = computed(() => (draft.value.trim() ? siteFor(recipes, draft.value) : undefined));
const problem = computed(() => {
  if (!draft.value.trim()) return '';
  const site = found.value;
  if (!site) return "Use a site's address, like example.com.";
  if (!ownSiteDomain(site.id)) return `${site.name} is built in, so it's in the list already.`;
  if (findRule(props.profile, site.id)) return `${site.name} is in the list already.`;
  if (!hasRoomForOwnSite(props.profile)) return `A profile can have up to ${maxOwnSites} sites of its own.`;
  return '';
});
// A site added while every other site is kept away is one to let through.
const addedLevel = computed(() => (others.value === 'allowed' ? 'redirected' : 'allowed'));
function add() {
  const site = found.value;
  if (!site || problem.value) return;
  if (addedLevel.value !== 'allowed') void askToReach([site.name]);
  void setSiteLevel(props.profile.id, site.id, addedLevel.value);
  draft.value = '';
}

// Toggles matter whenever any part of the site is tamed, even if only one page is.
const showsToggles = (recipeId: string) => {
  const rule = findRule(props.profile, recipeId);
  return rule?.level === 'tamed' || !!rule?.paths?.some((page) => page.level === 'tamed');
};

// The toggle being pointed at or focused, so the preview can outline what it hides.
const pointing = ref<{ recipeId: string; toggleId: string }>();
const point = (recipeId: string, toggleId?: string) => {
  pointing.value = toggleId ? { recipeId, toggleId } : undefined;
};
</script>

<template>
  <h2>Sites</h2>
  <p class="help">
    What happens to each site while {{ profile.name }} is running. Page rules give part of a site its own level, like
    redirecting YouTube Shorts while the rest of YouTube is tamed. Any other site can be added by its address at the
    bottom, and Every other site sets the rest, so a profile can let through only the sites it allows.
  </p>
  <dl class="card legend">
    <div v-for="level in siteLevels" :key="level.id">
      <dt>{{ level.label }}</dt>
      <dd>
        {{
          level.id === 'tamed'
            ? `${level.detail} You pick what's hidden under each site, or on the page itself from Seiza's popup. With no session running, a site can be tamed all the time, below.`
            : level.detail
        }}
      </dd>
    </div>
  </dl>
  <p v-if="locked" class="help small">While this Hard session runs, sites can only get stricter.</p>
  <ul class="list sites">
    <li v-for="(recipe, i) in listed" :key="recipe.id">
      <!-- The tour points at the first site's levels. -->
      <div class="site" :data-tour="i === 0 ? 'sites' : undefined">
        <span class="name">
          {{ recipe.name }}
          <button
            v-if="ownSiteDomain(recipe.id)"
            type="button"
            class="text-button danger"
            :aria-label="`Remove ${recipe.name}`"
            @click="removeSite(profile.id, recipe.id)"
          >
            Remove
          </button>
        </span>
        <LevelPicker
          :site-name="recipe.name"
          :can-tame="recipe.toggles.length > 0 || !!ownSiteDomain(recipe.id)"
          :model-value="levelOf(recipe)"
          @update:model-value="setLevel(recipe, $event)"
        />
      </div>
      <p v-if="needsReach(recipe)" class="reach">
        <span>Seiza can't reach {{ recipe.name }} yet, so it's left alone for now.</span>
        <button type="button" class="text-button" @click="askToReach([recipe.name])">Let Seiza reach it</button>
      </p>
      <PageRules :profile="profile" :recipe="recipe" />
      <div v-if="showsToggles(recipe.id)" class="toggles">
        <RecipePreview
          :recipe-id="recipe.id"
          :site-name="recipe.name"
          :values="resolveToggles(recipe, findRule(profile, recipe.id))"
          :highlight="pointing?.recipeId === recipe.id ? pointing.toggleId : undefined"
        />
        <ToggleSwitch
          v-for="toggle in recipe.toggles"
          :key="toggle.id"
          :label="toggle.label"
          :model-value="resolveToggles(recipe, findRule(profile, recipe.id))[toggle.id] ?? false"
          @update:model-value="setProfileToggle(profile.id, recipe.id, toggle.id, $event)"
          @mouseenter="point(recipe.id, toggle.id)"
          @mouseleave="point(recipe.id)"
          @focus="point(recipe.id, toggle.id)"
          @blur="point(recipe.id)"
        />
      </div>
    </li>
    <li class="others">
      <div class="site">
        <span class="name">Every other site</span>
        <LevelPicker
          site-name="Every other site"
          :can-tame="false"
          tame-detail="There's nothing to tame on sites that aren't listed."
          :model-value="others"
          @update:model-value="setOthers"
        />
      </div>
      <p class="detail">{{ othersDetail[others] }}</p>
      <p v-if="othersNeedReach" class="reach">
        <span>Seiza can't reach other sites yet, so they're left alone for now.</span>
        <button type="button" class="text-button" @click="askToReachEverySite()">Let Seiza reach every site</button>
      </p>
    </li>
  </ul>
  <form class="add" @submit.prevent="add">
    <input
      v-model="draft"
      maxlength="253"
      aria-label="Add a site"
      placeholder="Add a site, e.g. news.ycombinator.com"
    />
    <button type="submit" :disabled="!draft.trim() || !!problem">Add</button>
    <p v-if="problem" class="error">{{ problem }}</p>
  </form>
  <p class="help small">
    Added sites start out redirected, or allowed while every other site is kept away. Your browser asks to let Seiza
    reach each one (or every site, to keep away the ones that aren't listed), and Seiza only uses that to redirect or
    restrict them during a session.
  </p>
</template>

<style scoped>
/* What each level does, above the list. */
.legend {
  display: grid;
  gap: 8px;
  margin: 0 0 16px;
  padding: 14px 16px;
}

.legend div {
  display: grid;
  grid-template-columns: 96px 1fr;
  gap: 12px;
}

.legend dt {
  padding-top: 2px;
  color: var(--dim);
  font: 10px/1.5 var(--mono);
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.legend dd {
  margin: 0;
  color: var(--faint);
  font-size: 13px;
}

.sites {
  gap: 8px;
}

.sites li {
  padding: 14px 16px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: var(--sky-raised);
}

.site {
  display: grid;
  grid-template-columns: 1fr minmax(0, 340px);
  align-items: center;
  gap: 12px;
}

.toggles {
  margin-top: 6px;
  padding: 2px 0 0 12px;
  border-left: 1px solid var(--line);
}

.name .text-button {
  margin-left: 4px;
}

/* Every other site closes the list, set a little apart from the sites above it. */
.others {
  border-style: dashed;
}

.others .detail {
  margin: 8px 0 0;
  color: var(--faint);
  font-size: 12px;
}

/* A site Seiza can't reach yet, with the way to let it. */
.reach {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 4px 8px;
  margin: 6px 0 0;
  color: var(--glow);
  font-size: 12px;
}

.add .error {
  flex-basis: 100%;
  margin: 0;
}

@media (max-width: 480px) {
  .site {
    grid-template-columns: 1fr;
    gap: 6px;
  }
}
</style>
