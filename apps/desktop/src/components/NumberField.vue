<script lang="ts" setup>
// A stepper, like an instrument's: ‹ and › step the value, and it can be typed in too.
const props = defineProps<{ label: string; min: number; max: number; unit: string; step?: number }>();
const value = defineModel<number>({ required: true });

const within = (next: number) => Math.min(props.max, Math.max(props.min, next));

// Commits on change rather than every keystroke, and snaps the field back into range. Cleared,
// it goes back to the value it had.
function commit(event: Event) {
  const input = event.target as HTMLInputElement;
  const parsed = input.value.trim() === '' ? NaN : Math.round(Number(input.value));
  const next = Number.isFinite(parsed) ? within(parsed) : value.value;
  input.value = String(next);
  value.value = next;
}

/** Steps up or down to the next multiple of the step, e.g. 47 goes to 50 or 45 in steps of 5. */
function stepBy(direction: 1 | -1) {
  const step = props.step ?? 1;
  const next =
    direction > 0 ? Math.floor(value.value / step) * step + step : Math.ceil(value.value / step) * step - step;
  value.value = within(next);
}
</script>

<template>
  <span class="card stepper">
    <button type="button" :aria-label="`${label}, less`" :disabled="value <= min" @click="stepBy(-1)">‹</button>
    <input type="number" :aria-label="label" :min="min" :max="max" :value="value" @change="commit" />
    <span class="unit mono">{{ unit }}</span>
    <button type="button" :aria-label="`${label}, more`" :disabled="value >= max" @click="stepBy(1)">›</button>
  </span>
</template>

<style scoped>
.stepper {
  display: inline-flex;
  align-items: center;
  flex: none;
}

button {
  width: 24px;
  height: 24px;
  padding: 0;
  border-radius: 0;
  background: none;
  color: var(--dim);
  font: 12px var(--mono);
  transition: color 300ms;
}

button:hover:not(:disabled) {
  color: var(--starlight);
}

button:disabled {
  opacity: 0.3;
  cursor: default;
}

input {
  width: 3.2em;
  padding: 0;
  border: 0;
  outline: 0;
  background: none;
  color: var(--starlight);
  font: 500 12px var(--mono);
  text-align: center;
  appearance: textfield;
  -moz-appearance: textfield;
}

input::-webkit-outer-spin-button,
input::-webkit-inner-spin-button {
  margin: 0;
  -webkit-appearance: none;
}

.stepper:focus-within {
  --t: var(--tick-bright);
}

.unit {
  padding-right: 4px;
  white-space: nowrap;
}
</style>
