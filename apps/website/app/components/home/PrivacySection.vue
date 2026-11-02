<template>
  <section id="privacy" data-name="Privacy">
    <p class="kicker mono reveal"><b>05</b> · Private by design</p>
    <h2 class="reveal" style="--d: 80ms">Nothing leaves your computer</h2>
    <p class="lede reveal" style="--d: 160ms">
      Seiza collects nothing. It has no accounts, no analytics, no ads, and no servers of its own.
    </p>
    <ul class="promises">
      <li class="hud glass reveal" style="--d: 220ms">
        <strong>Kept on your device</strong>
        <p>
          Your profiles, sessions, history and sky stay in your browser and the app's data folder, and on your own sync
          server, encrypted, if you turn sync on.
        </p>
      </li>
      <li class="hud glass reveal" style="--d: 280ms">
        <strong>Linked locally</strong>
        <p>The extension and the app talk over your browser's Native Messaging, which never leaves your computer.</p>
      </li>
      <li class="hud glass reveal" style="--d: 340ms">
        <strong>One update check a day</strong>
        <p>
          The app asks GitHub once a day if there's a new version, which you can turn off. The extension makes no
          requests. The source code is public, so you can check any of it.
        </p>
      </li>
    </ul>
    <SyncNote class="reveal" style="--d: 400ms" />
    <p class="reveal" style="--d: 460ms">
      <NuxtLink class="textlink" to="/privacy">Read the privacy policy ›</NuxtLink>
    </p>
  </section>
</template>
