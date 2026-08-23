import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Strip as StripView } from './strip/model';
import { Strip } from './strip/strip';

/* AC-4 dla T-47: pasek loadoutu to JEDEN szklany torek, a segmentow jest tyle, ile krokow.
 *
 * NIE MA PASKA PROCENTOWEGO, bo kroki to nie procenty (DESIGN §2). Liczba segmentow pochodzi
 * z danych, nie z dlugosci napisu ani z liczby wpisanej w komponent — inaczej pasek rysowalby
 * relacje, ktorej w magazynie nie ma (niezmiennik 17).
 */

function stripOf(names: readonly string[], nowAt: number): StripView {
  return {
    blocks: names.map((name, at) => ({
      id: 'b' + String(at),
      name,
      state: at < nowAt ? 'done' : at === nowAt ? 'now' : 'todo',
      ended: false,
      wentWrong: false,
    })),
    caption: 'step ' + String(nowAt + 1) + ' of ' + String(names.length),
    spend: '',
  } as StripView;
}

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');

const mockup = (): string => (existsSync(MOCKUP) ? readFileSync(MOCKUP, 'utf8') : '');

/** Cialo reguly makiety o podanym selektorze, bez komentarzy. */
function ruleBody(selector: string): string {
  const css = mockup().replace(/\/\*[\s\S]*?\*\//g, ' ');
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(escaped + '\\s*\\{([^}]*)\\}').exec(css)?.[1] ?? '';
}

/** Nazwa tokenu, ktora makieta daje danemu stanowi segmentu. */
function tokenFor(selector: string): string {
  return /var\(--([a-z-]+)\)/.exec(ruleBody(selector))?.[1] ?? '';
}

/** Klasy segmentu o danym stanie. */
function segment(html: string, state: string): string {
  const re = new RegExp('data-block=\x22' + state + '\x22[^>]*class=\x22([^\x22]*)\x22');
  return re.exec(html)?.[1] ?? '';
}

const render = (strip: StripView): string =>
  renderToStaticMarkup(
    createElement(Strip, { strip, heading: 'Fix the CSV parser', controls: null }),
  );

/** Kontener segmentow: pierwszy element wewnatrz paska, ktory trzyma segmenty. */
function track(html: string): string {
  return /<div[^>]*data-blocks[^>]*>/.exec(html)?.[0] ?? '';
}

const segments = (html: string): number => [...html.matchAll(/data-block=/g)].length;

describe('pasek loadoutu', () => {
  const four = render(stripOf(['plan', 'research', 'build', 'check'], 2));

  it('holds the segments in ONE container that is glass and a capsule', () => {
    const shell = track(four);
    expect(
      shell,
      'the strip renders no marked container for its segments, so nothing says the segments are ' +
        'one track rather than four loose marks',
    ).not.toBe('');
    expect(
      /glass/.test(shell),
      'the segment track is not glass. The loadout bar is chrome, and chrome in this system is ' +
        'made of one material.',
    ).toBe(true);
    expect(
      /rounded-pill/.test(shell),
      'the segment track is not a capsule. The capsule is the shape this whole language repeats, ' +
        'and the track is the one place the bar shows it.',
    ).toBe(true);
  });

  it('gives all THREE states the value the mockup gives them', () => {
    /* POPRAWIONE po drugiej opinii: punkt nazywal trzy tokeny i sadzil jeden, a przy tym mylil
     * sie w obie strony — makieta daje skonczonemu wypelnienie `--muted`, a czekajacemu obrys
     * `--line-strong`. Wartosci sa teraz CZYTANE z rysunku, ktory jest wyrocznia wygladu: dzien,
     * w ktorym on zmieni zdanie, jest dniem, w ktorym ten punkt swieci na czerwono. */
    const wanted: ReadonlyArray<readonly [string, string]> = [
      ['now', tokenFor('.blk[data-s="now"] s')],
      ['done', tokenFor('.blk s')],
      ['todo', tokenFor('.blk[data-s="todo"] s')],
    ];
    const unread = wanted.filter(([, token]) => token === '').map(([state]) => state);
    expect(
      unread,
      'the mockup names no value for these segment states, so the comparison below would run ' +
        'against empty strings',
    ).toEqual([]);

    const wrong = wanted
      .filter(([state, token]) => !segment(four, state).includes(token))
      .map(
        ([state, token]) => state + ' should carry ' + token + ' but has: ' + segment(four, state),
      );
    expect(
      wrong,
      'these segment states disagree with the drawing. The three states of this bar are the ' +
        'screen signature; a pair that swaps or drops its value makes finished and waiting steps ' +
        'read at the wrong weight, and nothing else in the suite would say so.',
    ).toEqual([]);

    expect(
      /accent/.test(segment(four, 'now')),
      'the running segment carries the accent, which since 2026-08-19 means "this is ' +
        'interactive". A segment is a readout, not a control.',
    ).toBe(false);
  });

  it('draws exactly as many segments as the store holds, at two different lengths', () => {
    expect(
      segments(four),
      'the bar drew a different number of segments than the store holds. A bar that always draws ' +
        'four is a percentage bar wearing steps, and steps are not percentages (DESIGN §2).',
    ).toBe(4);
    const two = render(stripOf(['plan', 'build'], 1));
    expect(segments(two), 'the bar ignored a shorter workflow').toBe(2);
  });

  it('draws nothing when there are no steps', () => {
    expect(
      segments(render(stripOf([], 0))),
      'the bar drew segments for a workflow with no steps, which is a shape pretending to be a ' +
        'fact (invariant 17)',
    ).toBe(0);
  });
});
