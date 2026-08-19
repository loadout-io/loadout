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
    })),
    caption: 'step ' + String(nowAt + 1) + ' of ' + String(names.length),
    spend: '',
  } as StripView;
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

  it('colours the running segment with the happening-now colour, never the accent', () => {
    const now = /data-block="now"[^>]*class="([^"]*)"|class="([^"]*)"[^>]*data-block="now"/.exec(
      four,
    );
    const classes = (now?.[1] ?? now?.[2] ?? '') as string;
    expect(classes, 'no running segment was rendered, so nothing below is measured').not.toBe('');
    expect(
      /live/.test(classes),
      'the running segment does not carry the happening-now colour. It is the one saturated ' +
        'thing on this bar and the only reason the bar is a signature rather than a decoration.',
    ).toBe(true);
    expect(
      /accent/.test(classes),
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
