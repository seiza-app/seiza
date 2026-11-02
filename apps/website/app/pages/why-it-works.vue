<script setup lang="ts">
// The research behind the features, from PLAN §12, in plain words: each finding with its source
// and how strong the evidence is, and what's been left out or hasn't held up.
definePageMeta({ lockOnArrival: true });

usePageSeo({
  title: 'Why it works · Seiza',
  description:
    "The research behind Seiza, in plain words: what each feature rests on, how strong the evidence is, and what has been left out because it didn't hold up.",
});

type Strength = 'strong' | 'modest' | 'reasoning';

interface Finding {
  title: string;
  detail: string;
  strength: Strength;
  why: string;
  inSeiza: string;
  source: { cite: string; href?: string };
}

const STRENGTH: Record<Strength, string> = {
  strong: 'Strong',
  modest: 'Modest',
  reasoning: 'Reasoning',
};

const FINDINGS: Finding[] = [
  {
    title: 'A pause and an easy way out cut how often you give in',
    detail:
      'In a six-week field study of the one sec app, people opened the apps they chose 57% less. The option to back out mattered most and the wait helped; the message on its own did nothing.',
    strength: 'modest',
    why: 'one study',
    inSeiza: 'The redirect page: five minutes of focus first, and Not now always right there.',
    source: {
      cite: 'Grüning, Riedel & Lorenz-Spreen, 2023, PNAS',
      href: 'https://doi.org/10.1073/pnas.2213114120',
    },
  },
  {
    title: 'Limits you set yourself work',
    detail:
      'Letting people cap their own future screen time cut their social media use a lot, and short-term help to cut back kept working after it ended.',
    strength: 'strong',
    why: 'a large field experiment',
    inSeiza: 'Profiles, Hard mode and its passes, and Restricted sites.',
    source: {
      cite: 'Allcott, Gentzkow & Song, 2022, American Economic Review',
      href: 'https://www.nber.org/papers/w28936',
    },
  },
  {
    title: 'Habits are set off by their surroundings',
    detail:
      'Habits form from repeating something in the same context, and after that the context itself sets them off. Taking the cue away works better than fighting it every time.',
    strength: 'strong',
    why: 'a review',
    inSeiza: "Taming a site: its feeds, Shorts and recommendations are hidden, so the cue isn't there.",
    source: {
      cite: 'Wood & Rünger, 2016, Annual Review of Psychology',
      href: 'https://dornsife.usc.edu/wendy-wood/wp-content/uploads/sites/183/2023/10/wood.runger.2016.pdf',
    },
  },
  {
    title: "Deciding ahead of time what you'll do",
    detail:
      'An if-then plan, "when X happens, I\'ll do Y", had a medium-to-large effect on reaching goals across 94 tests, one of the strongest results in the field.',
    strength: 'strong',
    why: 'a meta-analysis',
    inSeiza:
      'An if-then plan on each profile, shown on the redirect page, and offered by the weekly report when a pattern is clear.',
    source: {
      cite: 'Gollwitzer & Sheeran, 2006, Advances in Experimental Social Psychology',
      href: 'https://doi.org/10.1016/S0065-2601(06)38002-1',
    },
  },
  {
    title: 'A specific goal beats "do your best"',
    detail: 'Setting a clear, specific goal leads to doing more than a vague intention to try hard.',
    strength: 'strong',
    why: 'decades of studies',
    inSeiza:
      'Session goals: a one-line goal at the start and "did you do it?" after. A switch on each profile, off by default.',
    source: { cite: 'Locke & Latham, 2002, American Psychologist', href: 'https://doi.org/10.1037/0003-066X.57.9.705' },
  },
  {
    title: 'Writing a plan down quiets an unfinished task',
    detail: 'Making a plan for an unfinished task stops it from pulling at your attention while you do something else.',
    strength: 'modest',
    why: 'a set of lab studies',
    inSeiza: 'Satellites: a thought for later, launched with Alt+P, back when the session ends.',
    source: {
      cite: 'Masicampo & Baumeister, 2011, Journal of Personality and Social Psychology',
      href: 'https://doi.org/10.1037/a0024192',
    },
  },
  {
    title: 'Tracking your progress helps you get there',
    detail:
      "Keeping track of progress helps people reach their goals, more so when it's written down. Across 138 studies.",
    strength: 'strong',
    why: 'a meta-analysis',
    inSeiza: 'The weekly report, and the sky itself.',
    source: { cite: 'Harkin et al., 2016, Psychological Bulletin', href: 'https://doi.org/10.1037/bul0000025' },
  },
  {
    title: "Habits take a while, and one missed day doesn't undo one",
    detail:
      "In the best-known study, a new habit took a median of about 66 days, anywhere from 18 to 254, and missing a day now and then didn't set it back.",
    strength: 'modest',
    why: 'a small study, not yet repeated',
    inSeiza: 'The sky never shows a broken streak, and nothing here promises a habit in 21 days.',
    source: {
      cite: 'Lally et al., 2010, European Journal of Social Psychology',
      href: 'https://doi.org/10.1002/ejsp.674',
    },
  },
  {
    title: 'Being kind to yourself after a slip',
    detail: 'Students who forgave themselves for putting off studying put it off less before the next exam.',
    strength: 'modest',
    why: 'correlational',
    inSeiza: 'A session that ends early just ends: no scolding, and nothing lost beyond that session.',
    source: {
      cite: 'Wohl, Pychyl & Bennett, 2010, Personality and Individual Differences',
      href: 'https://doi.org/10.1016/j.paid.2010.01.029',
    },
  },
  {
    title: 'Starting small makes starting easier',
    detail:
      "The hard part is usually starting, and a pull that comes from a moment of boredom tends to pass once you're back into the work. This is the two-minute rule from Atomic Habits; it hasn't been tested as such.",
    strength: 'reasoning',
    why: 'not tested',
    inSeiza: "The quiet timer, a short timer you're free to run past, and the redirect page's five minutes.",
    source: { cite: 'James Clear, Atomic Habits (2018)' },
  },
  {
    title: 'People keep at what they chose themselves',
    detail: 'Motivation from your own reasons lasts; pressure and control from outside wear it down.',
    strength: 'strong',
    why: 'a large body of work',
    inSeiza: 'Seiza only acts on what you set up, never nudges, and every default can be changed.',
    source: {
      cite: 'Ryan & Deci, 2000, self-determination theory, American Psychologist',
      href: 'https://doi.org/10.1037/0003-066X.55.1.68',
    },
  },
];
</script>

