<script setup lang="ts">
import type { OtherSitesLevel, Profile, SiteLevel, SiteRule } from '@lock-in/core';
import {
  hasRoomForOwnSite,
  maxOwnSites,
  ownSiteDomain,
  ownSiteRecipe,
  siteFor,
  siteLevels,
  withSiteLevel,
  withSiteRemoved,
} from '@lock-in/core';
import { recipes } from '@lock-in/recipes';
import { computed, ref } from 'vue';
import SegmentedControl from './SegmentedControl.vue';

// A profile's sites, the same list as in the browser extension's settings: each built-in site,
// then any added by address, with its level. The extension does the taming and redirecting, and
// gets every change here through the link. What Tamed hides, and page rules, are set there, and
// so are the parts of a page picked to hide with its element picker, and taming a site all the
// time with no session running. Every other site, at the end, is what happens to the rest, so a
// profile can let through only the sites it allows.
const props = defineProps<{ sites: SiteRule[]; otherSites: OtherSitesLevel }>();
const emit = defineEmits<{ update: [sites: SiteRule[], otherSites: OtherSitesLevel] }>();

const own = computed(() =>
  props.sites.flatMap((site) => {
    const domain = ownSiteDomain(site.recipeId);
    return domain ? [ownSiteRecipe(domain)] : [];
  }),
);
const listed = computed(() => [...recipes, ...own.value]);
const ruleFor = (id: string) => props.sites.find((site) => site.recipeId === id);

// What each level does is spelled out above the list, so nothing depends on hovering.
const levels = siteLevels;
// Every site can be tamed: built-in ones with their recipe, added ones by hiding the parts picked
// on the page in the extension.
const levelOptions = levels.map((level) => ({ id: level.id, label: level.label }));
// Sites nobody listed have no recipe, so Tamed stays in its place, turned off, to keep the
// columns lined up with the sites above.
const otherOptions = levels.map((level) => ({
  id: level.id,
  label: level.label,
  disabled: level.id === 'tamed',
  title: level.id === 'tamed' ? "There's nothing to tame on sites that aren't listed." : undefined,
}));
const othersDetail: Record<OtherSitesLevel, string> = {
  allowed: "Every site that isn't listed is left alone. Restrict them to let through only the sites you allow.",
  redirected:
    "Every site that isn't listed goes to Seiza's page first, with a way through for a few minutes. Addresses on this computer or your network always load.",
  restricted:
    "Only the listed sites that aren't restricted get through. Every other site is shut until the session ends. Addresses on this computer or your network always load.",
};
// A built-in site with no rule follows every other site, so that's the level it shows.
const levelOf = (id: string) => ruleFor(id)?.level ?? props.otherSites;

// The edits are the extension's own (packages/core), on this profile's sites alone.
const edit = (change: (list: Profile[]) => Profile[]) =>
  emit('update', change([{ id: 'this', name: '', sites: props.sites }])[0]!.sites, props.otherSites);
const setOthers = (level: SiteLevel) => {
  if (level !== 'tamed') emit('update', props.sites, level);
};
const setLevel = (recipeId: string, level: SiteLevel) => edit((list) => withSiteLevel(list, 'this', recipeId, level));
const remove = (recipeId: string) => edit((list) => withSiteRemoved(list, 'this', recipeId));

const pageCount = (id: string) => ruleFor(id)?.paths?.length ?? 0;

// Adding a site by its address. One that's built in is already listed.
const draft = ref('');
const found = computed(() => (draft.value.trim() ? siteFor(recipes, draft.value) : undefined));
const problem = computed(() => {
  if (!draft.value.trim()) return '';
  const site = found.value;
  if (!site) return "Use a site's address, like example.com.";
  if (!ownSiteDomain(site.id)) return `${site.name} is built in, so it's listed already.`;
  if (ruleFor(site.id)) return `${site.name} is listed already.`;
  if (!hasRoomForOwnSite({ id: 'this', name: '', sites: props.sites })) {
    return `A profile can have up to ${maxOwnSites} sites of its own.`;
  }
  return '';
});
// A site added while every other site is kept away is one to let through.
function add() {
  const site = found.value;
  if (!site || problem.value) return;
  setLevel(site.id, props.otherSites === 'allowed' ? 'redirected' : 'allowed');
  draft.value = '';
}
</script>

