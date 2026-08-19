import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-3 dla T-46: plywa dokladnie JEDNA rzecz, i przy „mniej przejrzystosci" szklo znika w calosci.
 *
 * DESIGN §3 mowi: cien wylacznie pod tym, co PLYWA. W calej aplikacji plywa jedna rzecz — panel
 * nawigacji. Glebia wewnatrz strony bierze sie ze zmiany powierzchni, nie z cienia. Reguła nie
 * jest estetyczna: „glebia wszedzie" jest dokladnie tym, co zamienia szklo w ozdobe, a wtedy
 * przestaje cokolwiek znaczyc, ze jedna rzecz jest nad pozostalymi.
 *
 * DLACZEGO LISTA REGUL JEST CZYTANA, NIE WPISANA. Sprawdzenie, ktore pyta „czy `.nav` ma cien",
 * nie mowi nic o tym, ile innych rzeczy tez go ma. Ten test wylicza WSZYSTKIE reguly makiety
 * i odejmuje wylacznie te, ktore makieta sama nazywa swoim rusztowaniem (pasek `.mockbar`
 * i noty projektowe `.an`) — reszta jest aplikacja i podlega regule.
 *
 * Cudzyslowy w wyrazeniach regularnych zapisane heksadecymalnie (\x22, \x27): literal
 * o nieparzystej ich liczbie rozsynchronizowuje skaner `checks/quick-vocabulary.sh`.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function withoutComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, ' ');
}

interface Rule {
  readonly selector: string;
  readonly body: string;
}

/** Wszystkie reguly arkusza makiety poza jej wlasnym rusztowaniem. */
function appRules(html: string): readonly Rule[] {
  const style = /<style>([\s\S]*?)<\/style>/.exec(html)?.[1] ?? '';
  const css = withoutComments(style);
  const out: Rule[] = [];
  for (const hit of css.matchAll(/(^|\})\s*([^{}@]+?)\s*\{([^}]*)\}/g)) {
    const selector = (hit[2] ?? '').replace(/\s+/g, ' ').trim();
    if (selector === '') continue;
    /* Rusztowanie makiety, ktore makieta sama tak nazywa: „pasek makiety: NIE jest czescia
     * aplikacji" oraz noty projektowe. Nie podlegaja regule o cieniach, bo nie sa aplikacja. */
    if (/mockbar/.test(selector)) continue;
    if (/(^|[\s,])\.an(\b|[.:[])/.test(selector)) continue;
    out.push({ selector, body: hit[3] ?? '' });
  }
  return out;
}

/** Cienie reguly, ktore NIE sa `inset`. Refleks na krawedzi szkla nie jest glebia. */
function liftingShadows(body: string): readonly string[] {
  const declared = /(?:^|;)\s*box-shadow\s*:([^;]*)/.exec(body)?.[1] ?? '';
  if (declared.trim() === '') return [];
  return declared
    .split(/,(?![^(]*\))/)
    .map((part) => part.trim())
    .filter((part) => part !== '' && part !== 'none' && !/^inset\b/.test(part));
}

describe('plywa dokladnie jedna rzecz', () => {
  const html = fileText(MOCKUP);
  const rules = appRules(html);

  it('read a real sheet out of the mockup', () => {
    expect(
      rules.length,
      'almost no rule was read out of the mockup style sheet, so every point below would loop ' +
        'over an empty list and pass on nothing',
    ).toBeGreaterThan(20);
  });

  it('lifts the navigation panel, so it really does float', () => {
    const nav = rules.find((rule) => rule.selector === '.nav');
    expect(nav, 'the mockup has no .nav rule at all').toBeDefined();
    expect(
      liftingShadows(nav?.body ?? ''),
      'the navigation panel carries no lifting shadow, so it is drawn as a pane that floats and ' +
        'reads as a pane that lies flat',
    ).not.toEqual([]);
  });

  it('lifts NOTHING else, because depth everywhere is depth nowhere', () => {
    const lifted = rules
      .filter((rule) => rule.selector !== '.nav')
      .filter((rule) => liftingShadows(rule.body).length > 0)
      .map((rule) => rule.selector + ' -> ' + liftingShadows(rule.body).join(' , '));
    expect(
      lifted,
      'these rules carry a lifting shadow and they do not float. DESIGN §3 keeps shadows for ' +
        'the one thing that is above the page; anything else gets its depth from a change of ' +
        'surface. Inset shadows are allowed everywhere — a gleam on the edge of glass is not depth.',
    ).toEqual([]);
  });

  it('turns ALL the glass solid when the reader asked for less transparency', () => {
    const block =
      /@media\s*\(prefers-reduced-transparency:\s*reduce\)\s*\{([\s\S]*?)\n\}/.exec(
        withoutComments(html),
      )?.[1] ?? '';
    expect(
      block,
      'the mockup has no prefers-reduced-transparency block. It is a HIG requirement and the ' +
        'design system next door enforces it: a reader who turned transparency off gets solid ' +
        'panes no matter what the design wants.',
    ).not.toBe('');

    const glass = ['.nav', '.strip', '.rail'];
    const missed = glass.filter((selector) => !block.includes(selector));
    expect(
      missed,
      'these glass surfaces are not turned solid by the reduced-transparency block. Turning one ' +
        'of three solid is worse than turning none: the window then mixes two materials for one ' +
        'kind of surface.',
    ).toEqual([]);
  });
});
