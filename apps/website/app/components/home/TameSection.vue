<script setup lang="ts">
// The four levels on a made-up video site. The redirect page's wait counts down while it shows,
// as it does for real.
type Level = 'allowed' | 'tamed' | 'redirected' | 'restricted';

const LEVELS: Record<Level, { name: string; url: string; says: string }> = {
  allowed: { name: 'Allowed', url: 'videos.example/watch', says: 'nothing changes.' },
  tamed: {
    name: 'Tamed',
    url: 'videos.example/watch',
    says: 'the site still works, but the distracting parts are hidden: feeds, Shorts, trending, suggested and promoted posts.',
  },
  redirected: {
    name: 'Redirected',
    url: 'Seiza · videos.example',
    says: 'visits go to a calm Seiza page first. Focus for five minutes, read your if-then plan, or open one of your own links.',
  },
  restricted: {
    name: 'Restricted',
    url: 'Seiza · videos.example',
    says: 'blocked, with no way through until the session ends.',
  },
};

const WAIT = 300;
const level = ref<Level>('allowed');
const left = ref(WAIT);
const clock = computed(() => `${Math.floor(left.value / 60)}:${String(left.value % 60).padStart(2, '0')}`);
let countdown: ReturnType<typeof setInterval> | undefined;

function setLevel(next: Level) {
  level.value = next;
  clearInterval(countdown);
  left.value = WAIT;
  if (next === 'redirected') {
    countdown = setInterval(() => (left.value = Math.max(0, left.value - 1)), 1000);
  }
}

onBeforeUnmount(() => clearInterval(countdown));
</script>

<template>
  <section id="tame" data-name="Tame">
    <p class="kicker mono reveal"><b>01</b> · The extension</p>
    <h2 class="reveal" style="--d: 80ms">Tame, don't just block.</h2>
    <p class="lede reveal" style="--d: 160ms">
      Most website blockers treat a site as all or nothing. Seiza gives every site one of four levels, chosen per
      profile. Try them on this made-up video site.
    </p>
    <div class="demo hud glass reveal" style="--d: 220ms" :data-level="level">
      <div class="levels" role="group" aria-label="Level for videos.example">
        <button
          v-for="(info, key) in LEVELS"
          :key="key"
          type="button"
          :aria-pressed="level === key"
          @click="setLevel(key)"
        >
          {{ info.name }}
        </button>
      </div>
      <p class="level-says" aria-live="polite">
        <b>{{ LEVELS[level].name }}:</b> {{ LEVELS[level].says }}
      </p>

      <div class="browser">
        <div class="chrome" aria-hidden="true">
          <i></i><i></i><i></i><span class="url">{{ LEVELS[level].url }}</span>
        </div>
        <div class="viewport">
          <div class="site" role="group" aria-label="A made-up video site">
            <div class="main">
              <div class="player"></div>
              <h3>Building a bookshelf by hand, start to finish</h3>
              <p class="by">Slow Workshop · 41 min</p>
              <div class="fold">
                <div>
                  <div class="label"><span class="live">NEW</span> Shorts <small>Swipe for more</small></div>
                  <div class="shorts-row">
                    <i style="--a: #ff3d7f; --b: #ff9a52"></i><i style="--a: #3fd8ff; --b: #d46bff"></i
                    ><i style="--a: #c4ff4a; --b: #3fd8ff"></i><i style="--a: #ff9a52; --b: #ff4a3d"></i
                    ><i style="--a: #2d6bff; --b: #3fd8ff"></i><i style="--a: #ff3d7f; --b: #d46bff"></i>
                  </div>
                </div>
              </div>
              <div class="comments">
                <p>The joinery at 14:20 is beautiful. Trying this on the weekend.</p>
                <p>Which wood would you use for something lighter?</p>
              </div>
            </div>
            <div class="recs fold">
              <div>
                <div class="label">Up next <small>Autoplay on</small></div>
                <p>
                  <i style="--a: #ff9a52; --b: #ff3d7f"></i
                  ><span>You won't believe this ending<small>2.1M views</small></span>
                </p>
                <p>
                  <i style="--a: #3fd8ff; --b: #2d6bff"></i><span>Top 50 moments, ranked<small>Trending</small></span>
                </p>
                <p>
                  <i style="--a: #d46bff; --b: #ff3d7f"></i
                  ><span>Reacting to my old videos<small>Live now</small></span>
                </p>
                <p>
                  <i style="--a: #c4ff4a; --b: #2fb37a"></i
                  ><span>Watch this before it's gone<small>Suggested for you</small></span>
                </p>
                <p>
                  <i style="--a: #ff4a3d; --b: #ff9a52"></i
                  ><span>Everyone's talking about it<small>Recommended</small></span>
                </p>
              </div>
            </div>
          </div>

          <div class="page wait" aria-hidden="true">
            <div class="brand"><span>✦</span> Seiza</div>
            <h4>Focus for 5 minutes first</h4>
            <p class="lead">If you still want videos.example after that, go right ahead.</p>
            <p class="clock">{{ clock }}</p>
            <figure>
              <figcaption class="mono">Your plan</figcaption>
              <p>When I want to scroll, I'll write one paragraph first.</p>
            </figure>
            <div class="links">
              <span>My draft ↗<small>docs.example.com</small></span>
              <span>Reading list ↗<small>example.com</small></span>
            </div>
            <div class="do">
              <span class="yes">Not now</span>
              <span class="go" :style="{ '--done': String(1 - left / WAIT) }"><i></i>Go to videos.example</span>
            </div>
            <p class="hint">Going on gives you 5 minutes on videos.example.</p>
          </div>

          <div class="page off" aria-hidden="true">
            <div class="brand"><span>✦</span> Seiza</div>
            <h4>videos.example is off for now</h4>
            <p>It's restricted during Deep Work and will be back when the session ends.</p>
            <div class="do"><span class="yes">Back to focus</span></div>
          </div>
        </div>
      </div>
    </div>

    <div class="rules">
      <div class="hud glass reveal" style="--d: 0ms">
        <span class="mono">Page rules</span>
        <h3>Part of a site, its own level</h3>
        <p>
          Redirect YouTube Shorts while the rest of YouTube is tamed, or allow reddit.com/r/writing while the rest of
          Reddit is redirected.
        </p>
      </div>
      <div class="hud glass reveal" style="--d: 80ms">
        <span class="mono">Recipes</span>
        <h3>Eleven sites, ready to tame</h3>
        <p>
          Reddit, YouTube, X, Instagram, TikTok, Facebook, LinkedIn, Pinterest, Twitch, Threads and Bluesky. Add any
          other site by its address.
        </p>
      </div>
      <div class="hud glass reveal" style="--d: 160ms">
        <span class="mono">Hide parts of this page</span>
        <h3>Click away the rest</h3>
        <p>
          Anything else you'd rather not see, you can click away, and keep a site tamed all the time, outside sessions
          too.
        </p>
      </div>
    </div>
  </section>
</template>
