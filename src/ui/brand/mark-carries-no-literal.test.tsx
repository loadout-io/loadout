import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { Mark } from './mark';

/* AC-3 dla T-49: znak w kodzie nie niesie ani jednego literalu i jest NEUTRALNY w chrome.
 *
 * Znak jest jedynym miejscem w `src/`, w ktorym gradient bylby naturalny — i wlasnie dlatego
 * gradientowa wersja mieszka w `docs/branding/`, czyli POZA `src/`. `checks/quick-tokens.sh`
 * odrzuca kazdy hex w komponencie; ten punkt sprawdza to z osobna, bo pokusa jest tu najwieksza.
 *
 * NEUTRALNOSC nie jest estetyka. Akcent znaczy „to jest interaktywne", coral „to sie dzieje
 * teraz" — a znak wisi w nawigacji takze wtedy, kiedy nic nie chodzi i nic nie jest klikalne.
 * Coral w znaku bylby po prostu nieprawda (DESIGN §3, niezmiennik 13).
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const SOURCE = resolve(ROOT, 'src/ui/brand/mark.tsx');

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

function withoutComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}

describe('znak w kodzie', () => {
  const source = withoutComments(text(SOURCE));
  const alone = renderToStaticMarkup(createElement(Mark, {}));
  const shell = renderToStaticMarkup(createElement(App, { section: 'run', screens: {} }));

  it('has a source to judge', () => {
    expect(source.length, 'src/ui/brand/mark.tsx is missing or empty').toBeGreaterThan(100);
  });

  it('states no colour of its own, in any spelling', () => {
    const hex = [...source.matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map((hit) => hit[0]);
    expect(
      hex,
      'the mark states a colour literally. Every colour in this app is a named value, and the ' +
        'mark is the one place where a gradient would feel natural — which is exactly why the ' +
        'gradient version lives outside src/.',
    ).toEqual([]);
    expect([...source.matchAll(/rgba?\(/g)].map((hit) => hit[0])).toEqual([]);
    expect(
      [...source.matchAll(/\b(?:text|fill|stroke|bg|border)-\[[^\]]+\]/g)].map((hit) => hit[0]),
      'the mark reaches for an arbitrary value, which is the same escape a hex is, written as a ' +
        'class',
    ).toEqual([]);
  });

  it('names exactly two values: one for the points, one for the lines', () => {
    expect(
      /fill-body/.test(source),
      'the round marks do not take the body text colour, so the mark either states its own or ' +
        'inherits something that is not a decision',
    ).toBe(true);
    expect(
      /stroke-muted/.test(source),
      'the joining lines do not take the muted text colour. Two values, two jobs: the round ' +
        'marks read as subject, the lines as the relation between them.',
    ).toBe(true);
    /* ZMIERZONE 2026-08-19 na wyrenderowanej powloce przy 22 px, czyli w jedynym rozmiarze,
     * w jakim znak stoi w aplikacji. Krawedzie brały wtedy `--color-line-strong`, czyli biel 16%
     * — wartosc, ktora w tym systemie rysuje wlos na krawedzi szkla. Linia 1,25 px w bieli 16%
     * na panelu daje kontrast okolo 1,7 : 1: znak czytal sie jako cztery kropki, czyli dokladnie
     * to, czym byl stary znak i czym miał przestac byc. Rodzina `line-*` jest OBRAMOWANIEM;
     * krawedzie tego rysunku sa jego tematem. */
    expect(
      [...source.matchAll(/stroke-line[\w-]*/g)].map((hit) => hit[0]),
      'the joining lines take a value out of the line family, which in this system draws the ' +
        'hairline on the edge of glass. At 22 px, the size the mark really stands at, such a ' +
        'line does not read at all and the mark falls back to four loose dots.',
    ).toEqual([]);
  });

  it('stays neutral: no accent, no happening-now colour', () => {
    for (const [what, mark] of [
      ['on its own', alone],
      /* SAM ZNAK, nie cala nawigacja: ona legalnie niesie akcent na glifie aktywnej sekcji
       * (T-46), wiec sprawdzanie jej calosci mierzylo nie ten element. viewBox 24 nalezy
       * wylacznie do znaku — glify sekcji maja 16. */
      ['inside the shell', /<svg[^>]*viewBox="0 0 24 24"[\s\S]*?<\/svg>/.exec(shell)?.[0] ?? ''],
    ] as const) {
      expect(
        /\baccent\b/.test(mark),
        'the mark carries the accent ' +
          what +
          '. The accent means "this is interactive"; the mark is not a control, it is the name of ' +
          'the product drawn as a picture.',
      ).toBe(false);
      expect(
        /\blive\b/.test(mark),
        'the mark carries the happening-now colour ' +
          what +
          '. It hangs in the navigation whether or not anything runs, so that colour on it would ' +
          'simply be untrue.',
      ).toBe(false);
    }
  });

  it('is hidden from a screen reader, because the logotype stands right beside it', () => {
    expect(
      /aria-hidden/.test(alone),
      'the mark is announced to assistive technology. The product name stands next to it in ' +
        'text, so a second name for the same thing is noise, not help.',
    ).toBe(true);
  });

  it('is really MOUNTED by the navigation, not merely importable', () => {
    const nav = shell.slice(0, shell.indexOf('</nav>') + 6);
    expect(nav.length, 'the shell rendered no nav at all').toBeGreaterThan(100);
    expect(
      /<svg[^>]*viewBox="0 0 24 24"/.test(nav),
      'the navigation does not carry the mark. Rendering it in a test proves it exists; only the ' +
        'shell proves anybody put it on screen.',
    ).toBe(true);
  });
});
