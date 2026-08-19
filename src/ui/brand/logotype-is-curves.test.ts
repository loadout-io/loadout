import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';

/* AC-4 dla T-49: logotyp nie zalezy od kroju zainstalowanego w systemie.
 *
 * Kryterium brzmialo pierwotnie „jest krzywymi, nie tekstem" i nazywalo JEDNA implementacje.
 * W tym srodowisku nie ma czym zamienic tekstu na krzywe: brak `fontTools`, `brotli`,
 * `rsvg-convert`, Inkscape'a i ImageMagicka. Wymog stojacy za tym slowem brzmi: logotyp nie ma
 * prawa po cichu spasc na inny kroj — i to samo osiaga wbudowanie kroju jako `data:`, a osiaga
 * LEPIEJ, bo plik zostaje edytowalny.
 *
 * Powod jest zmierzony w tym repo: `theme.css` deklarowal Intera od pierwszego dnia i rysowal
 * sie krojem systemowym przez cale zycie repo, bez ani jednego bledu w konsoli.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const LOGO = resolve(ROOT, 'docs/branding/loadout-logo.svg');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');
const NAV = resolve(ROOT, 'src/ui/shell/titlebar.tsx');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/* Elementy bez zamkniecia. React renderuje puste elementy SVG jako `<path ... />`, wiec sam
 * ukosnik zalatwia wiekszosc — ta lista domyka HTML. */
const VOID = new Set(['br', 'hr', 'img', 'input', 'meta', 'link', 'source', 'area', 'col']);

/* Przodkowie elementu stojacego pod danym indeksem, jako ich atrybuty.
 *
 * Kryterium mowi „logotyp nie stoi w kroju maszynowym", a nie „element logotypu nie niesie klasy
 * mono": rodzina dziedziczy sie, wiec `font-mono` na czymkolwiek NAD logotypem robi dokladnie to,
 * czego to kryterium zabrania, i nie zostawia sladu na samym elemencie. */
function ancestors(markup: string, index: number): readonly string[] {
  const stack: string[] = [];
  const tag = /<(\/?)([a-zA-Z][\w-]*)([^>]*)>/g;
  let hit = tag.exec(markup);
  while (hit !== null && hit.index < index) {
    const [whole, slash, name, attrs] = hit;
    if (slash === '/') stack.pop();
    else if (!whole.endsWith('/>') && !VOID.has(name ?? '')) stack.push(attrs ?? '');
    hit = tag.exec(markup);
  }
  return stack;
}

