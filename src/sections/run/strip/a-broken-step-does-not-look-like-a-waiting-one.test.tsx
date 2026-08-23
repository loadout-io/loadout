/* Krok, który padł, nie ma prawa wyglądać jak krok, który czeka.
 *
 * # Skarga, z której to powstało
 *
 * Właściciel, 2026-08-23, o pasku kroków: „może jakiś lepszy ich widok bo teraz nie wiadomo
 * które jak chodzą w sumie".
 *
 * Zmierzone na kodzie sprzed tej zmiany: PIĘĆ z siedmiu stanów kroku — `pending`, `ready`,
 * `failed`, `cancelled`, `skipped` — rysowało się tym samym pustym obrysem, a jedyną różnicą
 * między krokiem, który padł, a krokiem, który jeszcze nie ruszył, była kreska przerywana
 * o grubości jednego piksela na pasku wysokim na osiem.
 *
 * # Co to kryterium mierzy, a czego nie
 *
 * Nie mierzy KOLORÓW. Wartości trzech stanów bloku sądzi `strip-is-one-glass-track` i czyta je
 * z makiety, więc powtarzanie tego tutaj byłoby drugim domem tej samej reguły. To kryterium
 * pyta o coś, o co nie pytało nic: czy stany, które znaczą co innego, WYGLĄDAJĄ inaczej.
 *
 * Nie żąda też, żeby siedem stanów dało siedem wyglądów. Trzy pary znaczą to samo i mają
 * wyglądać tak samo: `pending` i `ready` to jeden fakt („czeka na swoją kolej"), a `cancelled`
 * i `skipped` to drugi („już się nie wydarzy i nikt nie zawinił"). Zrównanie ich jest treścią,
 * nie oszczędnością — dlatego stoi tu jako asercja, a nie jako milczenie.
 *
 * # Dlaczego różnica ma być geometryczna
 *
 * `live-and-fail-never-share-a-form` mówi to wprost: „te dwa odcienie dzieli 13 stopni i stoją
 * w sąsiednich wierszach, więc jedyne, co je odróżnia, to forma — a forma, która znaczy oba, nie
 * znaczy żadnego". Pierwsza wersja tej naprawy dawała krokowi, który padł, sam kolor przy tej
 * samej formie i tamto kryterium ją odrzuciło. Ostatnia asercja niżej pilnuje tego z drugiej
 * strony: po ODJĘCIU barw wygląd zepsutego i biegnącego dalej musi się różnić.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { StepState } from '../../../state/run';
import { stripFor } from './model';
import { Strip } from './strip';

/** Siedem stanów kroku [ARCHITECTURE §5], w stałej kolejności. */
const EVERY: readonly StepState[] = [
  'succeeded',
  'running',
  'pending',
  'ready',
  'failed',
  'cancelled',
  'skipped',
];

/** Klasy każdego segmentu, w kolejności kroków. */
function looks(): readonly string[] {
  const html = renderToStaticMarkup(
    <Strip
      strip={stripFor(
        'Deep research',
        EVERY.map((state, at) => ({ id: 's' + String(at), name: state, state })),
      )}
      heading="Run"
      controls={null}
    />,
  );
  return [...html.matchAll(/<span data-block="[^"]*" class="([^"]*)"/g)].map((hit) => hit[1] ?? '');
}

/** Wygląd po odjęciu barw — czyli sama forma. */
function form(classes: string): string {
  return classes
    .split(/\s+/)
    .filter((one) => one !== '' && !/(^|-)(live|fail|muted|line-strong|accent)$/.test(one))
    .sort()
    .join(' ');
}

const seen = looks();
const of = (state: StepState): string => seen[EVERY.indexOf(state)] ?? '';

describe('the loadout strip shows which steps are which', () => {
  it('drew one segment per step, or everything below is about an empty list', () => {
    expect(
      seen.length,
      'the strip drew a different number of segments than there are steps, so the lookups ' +
        'below would compare empty strings with each other and pass',
    ).toBe(EVERY.length);
    expect(
      seen.filter((one) => one === '').length,
      'a segment came out with no classes at all; there is nothing to tell apart',
    ).toBe(0);
  });

  it('never lets a step that broke read as one that has not started', () => {
    expect(
      of('failed'),
      'a step that broke looks exactly like a step still waiting its turn. Those are the two ' +
        'furthest apart facts on this bar: one needs somebody now, the other needs nothing at ' +
        'all. Both drew: ' +
        JSON.stringify(of('failed')),
    ).not.toBe(of('pending'));
    expect(
      of('failed'),
      'and it looks like a step that somebody stopped on purpose. Stopping is a decision, not a ' +
        'fault, and the scheduler already keeps those two apart on the way here',
    ).not.toBe(of('cancelled'));
  });

  it('gives five looks to five facts, and the same look to the same fact twice', () => {
    expect(
      new Set([of('succeeded'), of('running'), of('pending'), of('failed'), of('cancelled')]).size,
      'finished, working, waiting, broke and stopped are five different things to know, and ' +
        'the bar has to say five different things. It said: ' +
        JSON.stringify([of('succeeded'), of('running'), of('pending'), of('failed')]),
    ).toBe(5);
    expect(
      of('ready'),
      'ready and pending are one fact — this step waits its turn — so two looks for them would ' +
        'invent a difference the person cannot act on (invariant 17)',
    ).toBe(of('pending'));
    expect(
      of('skipped'),
      'skipped and cancelled are one fact too: this step will not happen, and nobody did ' +
        'anything wrong',
    ).toBe(of('cancelled'));
  });

  it('tells the broken one from the working one with more than colour', () => {
    expect(
      form(of('failed')),
      'take the colours away and a broken step is shaped exactly like a working one. Those two ' +
        'hues sit 13 degrees apart, so anyone who cannot separate them has nothing left to read',
    ).not.toBe(form(of('running')));
  });
});
