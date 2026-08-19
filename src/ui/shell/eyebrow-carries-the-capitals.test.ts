import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-5 dla T-45: wersaliki nosi nadoczko sekcji, nie etykieta pola.
 *
 * TO JEST JEDYNA ZMIANA KONTRAKTU ISTNIEJACEJ WYROCZNI W T-45 i dlatego jest osobnym
 * kryterium, a nie dopiskiem. `src/ui/shell/type-ladder.test.ts` wymagal
 * `text-transform:uppercase` na PIECIU regulach makiety — `.fld label`, `.card .role`,
 * `.side h3`, `.rail h2`, `.ctx .ch` — i argumentowal to wlasnym komentarzem, cytujac
 * DESIGN §4: „etykieta pola, WERSALIKI". Po T-45 DESIGN §4 mowi co innego: `--text-label`
 * jest zdaniowe, a wersaliki nosi nowy stopien `--text-eyebrow`.
 *
 * PODZIAL: trzy reguly ZOSTAJA w wersalikach, bo sa nadoczkami sekcji. Dwie je TRACA, bo sa
 * etykieta pola i rola agenta. Wersaliki na kazdej etykiecie pola sa najczestszym ruchem
 * domyslnego panelu admina i pierwsza rzecza, po ktorej formularz przestaje wygladac jak macOS.
 *
 * SLABA WERSJA: skasowanie punktu o wersalikach z type-ladder.test.ts i napisanie nowego
 * o `--text-eyebrow`. Przechodzi, a jednoczesnie zdejmuje ochrone z TRZECH regul, ktore maja
 * w wersalikach zostac — bo nikt ich juz nie sadzi. Stad punkty (c) i (d) czytaja makiete
 * wprost, a punkt (e) pilnuje, zeby przepisana wyrocznia nie stracila kontroli przeciw
 * pustemu porownaniu.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const THEME = resolve(ROOT, 'src/styles/theme.css');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');
const LADDER = resolve(ROOT, 'src/ui/shell/type-ladder.test.ts');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function tight(value: string): string {
  return value.replace(/\s+/g, ' ').trim();
}

/* Komentarz NIE jest kodem, a ten test czyta CSS jako tekst.
 *
 * ZMIERZONE 2026-08-19, na tym pliku: komentarz w `theme.css` wyjasniajacy zmiane cytowal
 * regule doslownie — „BYLO: `.text-label { text-transform: uppercase }`" — i parser wzial ten
 * cytat za zywa regule. Test byl czerwony na kodzie, ktory jest POPRAWNY. `checks/quick-tokens.sh`
 * ma na to `strip_comments` i wlasnie dlatego: dokumentacja obok kodu jest w tym repo gesta,
 * wiec parser, ktory jej nie odejmuje, sadzi proze. */
function withoutComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, ' ');
}

/** Ciala wszystkich regul CSS o podanym selektorze. */
function ruleBodies(css: string, selector: string): readonly string[] {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const re = new RegExp(escaped + '\\s*\\{([^}]*)\\}', 'g');
  return [...css.matchAll(re)].map((hit) => hit[1] ?? '');
}

function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return tight(found?.[1] ?? '');
}

/** Nadoczka sekcji — te ZOSTAJA w wersalikach. */
const EYEBROWS = ['.side h3', '.rail h2', '.ctx .ch'] as const;
/** Etykieta pola i rola agenta — te je TRACA. */
const LABELS = ['.fld label', '.card .role'] as const;

