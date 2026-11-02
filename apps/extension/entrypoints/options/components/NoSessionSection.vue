<script lang="ts" setup>
import {
  allTheTimeToggles,
  endLock,
  ownSiteDomain,
  ownSiteRecipe,
  siteSettingsFor,
  type SiteRecipe,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';
import { computed } from 'vue';
import RecipePreview from '@/components/preview/RecipePreview.vue';
import ToggleSwitch from '@/components/ToggleSwitch.vue';
import HiddenParts from './HiddenParts.vue';

// How sites look with no session running, for every profile: any site can be tamed all the time,
// with toggles of its own. The parts of a page picked to hide (from the popup, on the site) are
// listed here too, since they're hidden whenever the site is tamed, in a session or not.

const sites = useStorageItem(siteSettings);
const profileList = useStorageItem(profiles);
const session = useStorageItem(activeSession);
const locked = computed(() => !!session.value && endLock(session.value, Date.now()) === 'hard');

// The built-in sites, then any added by address to a profile or here.
const listed = computed(() => {
  const ids = [
    ...(profileList.value ?? []).flatMap((p) => p.sites.map((site) => site.recipeId)),
    ...(sites.value ?? []).map((site) => site.recipeId),
  ];
  const own = [...new Set(ids.flatMap((id) => ownSiteDomain(id) ?? []))].map(ownSiteRecipe);
  return [...recipes, ...own];
});

const settingsOf = (recipe: SiteRecipe) => siteSettingsFor(sites.value ?? [], recipe.id);

// An added site needs the browser's OK before Seiza can reach it.
const reach = useReachable(() => listed.value.filter((r) => ownSiteDomain(r.id)).map((r) => r.name));
const needsReach = (recipe: SiteRecipe) => !!ownSiteDomain(recipe.id) && !!reach.value && !reach.value.has(recipe.name);
function setAlways(recipe: SiteRecipe, on: boolean) {
  // The browser only asks straight from a click, so this comes first.
  if (on && needsReach(recipe)) void askToReach([recipe.name]);
  void setAllTheTime(recipe.id, on);
}
</script>

<template>
  <h2>No session</h2>
  <p class="help">
    How sites look when no session is running, whichever profile you use. Tame a site all the time to keep its feeds and
    the like hidden every day; in a session, its profile's settings apply instead. Parts you hide on a page, from
    Seiza's popup on the site, are hidden whenever it's tamed, in a session too.
  </p>
  <ul v-if="sites" class="list sites">
    <li v-for="recipe in listed" :key="recipe.id">
      <ToggleSwitch
        :label="`Tame ${recipe.name} all the time`"
        :model-value="!!settingsOf(recipe)?.allTheTime"
        @update:model-value="setAlways(recipe, $event)"
      />
      <template v-if="settingsOf(recipe)?.allTheTime">
        <p class="help small">{{ allTheTimeNote(recipe.name, true) }}</p>
        <p v-if="needsReach(recipe)" class="reach">
          <span>Seiza can't reach {{ recipe.name }} yet, so it's left alone for now.</span>
          <button type="button" class="text-button" @click="askToReach([recipe.name])">Let Seiza reach it</button>
        </p>
        <div v-if="recipe.toggles.length" class="toggles">
          <RecipePreview
            :recipe-id="recipe.id"
            :site-name="recipe.name"
            :values="allTheTimeToggles(recipe, settingsOf(recipe))"
          />
          <ToggleSwitch
            v-for="toggle in recipe.toggles"
            :key="toggle.id"
            :label="toggle.label"
            :model-value="allTheTimeToggles(recipe, settingsOf(recipe))[toggle.id] ?? false"
            @update:model-value="setAllTheTimeToggle(recipe.id, toggle.id, $event)"
          />
        </div>
      </template>
      <HiddenParts :recipe="recipe" :locked="locked" />
    </li>
  </ul>
  <p v-if="locked" class="help small">During this Hard session, parts can be hidden but not shown again.</p>
</template>

<style scoped>
.sites {
  gap: 8px;
}

.sites li {
  padding: 6px 16px 10px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: var(--sky-raised);
}

.sites li > .help {
  margin: 0;
}

.toggles {
  margin-top: 6px;
  padding: 2px 0 0 12px;
  border-left: 1px solid var(--line);
}

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
</style>
