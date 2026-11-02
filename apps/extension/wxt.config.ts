import { recipes } from '@lock-in/recipes';
import { defineConfig } from 'wxt';

const recipeSites = recipes.flatMap((recipe) => recipe.matches);

// `pnpm zip` builds for the Chrome Web Store, which won't take a manifest with a key (below).
// It builds into its own folder, so an unpacked copy loaded from .output/chrome-mv3 keeps its id.
const forStore = process.env.LOCK_IN_STORE === '1';

const chromeKey =
  'MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAydKVix/EgyWAVc8qyi7dOkxVHpDYSl71NX619hw7CmRbRiXMBUDTq2Q/8ugOXqsIXTjTU0AjJH2bOxndoIj8Bh/DcCn4b967hsooiSwibTrAUKpad3ql/QZiZmR/Dzn4VVkNKhQaNTVrDJdlX5kdiJnE2y9FrTRDsYM0se/Xpy97YjagJClVVJzUfFJKTBI5SIuu9bBf3wm/23Zul6vpq0GIV2tAUZX09h5Vts3Gjtx6TjhpAUNQwcGkzGyjJ0UXfG8gXuTB2DcVjNHgd9OZXi4yf9PzZcp2b/pKueth59l6+Iv0KR6Rb8YUkcep4NleSaKv6TD+EtdLZlfV3eJ9qwIDAQAB';

// See https://wxt.dev/api/reference/wxt/interfaces/inlineconfig.html
export default defineConfig({
  modules: ['@wxt-dev/module-vue'],
  ...(forStore ? { outDirTemplate: '{{browser}}-mv{{manifestVersion}}-store' } : {}),
  // Mozilla reviews the code it's given against the sources it was built from, so the Firefox
  // zip comes with the parts of the repo the extension is built from (SOURCES.md says how).
  zip: {
    name: 'seiza',
    sourcesRoot: '../..',
    includeSources: [
      'apps/extension/**',
      'packages/**',
      'package.json',
      'pnpm-lock.yaml',
      'pnpm-workspace.yaml',
      'LICENSE',
      'SOURCES.md',
    ],
    excludeSources: [
      'apps/extension/e2e/**',
      'apps/extension/store/**',
      'apps/extension/.output/**',
      '**/node_modules/**',
    ],
  },
  manifest: ({ browser, manifestVersion }) => ({
    name: 'Seiza',
    description: 'Tame, block, or redirect distracting sites during focus sessions.',
    homepage_url: 'https://github.com/seiza-app/seiza',
    // "WithHostAccess" limits redirects to the recipe sites below, so the browser doesn't
    // warn that Seiza can block content on every page.
    // nativeMessaging links the extension to the desktop app, when it's installed.
    // scripting runs the content script on sites people add themselves (utils/ownSites.ts).
    permissions: [
      'storage',
      'activeTab',
      'alarms',
      'declarativeNetRequestWithHostAccess',
      'nativeMessaging',
      'scripting',
    ],
    host_permissions: recipeSites,
    // Sites people add themselves are asked for one at a time, as they're added, and every site
    // only once a profile keeps away the sites it doesn't list ("Every other site"). Manifest V2
    // (Firefox) lists them with the other optional permissions.
    ...(manifestVersion === 2 ? { optional_permissions: ['*://*/*'] } : { optional_host_permissions: ['*://*/*'] }),
    // Redirected visits land on the redirect page. Chrome only allows that if the page the
    // visit came *from* may load it, and a link to Reddit can be on any site, so every site
    // may. use_dynamic_url stops websites from loading it directly to detect Seiza.
    web_accessible_resources: [{ resources: ['redirect.html'], matches: ['<all_urls>'], use_dynamic_url: true }],
    // Fixed ids, so the desktop app knows which extension may connect to it. In Chrome the id
    // comes from this public key (nfaamgjiehkppopelaampopbofjoepca); the app lists it in
    // apps/desktop/src-tauri/src/hosts.rs. The store zip leaves the key out and gets the id the
    // store gives it, which hosts.rs lists too.
    // Firefox asks every new extension what it collects. Seiza collects nothing: everything
    // stays in the browser, or goes to the desktop app on the same computer.
    ...(browser === 'firefox'
      ? {
          browser_specific_settings: {
            gecko: { id: 'lock-in@lock-in-focus-app.github.io', data_collection_permissions: { required: ['none'] } },
          },
        }
      : forStore
        ? {}
        : { key: chromeKey }),
  }),
});