<template>
  <div class="editor">
    <dl class="legend">
      <div v-for="level in levels" :key="level.id">
        <dt class="mono">{{ level.label }}</dt>
        <dd>{{ level.detail }}</dd>
      </div>
    </dl>
    <ul class="sites">
      <li v-for="recipe in listed" :key="recipe.id">
        <span class="name">
          <b>{{ recipe.name }}</b>
          <span v-if="pageCount(recipe.id)" class="pages mono">
            + {{ pageCount(recipe.id) }} page {{ pageCount(recipe.id) === 1 ? 'rule' : 'rules' }}
          </span>
        </span>
        <SegmentedControl
          class="levels"
          :label="`What happens to ${recipe.name}`"
          :options="levelOptions"
          :model-value="levelOf(recipe.id)"
          @update:model-value="setLevel(recipe.id, $event)"
        />
        <button
          v-if="ownSiteDomain(recipe.id)"
          class="key"
          type="button"
          :title="`Remove ${recipe.name}`"
          :aria-label="`Remove ${recipe.name}`"
          @click="remove(recipe.id)"
        >
          ×
        </button>
        <span v-else class="spacer" aria-hidden="true" />
      </li>
      <li class="others">
        <span class="name"><b>Every other site</b></span>
        <SegmentedControl
          class="levels"
          label="What happens to every other site"
          :options="otherOptions"
          :model-value="otherSites"
          @update:model-value="setOthers"
        />
        <span class="spacer" aria-hidden="true" />
        <p class="detail">{{ othersDetail[otherSites] }}</p>
      </li>
    </ul>

    <form class="add" @submit.prevent="add">
      <input
        v-model="draft"
        class="card"
        maxlength="253"
        placeholder="Add a site, like example.com"
        aria-label="Add a site"
      />
      <button class="card mono" type="submit" :disabled="!draft.trim() || !!problem">+ Add</button>
    </form>
    <p v-if="problem" class="problem">{{ problem }}</p>

    <p class="help">
      These apply in the browser, through Seiza's extension, while this profile runs. Added sites start out redirected
      (or allowed, while every other site is kept away), and the browser asks once to let Seiza reach each one, or every
      site to keep away the ones that aren't listed. What Tamed hides, and page rules, are set in the extension's
      settings. To hide part of a page, open the site and choose Hide parts of this page in the extension's popup. Any
      site can also be tamed all the time, with no session running, set there too.
    </p>
  </div>
</template>

<style scoped>
.editor {
  padding: 10px 0 12px;
}

/* What each level does: the name in mono, then a line on it. */
.legend {
  display: grid;
  gap: 6px;
  margin: 0 0 8px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: rgb(11 15 30 / 0.5);
}

.legend div {
  display: grid;
  grid-template-columns: 84px 1fr;
  gap: 10px;
  align-items: baseline;
}

.legend dt {
  color: var(--dim);
  font-size: 9.5px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.legend dd {
  margin: 0;
  color: var(--faint);
  font-size: 11.5px;
  line-height: 1.45;
}

ul {
  margin: 0;
  padding: 0;
  list-style: none;
}

/* Each site: its name (and × for an added one) over a full-width switch, with room to breathe. */
.sites li {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 8px 10px;
  padding: 12px 0 14px;
  border-bottom: 1px solid var(--line);
  font-size: 13px;
}

.sites li:last-child {
  border-bottom: 0;
}

/* Every other site closes the list, under a dashed line, with what it does spelled out. */
.sites li:nth-last-child(2) {
  border-bottom-style: dashed;
}

.others .detail {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--faint);
  font-size: 11.5px;
  line-height: 1.45;
}

.name {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
}

.name b {
  overflow: hidden;
  font-weight: 500;
  text-overflow: ellipsis;
}

.pages {
  flex: none;
  font-size: 9.5px;
}

.levels {
  grid-column: 1 / -1;
}

.levels :deep(button) {
  padding: 8px 4px;
}

.key,
.spacer {
  grid-row: 1;
  grid-column: 2;
}

.key {
  padding: 2px 8px;
}

.add {
  display: flex;
  gap: 8px;
  margin-top: 6px;
}

input {
  flex: 1;
  min-width: 0;
  padding: 7px 9px;
  border: 0;
  outline: 0;
  background: none;
  color: var(--starlight);
  font: 12px var(--mono);
}

input:focus {
  --t: var(--tick-bright);
}

.add button {
  padding: 6px 10px;
  background: none;
  color: var(--dim);
  transition: color 300ms;
}

.add button:not(:disabled):hover {
  color: var(--starlight);
}

.add button:disabled {
  cursor: default;
  opacity: 0.5;
}

.problem {
  margin: 6px 0 0;
  color: var(--glint);
  font-size: 11.5px;
}

.help {
  margin: 10px 0 0;
  color: var(--faint);
  font-size: 11.5px;
}
</style>
