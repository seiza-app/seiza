<script lang="ts" setup>
import { siteLevels, type SiteLevel } from '@lock-in/core';
import { computed } from 'vue';

const props = defineProps<{
  /** The site's name, for screen readers. */
  siteName: string;
  /** Tamed does nothing for sites whose recipe has no toggles yet. */
  canTame: boolean;
  /** Why Tamed can't be picked, in place of the usual line. */
  tameDetail?: string;
}>();
const level = defineModel<SiteLevel>({ required: true });

const levels = computed(() =>
  siteLevels.map((level) =>
    level.id === 'tamed' && !props.canTame
      ? { ...level, detail: props.tameDetail ?? `Nothing to hide on ${props.siteName} yet.` }
      : level,
  ),
);
const index = computed(() => levels.value.findIndex((l) => l.id === level.value));
</script>

<template>
  <div class="levels" role="radiogroup" :aria-label="`${siteName} level`">
    <span class="indicator" aria-hidden="true" :style="{ transform: `translateX(${index * 100}%)` }" />
    <button
      v-for="option in levels"
      :key="option.id"
      type="button"
      role="radio"
      :aria-checked="level === option.id"
      :class="option.id"
      :title="option.detail"
      :disabled="option.id === 'tamed' && !canTame && level !== 'tamed'"
      @click="level = option.id"
    >
      {{ option.label }}
    </button>
  </div>
</template>

<style scoped>
/* A segmented switch like the app's: mono choices in a hairline frame, the picked one lit. */
.levels {
  position: relative;
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  padding: 2px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: rgb(8 11 22 / 0.55);
}

/* Slides under the chosen level. */
.indicator {
  position: absolute;
  top: 2px;
  bottom: 2px;
  left: 2px;
  width: calc((100% - 4px) / 4);
  border-radius: 2px;
  background: rgb(220 228 255 / 0.08);
  box-shadow: inset 0 -1px 0 #cfe0ff;
  transition: transform 450ms var(--spring);
}

button {
  position: relative;
  padding: 7px 2px;
  background: none;
  color: var(--faint);
  font: 9.5px var(--mono);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  transition: color 300ms;
}

button[aria-checked='true'] {
  color: var(--starlight);
}

button.restricted[aria-checked='true'] {
  color: var(--glow);
}

button:not(:disabled):hover {
  color: var(--dim);
}

button[aria-checked='true']:hover {
  color: var(--starlight);
}

button:disabled {
  cursor: default;
  opacity: 0.35;
}

button:focus-visible {
  outline-offset: -2px;
}

@media (prefers-reduced-motion: reduce) {
  .indicator {
    transition: none;
  }
}
</style>
