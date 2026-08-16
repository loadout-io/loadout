/* Kryterium 6: dziesięć tysięcy linii nie rusza ani pamięci, ani tożsamości wierszy.
 *
 * `expect(lines.length).toBe(2000)` jest słabe w sposób, który w produkcji wygląda jak cisza:
 * przechodzi dla implementacji, która obcina OGON. Zostają wtedy dwa tysiące najstarszych linii,
 * strumień zamiera w połowie biegu, długość się zgadza i nikt tego nie zauważa, bo widok
 * wygląda dokładnie jak bieg, który się zatrzymał. Rozróżnia to `lines[1999].id === 10000`.
 *
 * Druga asercja jest o Reakcie, nie o pamięci: obiekt linii, który przetrwał paczkę, musi być
 * TYM SAMYM obiektem. Implementacja przemapowująca całą historię przy każdej paczce jest
 * poprawna co do wartości i katastrofalna przy czterech agentach — każdy wiersz dostaje nową
 * tożsamość dwadzieścia razy na sekundę.
 *
 * Limit 2000 wolno mieć tylko dlatego, że reszta leży na dysku (niezmiennik 4). Dlatego okno
 * bez licznika tego, co z niego wypadło, jest złamaniem, a nie oszczędnością: „Load earlier"
 * nie ma wtedy o co poprosić. Stąd `droppedBefore` i `earliestKnownId` w tym samym sprawdzeniu.
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { createRunStore } from '../../../state/run';
import { line } from './fixtures/lines';
import { agentAt } from './fixtures/run-200';

const BATCH = 2_000;
const BATCHES = 5;

/** Paczka `howMany` linii, licząc od identyfikatora `from`. */
function batch(from: number, howMany: number = BATCH): FeedLine[] {
  return Array.from({ length: howMany }, (_, i) =>
    line.read(from + i, (from + i) * 10, agentAt(i), 'src/file-' + String(from + i) + '.rs'),
  );
}

describe('ten thousand lines move neither the memory nor the identity of a row', () => {
  it('keeps the newest two thousand and remembers how many it let go', () => {
    const store = createRunStore();
    for (let i = 0; i < BATCHES; i += 1) store.getState().appendLines(batch(i * BATCH + 1));
    const state = store.getState();

    expect(state.lines.length, 'the window is two thousand lines wide [T2 §6.3]').toBe(BATCH);
    expect(
      state.lines.at(-1)?.id,
      'the NEWEST line is the one that survives. An implementation that trims the tail keeps ' +
        'the two thousand oldest, the stream dies halfway through the run, and the length ' +
        'above still reads two thousand',
    ).toBe(BATCH * BATCHES);
    expect(state.lines[0]?.id, 'and the window starts right after what was let go').toBe(8_001);
    expect(
      state.droppedBefore,
      'how many fell out of the head. Without this number "Load earlier" has nothing to offer ' +
        'and no reason to exist (invariant 16)',
    ).toBe(8_000);
    expect(
      state.earliestKnownId,
      'and this is what it asks for: the oldest line still in hand. Files are the truth, ' +
        'memory is a window on them (invariant 4)',
    ).toBe(8_001);
  });

  it('holds ids that climb, one at a time, with nothing repeated', () => {
    const store = createRunStore();
    for (let i = 0; i < BATCHES; i += 1) store.getState().appendLines(batch(i * BATCH + 1));
    const ids = store.getState().lines.map((row) => row.id);

    expect(
      ids.filter((id, i) => i > 0 && id <= (ids[i - 1] ?? id)),
      'ids climb strictly. A window that reorders on the way in makes "Load earlier" ask for ' +
        'a page it already has',
    ).toEqual([]);
    expect(new Set(ids).size, 'and no line arrives twice').toBe(ids.length);
  });

  it('stores the very objects it was handed, and keeps them across the next batch', () => {
    const store = createRunStore();
    for (let i = 0; i < BATCHES - 1; i += 1) store.getState().appendLines(batch(i * BATCH + 1));

    const fifth = batch(8_001);
    const handed = fifth.find((row) => row.id === 9_000);
    store.getState().appendLines(fifth);

    expect(
      store.getState().lines.find((row) => row.id === 9_000),
      'the row in the window IS the object that came in — not a copy that carries the same ' +
        'values. Remapping the whole history on every batch is correct and ruinous: at four ' +
        'agents it hands every visible row a new identity twenty times a second',
    ).toBe(handed);

    /* I jeszcze raz, po paczce, którą ta linia PRZEŻYŁA — bo „ten sam obiekt zaraz po
     * włożeniu" przechodzi też dla implementacji, która przepisuje okno dopiero przy obcinaniu. */
    store.getState().appendLines(batch(10_001, 10));
    expect(
      store.getState().lines.find((row) => row.id === 9_000),
      'and it is still the same object one batch later, after the window slid past it',
    ).toBe(handed);
  });
});
