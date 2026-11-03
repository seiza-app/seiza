<script setup lang="ts">
// Also in the extension (apps/extension/components/EndEarly.vue); keep the two copies working the
// same. This one is styled for the desktop app's HUD frames.
import { unlockText, type Strictness } from '@lock-in/core';
import { computed, onMounted, onUnmounted, ref } from 'vue';

const props = defineProps<{
  lock: Strictness;
  /** Emergency passes left, or undefined where passes can't be used (the extension). */
  passesLeft?: number;
}>();
const emit = defineEmits<{ end: []; pass: []; cancel: [] }>();

// Gentle: a short pause before ending works.
const wait = ref(props.lock === 'gentle' ? 10 : 0);
const tick = setInterval(() => wait.value > 0 && wait.value--, 1000);
onUnmounted(() => clearInterval(tick));

// Medium: type out random text. Pasting and dropping text in are turned off.
const target = unlockText();
const typed = ref('');
const typedRight = computed(() => typed.value.trim().replace(/\s+/g, ' ') === target);
// Ready to type into, as the End early button that opened this is gone.
const typing = ref<HTMLInputElement>();
onMounted(() => typing.value?.focus());
</script>

<template>
  <div class="end-early">
    <template v-if="lock === 'gentle'">
      <p>Take a breath. Still want to stop?</p>
      <button class="confirm" type="button" :disabled="wait > 0" @click="emit('end')">
        {{ wait > 0 ? `End session (${wait})` : 'End session' }}
      </button>
    </template>

    <template v-else-if="lock === 'medium'">
      <p>To end early, type this out:</p>
      <p class="target" aria-hidden="true">{{ target }}</p>
      <input
        ref="typing"
        v-model="typed"
        :aria-label="`Type: ${target}`"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        @paste.prevent
        @drop.prevent
      />
      <button class="confirm" type="button" :disabled="!typedRight" @click="emit('end')">End session</button>
    </template>

    <template v-else>
      <p>This session is in Hard mode, so it can't end until the timer is done.</p>
      <p v-if="passesLeft === undefined" class="help">An emergency pass in the Seiza app can end it.</p>
      <template v-else-if="passesLeft > 0">
        <button class="confirm" type="button" @click="emit('pass')">Use an emergency pass</button>
        <p class="help">{{ passesLeft }} left this week. Each one comes back 7 days after it's used.</p>
      </template>
      <p v-else class="help">No emergency passes left this week.</p>
    </template>

    <button class="keep" type="button" @click="emit('cancel')">Keep going</button>
  </div>
</template>

<style scoped>
.end-early {
  display: grid;
  gap: 8px;
  text-align: center;
}

p {
  margin: 0;
  font-size: 12.5px;
}

.help {
  color: var(--faint);
  font-size: 11.5px;
}

.target {
  padding: 8px;
  border: 1px dashed var(--line);
  border-radius: 3px;
  font: 14px var(--mono);
  letter-spacing: 0.05em;
  user-select: none;
}

input {
  padding: 8px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: rgb(8 11 22 / 0.5);
  color: var(--starlight);
  font: 14px var(--mono);
  text-align: center;
  caret-color: #9fe3c9;
}

input:focus {
  border-color: var(--tick);
  outline: none;
}

button {
  padding: 8px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: none;
  font: 10px/1.5 var(--mono);
  letter-spacing: 0.08em;
  text-transform: uppercase;
  transition:
    color 300ms,
    border-color 300ms;
}

.confirm {
  color: var(--dim);
}

.confirm:hover:not(:disabled) {
  border-color: rgb(220 228 255 / 0.3);
  color: var(--starlight);
}

.confirm:disabled {
  opacity: 0.5;
  cursor: default;
}

/* Keeping going is the easy choice. */
.keep {
  border-color: color-mix(in srgb, var(--glow) 50%, transparent);
  color: var(--glow);
  text-shadow: 0 0 8px color-mix(in srgb, var(--glow) 50%, transparent);
}

.keep:hover {
  border-color: var(--glow);
}
</style>