<template>
  <div class="site-page reading">
    <section id="top" class="page-head" data-name="Top">
      <p class="kicker mono reveal"><b>✦</b> · Why it works</p>
      <h1 class="reveal" style="--d: 80ms">The research behind Seiza</h1>
      <p class="lede reveal" style="--d: 160ms">
        Seiza should help because of research that has held up, not because it sounds like it should. Here's what each
        feature rests on, how strong the evidence is, and what's been left out because it didn't hold up.
      </p>
      <dl class="key reveal" style="--d: 240ms">
        <div>
          <dt><span class="strength strong">Strong</span></dt>
          <dd>A large meta-analysis, review or field experiment.</dd>
        </div>
        <div>
          <dt><span class="strength modest">Modest</span></dt>
          <dd>Real, but small, or a single study.</dd>
        </div>
        <div>
          <dt><span class="strength reasoning">Reasoning</span></dt>
          <dd>Sensible, but not tested.</dd>
        </div>
      </dl>
    </section>

    <section id="findings" data-name="Findings">
      <p class="kicker mono reveal"><b>01</b> · What Seiza does, and why</p>
      <h2 class="reveal" style="--d: 80ms">Each feature, and what it rests on</h2>
      <ol class="findings">
        <li
          v-for="(finding, i) in FINDINGS"
          :key="finding.title"
          class="finding hud glass reveal"
          :style="{ '--d': `${120 + (i % 2) * 80}ms` }"
        >
          <p class="strength" :class="finding.strength">
            {{ STRENGTH[finding.strength] }} <span>· {{ finding.why }}</span>
          </p>
          <h3>{{ finding.title }}</h3>
          <p>{{ finding.detail }}</p>
          <p class="in-seiza"><span class="mono">In Seiza</span> {{ finding.inSeiza }}</p>
          <p class="cite mono">
            <a v-if="finding.source.href" :href="finding.source.href">{{ finding.source.cite }} ↗</a>
            <template v-else>{{ finding.source.cite }}</template>
          </p>
        </li>
      </ol>
    </section>

    <section id="left-out" data-name="Left out">
      <p class="kicker mono reveal"><b>02</b> · Considered and left out</p>
      <h2 class="reveal" style="--d: 80ms">Good ideas that don't belong here</h2>
      <ul class="plain">
        <li class="reveal" style="--d: 120ms">
          <strong>Planning the week at a fresh start,</strong> like a Monday or a new month. Seiza isn't a planning app.
        </li>
        <li class="reveal" style="--d: 160ms">
          <strong>Break timers between sessions.</strong> Short breaks help more with tiredness than with getting things
          done, and they'd mean more settings.
        </li>
        <li class="reveal" style="--d: 200ms">
          <strong>A separate rule for forgiving slips.</strong> Seiza already doesn't punish them.
        </li>
        <li class="reveal" style="--d: 240ms">
          <strong>Habit goals,</strong> like earning a site by going for a run. Seiza is for focus, and building habits
          is another app's job. Pairing a treat with a chore does work, but a larger follow-up found a smaller effect
          than the first study:
          <a href="https://repository.upenn.edu/oid_papers/150">Milkman, Minson &amp; Volpp, 2014</a>.
        </li>
      </ul>
    </section>

    <section id="not-held-up" data-name="Not held up">
      <p class="kicker mono reveal"><b>03</b> · What hasn't held up</p>
      <h2 class="reveal" style="--d: 80ms">Popular, but not built on</h2>
      <ul class="plain">
        <li class="reveal" style="--d: 120ms">
          <strong>Willpower as a fuel tank that runs out</strong> ("ego depletion"). A replication across 23 labs found
          an effect of about zero (<a href="https://doi.org/10.1177/1745691616652873">Hagger et al., 2016</a>).
        </li>
        <li class="reveal" style="--d: 160ms">
          <strong>"21 days to form a habit."</strong> It doesn't come from research; the study above found anywhere from
          18 to 254.
        </li>
        <li class="reveal" style="--d: 200ms">
          <strong>Deadlines you set yourself beating procrastination,</strong> often cited for tools like this one. A
          replication didn't find it, and the authors have asked for the paper to be retracted (<a
            href="https://datacolada.org/138"
            >Data Colada</a
          >). The study of limits you set yourself, above, is the one to go by.
        </li>
      </ul>
    </section>

    <section id="kept-out" data-name="Kept out">
      <p class="kicker mono reveal"><b>04</b> · What's kept out</p>
      <h2 class="reveal" style="--d: 80ms">Things Seiza won't do</h2>
      <ul class="plain">
        <li class="reveal" style="--d: 120ms">
          <strong>Points, badges and leaderboards.</strong> Rewards you expect and can count can wear down your own
          reasons for doing something (<a href="https://doi.org/10.1037/0033-2909.125.6.627"
            >Deci, Koestner &amp; Ryan, 1999</a
          >; still debated). The sky reflects what you did; it isn't a currency to chase.
        </li>
        <li class="reveal" style="--d: 160ms">
          <strong>Streaks you're afraid to lose.</strong> You'll see your current and longest streak, never a broken
          one.
        </li>
        <li class="reveal" style="--d: 200ms">
          <strong>Nudges and nagging.</strong> Seiza only acts on what you set up.
        </li>
        <li class="reveal" style="--d: 240ms">
          <strong>The slot-machine pull.</strong> Surprises like comets and meteor showers come only from focusing,
          never from visiting a site, and nothing tells you to chase them.
        </li>
      </ul>
      <p class="aside reveal" style="--d: 300ms">
        Each finding was checked against its paper in October 2026. If you know of research that changes any of this,
        <a href="https://github.com/seiza-app/seiza/issues">open an issue</a>.
      </p>
      <SiteFooter class="reveal" style="--d: 360ms" />
    </section>
  </div>
</template>
