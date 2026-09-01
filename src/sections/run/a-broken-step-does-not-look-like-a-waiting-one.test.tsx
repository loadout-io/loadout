/* Krok, który padł, nie ma prawa wyglądać jak krok, który czeka.
 *
 * # Skarga, z której to powstało
 *
 * Właściciel, 2026-08-23, o pasku kroków: „może jakiś lepszy ich widok bo teraz nie wiadomo
 * które jak chodzą w sumie".
 *
 * Zmierzone na kodzie sprzed tamtej zmiany: PIĘĆ z siedmiu stanów kroku — `pending`, `ready`,
 * `failed`, `cancelled`, `skipped` — rysowało się tym samym pustym obrysem, a jedyną różnicą
 * między krokiem, który padł, a krokiem, który jeszcze nie ruszył, była kreska przerywana
 * o grubości jednego piksela na pasku wysokim na osiem.
 *
 * 2026-08-31 — TO SAMO PYTANIE, NOWA POWIERZCHNIA. Torek bloków w pasku loadoutu zniknął: był
 * drugim rysunkiem planu obok obrazu w kolumnie pracy, przy limicie jednego żywego regionu na
 * fakt (niezmiennik 13). Kroki widać dziś na kafelkach planu, więc pytanie „czy stany, które
 * znaczą co innego, WYGLĄDAJĄ inaczej" przeniosło się na nie — razem z całą listą siedmiu
 * stanów i z trzema parami, które mają wyglądać tak samo.
 *
 * # Co to kryterium mierzy, a czego nie
 *
 * Nie mierzy KOLORÓW z osobna. Pyta o coś innego: czy dwa różne fakty da się od siebie odróżnić
 * na ekranie w ogóle.
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
 * znaczy żadnego". Ostatnia asercja niżej pilnuje tego z drugiej strony: po ODJĘCIU barw wygląd
 * kroku, który padł, i kroku, który pracuje, musi się różnić.
 *
 * # Dlaczego CAŁY ekran, a nie sam kafelek
 *
 * Kafelek wyrenderowany wprost przechodzi także wtedy, gdy nikt go nigdy nie montuje. Plan bez
 * pozycji — a taki okno składa dla wpisanego pytania — rysuje się listą tych samych kafelków,
 * więc lista jest drogą, po której człowiek te formy naprawdę widzi (niezmiennik 29).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Step, StepState } from '../../state/run';
import { useRun } from '../../state/run';
import Run from './index';

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

/* TA SAMA NAZWA SIEDEM RAZY. Gdyby kroki różniły się nazwami, ich markup różniłby się z powodu
 * nazwy, a punkt o rozróżnialności przechodziłby nad kafelkiem, który o stanie nie mówi ani
 * jednym pikselem. Pozycji nie ma ani jedna: obraz milczy o kształcie i pokazuje listę. */
const NAME = 'Build the parser';
const STEPS: readonly Step[] = EVERY.map((state, at) => ({
  id: 's' + String(at),
  name: NAME,
  state,
}));

useRun.setState({ workflow: 'Deep research', steps: STEPS, links: null });
const MARKUP = renderToStaticMarkup(<Run />);

/**
 * Wygląd kafelka każdego kroku, w kolejności planu — BEZ jego klucza.
 *
 * Klucz jest w każdym kafelku inny i tylko on; zostawiony w wycinku dałby siedem różnych
 * napisów z siedmiu identycznych kafelków, czyli zieleń na niczym.
 *
 * WYCINEK KOŃCZY SIĘ NA PIERWSZYM `</div>`, czyli na wierszu nazwy. Stoją w nim obie rzeczy,
 * którymi kafelek mówi o stanie: komplet klas karty (podkład, obrys, przygaszenie) i glif
 * przy nazwie. Wycinek do następnego kafelka byłby dla OSTATNIEGO z nich całą resztą ekranu.
 */
function looks(): readonly string[] {
  return MARKUP.split('data-step="')
    .slice(1)
    .map((chunk) => chunk.slice(chunk.indexOf('"') + 1))
    .map((chunk) => chunk.slice(0, chunk.indexOf('</div>')));
}

/** Wygląd po odjęciu barw — czyli sama forma. */
function form(markup: string): string {
  return markup.replace(/\b[a-z-]*(live|fail|attend|muted|accent)[a-z-]*\b/g, '');
}

const seen = looks();
const of = (state: StepState): string => seen[EVERY.indexOf(state)] ?? '';

describe('the picture of the plan shows which steps are which', () => {
  it('drew one card per step, or everything below is about an empty list', () => {
    expect(
      seen.length,
      'the screen drew a different number of cards than the run has steps, so the lookups ' +
        'below would compare empty strings with each other and pass',
    ).toBe(EVERY.length);
    expect(
      seen.filter((one) => one === '').length,
      'a card came out with nothing in it at all; there is nothing to tell apart',
    ).toBe(0);
  });

  it('never lets a step that broke read as one that has not started', () => {
    expect(
      of('failed'),
      'a step that broke looks exactly like a step still waiting its turn. Those are the two ' +
        'furthest apart facts here: one needs somebody now, the other needs nothing at all. ' +
        'Both drew: ' +
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
        'the screen has to say five different things. It said: ' +
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
