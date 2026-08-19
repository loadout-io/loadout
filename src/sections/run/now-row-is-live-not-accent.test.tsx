import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { NowZone } from './feed/model';
import { Now } from './feed/now';

/* AC-2 dla T-47: strefa „teraz" ma DOKLADNIE JEDEN zywy region i nie rosnie.
 *
 * Pierwotne kryterium zadalo „dokladnie jeden WIERSZ niosacy `--live`" i bylo zadaniem o fakcie,
 * ktorego dane nie maja: `NowRow` to `{ agent, text }`, a ktory agent pracuje, a ktory czeka,
 * jest trescia zdania. Wyprowadzenie tego w widoku przez szukanie slowa `waiting` w napisie
 * wymyslilo by fakt (niezmiennik 17) i postawilo polityke „kto co robi" drugi raz, w komponencie
 * (niezmiennik 23). Regula, ktora dane utrzymuja, jest mocniejsza: strefa jest JEDNYM zywym
 * regionem na jeden fakt, a fakt brzmi „cos sie teraz dzieje" (niezmiennik 13, limit 1).
 */

const busy: NowZone = {
  rows: [
    { agent: 'Forge', text: 'writing src/parser.rs' },
    { agent: 'Needle', text: 'waiting on Forge' },
    { agent: 'Rivet', text: 'waiting on Needle' },
  ],
  thinking: 'Thinking',
};

const idle: NowZone = { rows: [], thinking: null };

const render = (zone: NowZone): string => renderToStaticMarkup(createElement(Now, { now: zone }));

const count = (html: string, pattern: RegExp): number => [...html.matchAll(pattern)].length;

const LIVE = /\b[a-z-]*live\b/g;
const ACCENT = /\b[a-z-]*accent\b/g;

describe('strefa teraz', () => {
  const working = render(busy);
  const quiet = render(idle);

  it('renders the rows it was given, and their words come from the store', () => {
    expect(working, 'the zone rendered nothing at all').not.toBe('');
    for (const row of busy.rows) {
      expect(
        working,
        'the zone does not carry the sentence the store gave for ' +
          row.agent +
          '. A zone writing its own words answers a different question than the one the store ' +
          'was asked.',
      ).toContain(row.text);
    }
  });

  it('carries the happening-now colour EXACTLY once while something runs', () => {
    const hits = count(working, LIVE);
    expect(
      hits,
      'the zone carries the happening-now colour ' +
        String(hits) +
        ' time(s). One live region per fact is the limit (invariant 13): two places saying "this ' +
        'is alive" is two answers to one question, and zero says the zone stopped answering it.',
    ).toBe(1);
  });

  it('never says "interactive" where it means "happening"', () => {
    expect(
      count(working, ACCENT),
      'the zone carries the accent. Since 2026-08-19 the accent means "this is interactive" and ' +
        'nothing else; the zone is not a control, it is a readout.',
    ).toBe(0);
  });

  it('says nothing is alive when nothing is', () => {
    expect(
      count(quiet, LIVE),
      'the zone still carries the happening-now colour with no rows at all, so the colour stops ' +
        'meaning anything: it is on screen whether or not something runs.',
    ).toBe(0);
    expect(
      /<span[^>]*font-mono/.test(quiet),
      'the zone renders a row with an empty store, which is a placeholder pretending to be a ' +
        'fact (invariant 17)',
    ).toBe(false);
  });

  it('does not grow with the number of rows', () => {
    /* DESIGN §1: „stala wysokosc, nadpisywana w miejscu". Strefa, ktora rosnie, przesuwa
     * strumien pod soba i cala teza tego ekranu przestaje byc prawdziwa. */
    const shell = /<div[^>]*data-now[^>]*>/.exec(working)?.[0] ?? '';
    expect(shell, 'no data-now container was rendered').not.toBe('');
    expect(
      /shrink-0|height/.test(shell),
      'the zone declares neither shrink-0 nor a height, so it grows with its content and pushes ' +
        'the history above it — which is exactly the scrolling this screen is designed against.',
    ).toBe(true);
  });
});
