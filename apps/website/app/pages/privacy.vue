<script setup lang="ts">
// The privacy policy, as in PRIVACY.md at the root of the repository. Change both together.
definePageMeta({ lockOnArrival: true });

usePageSeo({
  title: 'Privacy · Seiza',
  description:
    "Seiza collects nothing. It has no accounts, no analytics, no ads, and no servers of its own. If you turn on sync, your data goes to a server you choose, encrypted so it can't read it.",
});
</script>

<template>
  <div class="site-page reading">
    <section id="top" class="page-head" data-name="Top">
      <p class="kicker mono reveal"><b>✦</b> · Privacy</p>
      <h1 class="reveal" style="--d: 80ms">Seiza collects nothing</h1>
      <p class="lede reveal" style="--d: 160ms">
        It has no accounts, no analytics, no ads, and no servers of its own. Nothing you do in it is sent to us or
        anyone else. If you turn on sync, your data goes to a sync server you choose, encrypted so that server can't
        read it. This covers the Seiza desktop app and the Seiza browser extension.
      </p>
    </section>

    <section id="stays" data-name="Stays">
      <p class="kicker mono reveal"><b>01</b> · What stays on your computer</p>
      <h2 class="reveal" style="--d: 80ms">Everything you do in it</h2>
      <ul class="plain">
        <li class="reveal" style="--d: 120ms">
          <strong>Your profiles, sessions and settings:</strong> the sites and apps each profile tames, redirects or
          restricts, the parts of pages you've hidden, and your other settings.
        </li>
        <li class="reveal" style="--d: 160ms">
          <strong>Your history:</strong> when your sessions ran, and each visit to a redirected site: which site, when,
          how many times today, and the reason you gave, if you gave one. This is what the night sky and the weekly
          report are drawn from.
        </li>
        <li class="reveal" style="--d: 200ms"><strong>Your satellites and your own quotes.</strong></li>
      </ul>
      <p class="aside reveal" style="--d: 240ms">
        The extension keeps its copy in your browser's extension storage. The app keeps its copy in its data folder on
        your computer, along with sync's key and this computer's sign-in for the sync server if you use sync.
        Uninstalling either removes its copy. A backup from the app's Settings is a file you save wherever you choose.
      </p>
    </section>

    <section id="shared" data-name="Shared">
      <p class="kicker mono reveal"><b>02</b> · What the extension and the app share</p>
      <h2 class="reveal" style="--d: 80ms">Only with each other</h2>
      <p class="lede reveal" style="--d: 160ms">
        When both are installed, the extension and the app talk to each other over your browser's Native Messaging,
        which never leaves your computer. They share your profiles, the current session, site settings and visits to
        redirected sites, so each one shows the same thing.
      </p>
    </section>

    <section id="extension" data-name="Extension">
      <p class="kicker mono reveal"><b>03</b> · What the extension reads</p>
      <h2 class="reveal" style="--d: 80ms">Only what it hides</h2>
      <p class="lede reveal" style="--d: 160ms">
        On sites it tames, the extension adds a stylesheet that hides parts of the page, and pauses any video it hides.
        When you use "Hide parts of this page", it notes which part you clicked so it can hide it again later. It
        doesn't read or keep what you type or see on any page, and it doesn't keep your browsing history: the popup
        checks the address of the tab you open it on, so it can show that site's settings.
      </p>
    </section>

    <section id="app" data-name="App">
      <p class="kicker mono reveal"><b>04</b> · What the app reads</p>
      <h2 class="reveal" style="--d: 80ms">Just enough to run sessions</h2>
      <ul class="plain">
        <li class="reveal" style="--d: 120ms">
          <strong>How long since your last keyboard or mouse input,</strong> only if you turn on ending a session when
          you're away, so one left running ends by itself.
        </li>
        <li class="reveal" style="--d: 160ms">
          <strong>Which programs are running,</strong> only during a session, to close or hide the apps your profile
          lists and to notice a browser running without the extension.
        </li>
        <li class="reveal" style="--d: 200ms">
          <strong>The first day of the week set on your computer,</strong> for the weekly report.
        </li>
      </ul>
      <p class="aside reveal" style="--d: 240ms">
        On macOS, if you turn on blocking sites without the extension, the app lists your session's restricted sites in
        your computer's hosts file while the session runs, and takes them out after.
      </p>
    </section>

    <section id="network" data-name="Network">
      <p class="kicker mono reveal"><b>05</b> · Network</p>
      <h2 class="reveal" style="--d: 80ms">An update check, and sync if you want it</h2>
      <p class="lede reveal" style="--d: 160ms">
        The extension makes no network requests. The app checks once a day for a new version, which you can turn off in
        Settings › Updates. The only other thing it sends is sync, which is off until you set it up in Settings › Sync.
      </p>
      <SyncNote class="reveal" style="--d: 220ms" />
      <ul class="plain">
        <li class="reveal" style="--d: 260ms">
          <strong>What sync sends:</strong> your finished sessions and their goals, the session running now (with its
          goal and its profile's rules, so it runs on your other computers too), your visits to redirected sites, your
          constellation names, your profiles and this computer's name. It keeps one connection open to the server, so
          changes arrive within seconds. Before a session ends because you're away, the app asks your other computers
          how long it's been since their keyboard or mouse was used; the server passes the question and the answers on
          without keeping them.
        </li>
        <li class="reveal" style="--d: 300ms">
          <strong>What the server sees:</strong> all of it is encrypted on your computer with a key the server never
          gets, so whoever runs the server can't read it. They can see which computers are in your group, when each last
          synced or was connected, how much it stores, and the IP addresses it syncs from.
        </li>
        <li class="reveal" style="--d: 340ms">
          <strong>Leave sync</strong> in Settings › Sync stops this computer syncing and keeps everything on it, and the
          server deletes your group once its last computer leaves.
        </li>
      </ul>
      <p class="aside reveal" style="--d: 380ms">
        Your settings stay on each computer, and sync's key and sign-in are left out of backups. The quotes are bundled
        with the app. Links you add yourself, like the support link or your own links on the redirect page, open in your
        browser like any other link.
      </p>
    </section>

    <section id="changes" data-name="Changes">
      <p class="kicker mono reveal"><b>06</b> · Changes and questions</p>
      <h2 class="reveal" style="--d: 80ms">This page changes first</h2>
      <p class="lede reveal" style="--d: 160ms">
        If this ever changes, this page changes first, with the date below. The source code is public at
        <a href="https://github.com/seiza-app/seiza">github.com/seiza-app/seiza</a>, so you can check any of it.
        Questions go to the repository's <a href="https://github.com/seiza-app/seiza/issues">issues</a>.
      </p>
      <p class="aside mono reveal" style="--d: 220ms">Last updated: 2 November 2026</p>
      <SiteFooter class="reveal" style="--d: 280ms" />
    </section>
  </div>
</template>
