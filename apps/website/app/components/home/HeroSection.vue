<script setup lang="ts">
const { locked, toggle } = useLock();
</script>

<template>
  <section id="top" class="hero" data-name="Home">
    <div class="noise" aria-hidden="true">
      <div class="n" style="--x: 7%; --y: 22%; --i: 0; --r: -14deg">
        <div class="jit reel" style="--j: 0.7s"></div>
      </div>
      <div class="n" style="--x: 15%; --y: 7%; --i: 1"><div class="jit badge" style="--j: 0.5s">99+</div></div>
      <div class="n m-low" style="--x: 14.5%; --y: 52%; --i: 2">
        <div class="jit toast" style="--j: 1.1s">
          <i></i>
          <span><b>everyone_now</b> mentioned you in a thread</span>
        </div>
      </div>
      <div class="n" style="--x: 37%; --y: 91%; --i: 3">
        <div class="jit pill" style="--j: 0.8s">Trending now</div>
      </div>
      <div class="n wide-only" style="--x: 17%; --y: 85%; --i: 4">
        <div class="jit thumb" style="--j: 1.3s">
          <div></div>
          <p>Up next · Autoplay in 3</p>
        </div>
      </div>
      <div class="n" style="--x: 92%; --y: 21%; --i: 5; --r: 12deg">
        <div class="jit reel b" style="--j: 0.6s"></div>
      </div>
      <div class="n wide-only" style="--x: 80%; --y: 8%; --i: 6">
        <div class="jit pill cyan" style="--j: 0.9s">Suggested for you</div>
      </div>
      <div class="n m-hide" style="--x: 88%; --y: 52%; --i: 7">
        <div class="jit post" style="--j: 1s">
          <header>daily.drama</header>
          <p>Everyone is talking about this. Are you?</p>
          <footer>♥ 12K · Reposted</footer>
        </div>
      </div>
      <div class="n m-badge" style="--x: 82%; --y: 36%; --i: 8">
        <div class="jit badge" style="--j: 0.45s">12</div>
      </div>
      <div class="n" style="--x: 64%; --y: 93%; --i: 9">
        <div class="jit pill live" style="--j: 0.7s">● LIVE · New messages (7)</div>
      </div>
      <div class="n wide-only" style="--x: 53%; --y: 84%; --i: 10">
        <div class="jit badge" style="--j: 0.55s">3</div>
      </div>
      <div class="n wide-only" style="--x: 83%; --y: 85%; --i: 11">
        <div class="jit thumb" style="--j: 1.2s">
          <div></div>
          <p>Because you watched…</p>
        </div>
      </div>
    </div>

    <p class="kicker mono reveal">A free website and app blocker for Mac and Linux</p>
    <h1 class="headline reveal">
      Social media is
      <span class="glare"
        ><span class="hazy">light pollution.</span><span class="clear" aria-hidden="true">light pollution.</span></span
      >
    </h1>
    <p class="tagline reveal" style="--d: 140ms">
      When you lock in, the noise fades and you can finally see the stars.
    </p>
    <div class="reveal" style="--d: 280ms">
      <button class="switch" role="switch" :aria-checked="locked" @click="toggle">
        <span class="track" aria-hidden="true"></span>
        <span>
          <strong>Lock in</strong>
        </span>
      </button>
    </div>
  </section>
</template>
