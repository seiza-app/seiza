import { matchesPattern, parseHttpUrl } from './match';
import { levelFor } from './paths';
import { sessionProfile } from './profiles';
import type { HiddenPart, Profile, Session, SiteRecipe, SiteRule, SiteSettings } from './types';

/**
 * While a site is tamed, the extension keeps the page's path in this attribute on
 * `<html>` and updates it as single-page apps navigate. CSS can't see the URL, so this
 * is how a recipe hides something on some pages only.
 */
export const pagePathAttribute = 'data-lock-in-path';

/**
 * Selectors end up inside a generated stylesheet, so anything that could close the rule
 * or start a new one (braces, semicolons, at-rules, comments, escapes) is rejected. This
 * matters for recipes imported from other people.
 */
export function isSafeSelector(selector: string): boolean {
  return selector.trim().length > 0 && !/[{};@\\]|\/\*/.test(selector);
}

export function findRecipe(recipes: readonly SiteRecipe[], url: string | URL): SiteRecipe | undefined {
  return recipes.find((recipe) => recipe.matches.some((pattern) => matchesPattern(pattern, url)));
}

/**
 * The value of every toggle in a recipe: the recipe's defaults, then the profile's
 * overrides, then any changes made for the current session.
 */
export function resolveToggles(
  recipe: SiteRecipe,
  rule: SiteRule | undefined,
  sessionOverrides?: Record<string, boolean>,
): Record<string, boolean> {
  return Object.fromEntries(
    recipe.toggles.map((toggle) => [
      toggle.id,
      sessionOverrides?.[toggle.id] ?? rule?.toggles?.[toggle.id] ?? toggle.default,
    ]),
  );
}

/**
 * Builds the stylesheet that tames a site with the given toggles switched on, and hides any parts
 * of the page someone picked.
 */
export function buildRecipeCss(
  recipe: SiteRecipe,
  toggles: Record<string, boolean>,
  hidden: readonly HiddenPart[] = [],
): string {
  const enabled = recipe.toggles.filter((toggle) => toggles[toggle.id]);
  const selectors = [
    ...enabled.flatMap((toggle) => toggle.selectors ?? []),
    ...hidden.map((part) => part.selector),
  ].filter(isSafeSelector);

  // One rule per selector, so a selector this browser can't read drops only its own rule.
  const rules = selectors.map((selector) => `${selector} {\n  display: none !important;\n}`);
  if (enabled.some((toggle) => toggle.effect === 'grayscale')) {
    rules.push('html {\n  filter: grayscale(1) !important;\n}');
  }
  return rules.join('\n\n');
}

/** The toggles for a site tamed all the time: its own, then the recipe's defaults. */
export function allTheTimeToggles(recipe: SiteRecipe, settings: SiteSettings | undefined): Record<string, boolean> {
  const toggles = settings?.toggles;
  return resolveToggles(recipe, toggles ? { recipeId: recipe.id, level: 'tamed', toggles } : undefined);
}

/**
 * The CSS to inject into a page: for the running session's profile, or with no session, for a
 * site tamed all the time. Parts picked to hide on the site are hidden either way. Empty when
 * the page isn't tamed.
 */
export function cssForPage(options: {
  url: string | URL;
  session: Session | null;
  profiles: readonly Profile[];
  recipes: readonly SiteRecipe[];
  siteSettings?: readonly SiteSettings[];
}): string {
  const { url, session, profiles, recipes, siteSettings = [] } = options;
  const recipe = findRecipe(recipes, url);
  if (!recipe) return '';
  const settings = siteSettings.find((site) => site.recipeId === recipe.id);

  if (!session) {
    return settings?.allTheTime ? buildRecipeCss(recipe, allTheTimeToggles(recipe, settings), settings.hidden) : '';
  }
  const profile = sessionProfile(profiles, session);
  const pathname = parseHttpUrl(String(url))?.pathname;
  if (!profile || levelFor(profile, recipe.id, pathname) !== 'tamed') return '';

  const rule = profile.sites.find((site) => site.recipeId === recipe.id);
  return buildRecipeCss(recipe, resolveToggles(recipe, rule, session.overrides?.[recipe.id]), settings?.hidden);
}
