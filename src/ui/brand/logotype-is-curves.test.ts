import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

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

describe('logotyp', () => {
  const logo = text(LOGO);

  it('exists at all', () => {
    expect(logo.length, 'docs/branding/loadout-logo.svg is missing or empty').toBeGreaterThan(200);
  });

  it('carries every face it names, so it cannot fall back to another one', () => {
    const named = [...logo.matchAll(/font-family="([^"]+)"/g)].map((hit) => hit[1] ?? '');
    const embedded = [...logo.matchAll(/@font-face[\s\S]*?font-family:\s*"([^"]+)"/g)].map(
      (hit) => hit[1] ?? '',
    );
    const orphans = named.filter((one) => !embedded.includes(one));
    expect(
      orphans,
      'these faces are named by the logotype and carried nowhere inside it: ' +
        JSON.stringify(orphans) +
        '. A named face without the file is a promise, and this repo already learned what that ' +
        'promise is worth — the app drew in the system face for its whole life and never said so.',
    ).toEqual([]);
    if (named.length > 0) {
      expect(
        /src:\s*url\(data:font/.test(logo),
        'the logotype declares a face and embeds no font data, so the file renders differently ' +
          'on every machine',
      ).toBe(true);
    }
  });

  it('carries the mark, so the lockup is a lockup', () => {
    expect(
      [...logo.matchAll(/<circle\b/g)].length,
      'the logotype carries no four round marks, so it is a word without the mark',
    ).toBe(4);
  });

  it('is lowercase in the navigation, and not in a machine face', () => {
    const nav = text(NAV);
    expect(nav.length, 'the navigation source could not be read').toBeGreaterThan(100);
    const brand = /<b className="([^"]*)">([^<]*)<\/b>/.exec(nav);
    expect(brand, 'no logotype element was found in the navigation').not.toBeNull();
    const classes = brand?.[1] ?? '';
    const word = brand?.[2] ?? '';
    expect(
      word,
      'the logotype in the navigation is not the product name in lowercase. Capitals with wide ' +
        'tracking are a quotation from a terminal, not a logotype.',
    ).toBe('loadout');
    expect(
      /font-mono|text-mono/.test(classes),
      'the logotype stands in the machine-value face. In this system mono means "a machine ' +
        'produced this and you can copy it"; the product name is human language.',
    ).toBe(false);
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
