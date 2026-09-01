/* Filtr, który schował CAŁY wynik skanu, ma to powiedzieć — i dać się zdjąć jednym ruchem.
 *
 * ZMIERZONE, 2026-08-31. Ustaw „Needs attention", naciśnij Scan, dostań same gotowe pozycje —
 * i `<tbody>` renderuje się PUSTY. Ani jednego zdania: ani „nic nie znaleziono", ani „filtr to
 * schował". Liczniki nad tabelą mówią wtedy „17 Skills", a tabela pod nimi jest pusta, więc
 * jedyne, co człowiek może z tego wyczytać, to że skan się zepsuł. Nie zepsuł się — to filtr
 * sprzed skanu dalej stoi.
 *
 * DLACZEGO CZYSTY MODUŁ. Stan pustej tabeli powstaje dopiero po dwóch kliknięciach (pigułka
 * filtra, potem Scan), a w tym repo nie ma jsdom i `renderToStaticMarkup` nie odpala `onClick`.
 * Niezmiennik 29 dopuszcza czysty moduł jako dowód TREŚCI zdania — i to jest ta droga.
 */
import { describe, expect, it } from 'vitest';

import { FILTER_NAMES, SHOW_ALL, hiddenSays, hidesEverything, keptBy } from './shown';

interface Row {
  id: string;
  ready: boolean;
}

const ROWS: readonly Row[] = [
  { id: 'skill-1', ready: true },
  { id: 'skill-2', ready: true },
  { id: 'agent-1', ready: true },
];

const isReady = (row: Row): boolean => row.ready;

describe('the item list when a filter hides all of it', () => {
  it('keeps only what the filter asks for', () => {
    /* Kontrola przeciw pustej asercji: bez filtra widać wszystko. */
    expect(keptBy(ROWS, 'all', isReady), 'the list stopped showing anything at all').toHaveLength(
      3,
    );
    expect(keptBy(ROWS, 'ready', isReady)).toHaveLength(3);
    expect(keptBy(ROWS, 'attention', isReady)).toHaveLength(0);
  });

  it('knows the difference between an empty scan and a filter that hid the answer', () => {
    expect(
      hidesEverything(0, 3, 'attention'),
      'the screen cannot tell a scan that found nothing from a filter that hid everything, so ' +
        'it says the same nothing in both places',
    ).toBe(true);
    expect(
      hidesEverything(0, 0, 'attention'),
      'a project with no setup files would now be blamed on the filter',
    ).toBe(false);
    expect(hidesEverything(3, 3, 'all'), 'a full list is claiming to be hidden').toBe(false);
  });

  it('says which filter hid them, and offers to take it off', () => {
    const said = hiddenSays(3, 'attention');

    expect(said, 'the empty table says nothing about why it is empty').toContain('3 item(s)');
    expect(
      said,
      'the sentence does not name the filter that hid them, so the person has three pills to ' +
        'guess between',
    ).toContain(FILTER_NAMES.attention);
    expect(
      FILTER_NAMES.attention,
      'the sentence names a filter by some other word than the pill',
    ).toBe('Needs attention');
    expect(SHOW_ALL, 'nothing on screen takes the filter back off in one move').toBe(
      'Show all items',
    );
  });
});
