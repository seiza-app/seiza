<!-- The logo, as on the website: "seiza" in light lowercase with the gold star for the dot of the i,
     and its reading, せいざ, underneath. It takes its size from the font size it's given. -->
<template>
  <span class="wordmark" role="img" aria-label="Seiza">
    <span class="word" aria-hidden="true"
      >se<span class="i"
        >ı<svg viewBox="28 28 72 72">
          <path d="M64 30 Q67.5 60.5 98 64 Q67.5 67.5 64 98 Q60.5 67.5 30 64 Q60.5 60.5 64 30 Z" /></svg></span
      >za</span
    >
    <span class="kana" lang="ja" aria-hidden="true">せいざ</span>
  </span>
</template>

<style scoped>
.wordmark {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  gap: 0.14em;
  color: var(--starlight);
  text-transform: none;
}

.word {
  font-family:
    system-ui,
    -apple-system,
    sans-serif;
  font-weight: 300;
  line-height: 1;
  letter-spacing: 0.04em;
}

.i {
  position: relative;
}

svg {
  position: absolute;
  left: 50%;
  top: 0.02em;
  width: 0.34em;
  height: 0.34em;
  fill: var(--glow);
  filter: drop-shadow(0 0 4px rgb(245 200 107 / 0.6));
  transform: translateX(-50%);
}

/* Never so small that the kana blur together. */
.kana {
  font:
    max(8.5px, 0.31em) / 1 'Hiragino Sans',
    'Yu Gothic',
    'Noto Sans JP',
    sans-serif;
  letter-spacing: 0.5em;
  margin-right: -0.5em;
  color: var(--dim);
}
</style>
