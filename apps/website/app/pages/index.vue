<script setup lang="ts">
// Says what Seiza is in the words people search with ("website blocker", "focus app"), since few
// search for its name yet.
const description =
  'A free, open-source website and app blocker with a focus timer, for Mac and Linux. Tame or redirect distracting sites, and watch your night sky fill with stars.';

usePageSeo({ title: 'Seiza · A free website blocker and focus app for Mac and Linux', description });

// Tells search engines what Seiza is: the site's name, and the app, which is free.
useHead({
  script: [
    {
      type: 'application/ld+json',
      textContent: JSON.stringify({
        '@context': 'https://schema.org',
        '@graph': [
          { '@type': 'WebSite', name: 'Seiza', url: `${siteUrl}/` },
          {
            '@type': 'SoftwareApplication',
            name: 'Seiza',
            description,
            url: `${siteUrl}/`,
            image: `${siteUrl}/og.jpg`,
            applicationCategory: 'ProductivityApplication',
            operatingSystem: 'macOS, Linux',
            downloadUrl: 'https://github.com/seiza-app/seiza/releases/latest',
            license: 'https://www.gnu.org/licenses/agpl-3.0.html',
            isAccessibleForFree: true,
            offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
          },
        ],
      }),
    },
  ],
});
</script>

<template>
  <div class="site-page">
    <HomeHeroSection />
    <HomeTameSection />
    <HomePauseSection />
    <HomeSkySection />
    <HomeSatellitesSection />
    <HomePrivacySection />
    <HomeInstallSection />
  </div>
</template>
