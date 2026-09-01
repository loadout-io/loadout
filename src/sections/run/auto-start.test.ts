/* Który wiersz uruchamia bieg sam, a który tylko go proponuje.
 *
 * # Rozstrzygnięcie, którego to pilnuje
 *
 * Właściciel, 2026-08-30, na pytanie „po rozmowie z liderem — klikasz przycisk, czy bieg rusza
 * sam?": **„rusza samo"**. Cofa to część rozstrzygnięcia z 2026-08-19 — tę i tylko tę, która
 * mówiła, KTO może zacząć bieg.
 *
 * # Czego to rozstrzygnięcie NIE cofa, i to jest tu ważniejsze
 *
 * Proza dalej nie uruchamia niczego. Właściciel odrzucił to wprost, zobaczywszy skutek: „jak
 * piszę bez komendy… to się na nowo całe workflow odpala". Wiersz rozpoznany z prozy niesie
 * `auto: false` i dostaje przycisk; sam uruchamia się wyłącznie wiersz, który powstał z wywołania
 * czasownika `start_workflow` — czyli z jawnej decyzji lidera.
 *
 * Dlatego drugi przypadek w tym pliku waży więcej niż pierwszy: „propozycja z prozy NIE rusza"
 * jest zdaniem o pieniądzach i o zaufaniu, a „decyzja lidera rusza" jest zdaniem o wygodzie.
 */
import { describe, expect, it } from 'vitest';

import { autoStarts } from './auto-start';

/** Wiersz w kształcie, w jakim przychodzi z drutu. */
function suggested(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    kind: 'suggested',
    agent: 'Lead',
    text: 'Starting Ship a feature',
    command: '/run ship-a-feature build the parser',
    auto: true,
    ...over,
  };
}

describe('a row that starts a run by itself', () => {
  it('runs the command the lead decided on, signed by the lead', () => {
    expect(autoStarts([suggested()])).toEqual([
      { command: '/run ship-a-feature build the parser', agent: 'Lead' },
    ]);
  });

  it('never runs a suggestion that came out of prose', () => {
    expect(
      autoStarts([suggested({ auto: false })]),
      'a row recognised from prose gets a button, not a run. The person rejected the other ' +
        'behaviour on 2026-08-19 after watching it happen: writing a sentence without a command ' +
        'restarted the whole workflow',
    ).toEqual([]);
  });

  it('never runs anything that only looks like the flag', () => {
    const lookalikes = [
      suggested({ auto: 'true' }),
      suggested({ auto: 1 }),
      suggested({ auto: {} }),
      suggested({ auto: undefined }),
    ];
    expect(
      autoStarts(lookalikes),
      'only the boolean true starts a run. A truthy value here means a row from some other ' +
        'version of Rust can start work nobody asked for',
    ).toEqual([]);
  });

  it('ignores rows of every other kind, even carrying a command', () => {
    expect(autoStarts([suggested({ kind: 'note' }), suggested({ kind: 'told' })])).toEqual([]);
  });

  it('refuses an empty command instead of starting whatever comes first', () => {
    expect(
      autoStarts([suggested({ command: '   ' })]),
      'an empty command reaches the start policy as "no name given", and that starts the first ' +
        'runnable workflow on the shelf — which is exactly not the one anybody was talking about',
    ).toEqual([]);
  });

  it('survives a batch that is not what this version expects', () => {
    expect(
      autoStarts([null, 42, 'text', undefined, [], suggested()]),
      'one unreadable row must not take the window down with it, and must not stop the row ' +
        'behind it from being read',
    ).toEqual([{ command: '/run ship-a-feature build the parser', agent: 'Lead' }]);
  });

  it('keeps the order the lines arrived in', () => {
    const batch = [suggested({ command: '/run first' }), suggested({ command: '/run second' })];
    expect(autoStarts(batch).map((one) => one.command)).toEqual(['/run first', '/run second']);
  });
});