describe('logotyp', () => {
  const logo = text(LOGO);

  it('exists at all', () => {
    expect(logo.length, 'docs/branding/loadout-logo.svg is missing or empty').toBeGreaterThan(200);
  });

  /* Bloki `@font-face` to DEKLARACJE kroju; wszystko poza nimi to UZYCIA. Do 2026-08-19 uzycia
   * czytane byly wylacznie z atrybutu `font-family="..."`, wiec nazwanie kroju w CSS-ie — czyli
   * tak, jak nazywa go ten wlasnie plik w regule `.w` — zostawialo liste uzyc PUSTA. A wtedy
   * kontrola sierot byla prozna, ORAZ przeskakiwany byl caly warunek na wbudowany krój, bo stal
   * pod `named.length > 0`: plik nazywajacy kroj i nie niosacy ani bajtu przechodzil jako „nie ma
   * ani jednej nazwy, bo jest krzywymi". To jest dokladnie ten defekt, przed ktorym to kryterium
   * stoi. */
  const declarations = [...logo.matchAll(/@font-face\s*\{([\s\S]*?)\}/g)].map(
    (hit) => hit[1] ?? '',
  );
  const outsideDeclarations = logo.replace(/@font-face\s*\{[\s\S]*?\}/g, ' ');
  const face = (block: string): string =>
    (/font-family\s*[=:]\s*\x22?([^\x22;}<]+)/.exec(block)?.[1] ?? '').trim();

  it('carries every face it names, so it cannot fall back to another one', () => {
    const named = [...outsideDeclarations.matchAll(/font-family\s*[=:]\s*\x22?([^\x22;}<]+)/g)].map(
      (hit) => (hit[1] ?? '').trim(),
    );
    const embedded = declarations.map(face);
    const orphans = named.filter((one) => !embedded.includes(one));
    expect(
      orphans,
      'these faces are named by the logotype and carried nowhere inside it: ' +
        JSON.stringify(orphans) +
        '. A named face without the file is a promise, and this repo already learned what that ' +
        'promise is worth — the app drew in the system face for its whole life and never said so.',
    ).toEqual([]);

    /* Kryterium daje dwie drogi — wbudowany kroj ALBO krzywe — i tu stoi ta dysjunkcja wprost.
     * Droga krzywych ma wlasny warunek: krzywe nie rysuja tekstu. Bez tego pusta lista nazw
     * przechodzila jako „to sa krzywe" takze dla pliku, ktory nazywa kroj w CSS-ie i nie niesie
     * ani bajtu — a to jest wlasnie ten defekt, przed ktorym to kryterium stoi. */
    if (named.length === 0) {
      expect(
        /<text\b/.test(logo),
        'the logotype names no face, so the only way it cannot fall back is by being curves — and ' +
          'it still draws live text',
      ).toBe(false);
    } else {
      expect(
        declarations.length,
        'the logotype names ' +
          JSON.stringify(named) +
          ' and embeds no face at all, so the file renders differently on every machine',
      ).toBeGreaterThan(0);
    }

    /* Obecnosc `data:font` nie wystarcza: `base64,` z pustym albo obcietym ladunkiem tez ja
     * spelnia. Kazdy zadeklarowany kroj musi wiec ROZKODOWAC sie do pliku woff2 — magia `wOF2`
     * stoi w pierwszych czterech bajtach. */
    for (const block of declarations) {
      const payload = /base64,([A-Za-z0-9+/=]+)/.exec(block)?.[1] ?? '';
      const raw = Buffer.from(payload, 'base64');
      expect(
        raw.length,
        'the face ' +
          face(block) +
          ' is declared with no usable font data, so the file renders ' +
          'differently on every machine',
      ).toBeGreaterThan(1000);
      expect(
        raw.subarray(0, 4).toString('latin1'),
        'the data carried for ' + face(block) + ' is not a woff2 file',
      ).toBe('wOF2');
    }

    /* I na kazdej WADZE, o ktora plik prosi. Kroj zmienny deklaruje zakres; kroj o jednej wadze
     * kazalby przegladarce dorobic pogrubienie samemu, a syntetyczny bold to nie jest ten sam
     * rysunek liter. */
    const range = declarations.flatMap((block) =>
      [...block.matchAll(/font-weight\s*:\s*(\d+)(?:\s+(\d+))?/g)].map((hit) => [
        Number(hit[1]),
        Number(hit[2] ?? hit[1]),
      ]),
    );
    const asked = [...outsideDeclarations.matchAll(/font-weight\s*[=:]\s*\x22?(\d+)/g)].map((hit) =>
      Number(hit[1]),
    );
    for (const weight of asked) {
      expect(
        range.some(([low, high]) => weight >= (low ?? 0) && weight <= (high ?? 0)),
        'the logotype asks for weight ' +
          String(weight) +
          ', which no embedded face declares. ' +
          'The browser would then draw a synthetic one, and a synthesised bold is a different ' +
          'drawing of the letters.',
      ).toBe(true);
    }
  });

  it('carries the mark, so the lockup is a lockup', () => {
    expect(
      [...logo.matchAll(/<circle\b/g)].length,
      'the logotype carries no four round marks, so it is a word without the mark',
    ).toBe(4);
  });

  /* NA WYRENDEROWANEJ POWLOCE, jak mowi kryterium — nie na zrodle jednego pliku.
   *
   * Do 2026-08-19 ten punkt czytal tekst `titlebar.tsx` regexpem, wiec `uppercase` albo
   * rozstrzelenie na tym samym elemencie przechodzilo (zabroniony byl tylko kroj maszynowy),
   * a kroj maszynowy ustawiony na przodku przechodzil tym bardziej. Dokladnie ten wyglad — WERSALIKI
   * z rozstrzeleniem .12em — to zadanie usuwa, wiec asercja musi go widziec. */
  it('is lowercase in the navigation, and not in a machine face', () => {
    const nav = text(NAV);
    expect(nav.length, 'the navigation source could not be read').toBeGreaterThan(100);
    const shell = renderToStaticMarkup(createElement(App, { section: 'run', screens: {} }));
    const brand = /<b class="([^"]*)">([^<]*)<\/b>/.exec(shell);
    expect(brand, 'no logotype element was found in the rendered shell').not.toBeNull();
    const classes = brand?.[1] ?? '';
    const word = brand?.[2] ?? '';
    expect(
      word,
      'the logotype in the navigation is not the product name in lowercase. Capitals with wide ' +
        'tracking are a quotation from a terminal, not a logotype.',
    ).toBe('loadout');
    expect(/[A-Z]/.test(word), 'the logotype carries a capital letter of its own').toBe(false);
    expect(
      /uppercase|text-eyebrow/.test(classes),
      'the logotype is turned into capitals by a class, so the word in the source says one thing ' +
        'and the window shows another',
    ).toBe(false);
    expect(
      /tracking-(?:wide|wider|widest|\[)/.test(classes),
      'the logotype is spread out. Wide tracking on capitals is the terminal quotation this task ' +
        'removes; a logotype is set tight.',
    ).toBe(false);

    const inherited = ancestors(shell, shell.indexOf(brand?.[0] ?? '')).filter((attrs) =>
      /font-mono|text-mono/.test(attrs),
    );
    expect(
      [...inherited, ...(/font-mono|text-mono/.test(classes) ? [classes] : [])],
      'the logotype stands in the machine-value face, or inherits it from something above it. ' +
        'In this system mono means a machine produced this and you can copy it; the product name ' +
        'is human language.',
    ).toEqual([]);
  });

  it('says the same thing in the drawing, which is the oracle for looks', () => {
    const html = text(MOCKUP);
    expect(html.length, 'the mockup could not be read').toBeGreaterThan(100);
    const brand = /<b>([^<]*)<\/b>/.exec(html.slice(html.indexOf('class="brand"')));
    expect(brand?.[1] ?? '', 'the mockup does not carry the logotype in lowercase').toBe('loadout');
    const rule = /\.brand b\{([^}]*)\}/.exec(html)?.[1] ?? '';
    expect(rule, 'no .brand b rule was read out of the mockup').not.toBe('');
    expect(
      /var\(--mono\)/.test(rule),
      'the mockup still sets the logotype in the machine-value face, so the drawing and the app ' +
        'disagree about the one element that is the product name',
    ).toBe(false);
  });
});