describe('wersaliki nosi nadoczko', () => {
  const theme = withoutComments(fileText(THEME));
  const html = withoutComments(fileText(MOCKUP));

  it('read both files, or nothing below means anything', () => {
    expect(theme.length, 'src/styles/theme.css is empty or missing').toBeGreaterThan(100);
    expect(html.length, 'docs/mockup/index.html is empty or missing').toBeGreaterThan(100);
  });

  it('defines a --text-eyebrow rung at all', () => {
    expect(
      /--text-eyebrow\s*:/.test(theme),
      'there is no --text-eyebrow rung. Without it the ladder has one rung doing two jobs, and ' +
        'capitals go either on every field label or on none of them.',
    ).toBe(true);
  });

  it('puts the capitals on the eyebrow rung exactly once, in the components layer', () => {
    const declared = ruleBodies(theme, '.text-eyebrow')
      .map((body) => property(body, 'text-transform'))
      .filter((value) => value !== '');
    expect(
      declared,
      'the eyebrow rung does not carry capitals exactly once. Two declarations would be two ' +
        'places for one fact (invariant 13).',
    ).toEqual(['uppercase']);

    const components = /@layer components\s*\{([\s\S]*)\}/.exec(theme)?.[1] ?? '';
    expect(
      components,
      'nothing was read out of the @layer components block, so the assertion below would pass ' +
        'on an empty string',
    ).not.toBe('');
    expect(
      property(ruleBodies(components, '.text-eyebrow')[0] ?? '', 'text-transform'),
      'the capitals do not live in the components layer. Below the utilities layer they can be ' +
        'lifted with `normal-case`; inside it they cannot be lifted anywhere.',
    ).toBe('uppercase');
  });

  it('takes the capitals OFF the label rung', () => {
    const declared = ruleBodies(theme, '.text-label')
      .map((body) => property(body, 'text-transform'))
      .filter((value) => value !== '');
    expect(
      declared.filter((value) => value === 'uppercase'),
      'the label rung still carries capitals. A field label in capitals is the default move of ' +
        'an admin panel and the first thing that stops a form reading as macOS.',
    ).toEqual([]);
  });

  it('agrees with the mockup on which three rules keep the capitals', () => {
    const notUpper = EYEBROWS.filter(
      (selector) => property(ruleBodies(html, selector)[0] ?? '', 'text-transform') !== 'uppercase',
    );
    expect(
      notUpper,
      'these mockup rules are section eyebrows and no longer ask for capitals, so this point ' +
        'would be guarding a rule the oracle stopped wanting',
    ).toEqual([]);
  });

  it('agrees with the mockup on which two rules lose them', () => {
    const read = LABELS.map((selector) => [selector, ruleBodies(html, selector)[0] ?? ''] as const);
    const unread = read.filter(([, body]) => body === '').map(([selector]) => selector);
    expect(
      unread,
      'nothing was read out of these mockup rules, so the assertion below would pass on empty ' +
        'strings — the same empty-comparison failure this file exists to prevent',
    ).toEqual([]);

    const stillUpper = read
      .filter(([, body]) => property(body, 'text-transform') === 'uppercase')
      .map(([selector]) => selector);
    expect(
      stillUpper,
      'these mockup rules are a field label and an agent role, and they still say ' +
        'text-transform:uppercase. The mockup is the only oracle for looks, so as long as it ' +
        'asks for capitals here, the sheet is right to give them.',
    ).toEqual([]);
  });

  it('does not let the rewritten ladder lose its empty-comparison guards', () => {
    const ladder = fileText(LADDER);
    expect(ladder.length, 'src/ui/shell/type-ladder.test.ts is missing').toBeGreaterThan(100);

    const reads = [...ladder.matchAll(/ruleBod(?:y|ies)\(html,/g)].length;
    const guards = [...ladder.matchAll(/(?:nothing|no [a-z-]+) was read out of/g)].length;
    expect(
      reads,
      'the ladder no longer reads any rule out of the mockup, which means it stopped using the ' +
        'oracle at all',
    ).toBeGreaterThan(0);
    expect(
      guards,
      'the rewritten ladder has fewer empty-comparison guards (' +
        String(guards) +
        ') than reads from the mockup (' +
        String(reads) +
        '). Rewriting an oracle is the easiest place to weaken it silently: a test that ends up ' +
        'comparing two empty strings is green and checks nothing.',
    ).toBeGreaterThanOrEqual(reads);
  });
});
