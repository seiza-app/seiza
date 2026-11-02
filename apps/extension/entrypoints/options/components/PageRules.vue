<script lang="ts" setup>
import {
  findRule,
  maxPathLength,
  maxPathRules,
  normalizePath,
  otherSitesLevel,
  ownSiteDomain,
  type Profile,
  type SiteRecipe,
} from '@lock-in/core';
import { computed, ref } from 'vue';
import LevelPicker from '@/components/LevelPicker.vue';

const props = defineProps<{ profile: Profile; recipe: SiteRecipe }>();

const siteLevel = computed(() => findRule(props.profile, props.recipe.id)?.level ?? otherSitesLevel(props.profile));
const rules = computed(() => findRule(props.profile, props.recipe.id)?.paths ?? []);
// A new page goes the other way from its site: redirected inside an open site, allowed inside a closed one.
const siteClosed = computed(() => siteLevel.value === 'redirected' || siteLevel.value === 'restricted');
const newLevel = computed(() => (siteClosed.value ? 'allowed' : 'redirected'));

const has = (path: string) => rules.value.some((rule) => rule.path.toLowerCase() === path.toLowerCase());
// Suggested pages, e.g. "Shorts", only make sense while the rest of the site is open.
const suggestions = computed(() =>
  siteClosed.value ? [] : (props.recipe.pages ?? []).filter((page) => !page.paths.every(has)),
);
const full = computed(() => rules.value.length >= maxPathRules);

const adding = ref(false);
const draft = ref('');
const invalid = computed(() => draft.value.trim() !== '' && !normalizePath(draft.value));
const input = ref<HTMLInputElement>();

function startAdding() {
  adding.value = true;
  requestAnimationFrame(() => input.value?.focus());
}

async function add() {
  if (!draft.value.trim() || invalid.value) return;
  await setPageLevel(props.profile.id, props.recipe.id, [draft.value], newLevel.value);
  draft.value = '';
  adding.value = false;
}
</script>

<template>
  <div class="pages">
    <ul v-if="rules.length" class="list rules" :aria-label="`${recipe.name} pages`">
      <li v-for="rule in rules" :key="rule.path">
        <button
          type="button"
          class="text-button remove"
          :aria-label="`Remove ${rule.path}`"
          title="Remove"
          @click="removePageRule(profile.id, recipe.id, rule.path)"
        >
          ×
        </button>
        <code class="path" :title="rule.path">{{ rule.path === '/' ? '/ (home page)' : rule.path }}</code>
        <LevelPicker
          :site-name="`${recipe.name} ${rule.path}`"
          :can-tame="recipe.toggles.length > 0 || !!ownSiteDomain(recipe.id)"
          :model-value="rule.level"
          @update:model-value="setPageLevel(profile.id, recipe.id, [rule.path], $event)"
        />
      </li>
    </ul>

    <form v-if="adding" class="add" @submit.prevent="add">
      <input
        ref="input"
        v-model="draft"
        :maxlength="maxPathLength"
        :aria-label="`Page on ${recipe.name}`"
        :aria-invalid="invalid"
        placeholder="/path, e.g. /r/all"
        @keydown.esc="adding = false"
      />
      <button type="submit" :disabled="!draft.trim() || invalid">Add</button>
      <button type="button" class="text-button" @click="adding = false">Cancel</button>
      <p v-if="invalid" class="error">Use a path like /r/all, without ? or #.</p>
    </form>

    <div v-else-if="!full" class="actions">
      <button
        v-for="page in suggestions"
        :key="page.label"
        type="button"
        class="text-button suggestion"
        :title="page.paths.join(', ')"
        @click="setPageLevel(profile.id, recipe.id, page.paths, 'redirected')"
      >
        + Redirect {{ page.label }}
      </button>
      <button type="button" class="text-button" @click="startAdding">+ Page rule</button>
    </div>
  </div>
</template>

<style scoped>
.pages {
  margin-top: 6px;
}

.rules li {
  display: grid;
  grid-template-columns: auto 1fr minmax(0, 320px);
  align-items: center;
  gap: 4px;
}

.path {
  overflow: hidden;
  color: var(--starlight);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.remove {
  margin-left: -8px;
  font-size: 16px;
  line-height: 1;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
  margin-left: -8px;
}

.suggestion {
  color: var(--glow);
}

.error {
  flex-basis: 100%;
  margin: 0;
}

@media (max-width: 480px) {
  .rules li {
    grid-template-columns: auto 1fr;
  }

  .rules li :deep(.levels) {
    grid-column: 1 / -1;
    grid-row: 2;
  }
}
</style>
