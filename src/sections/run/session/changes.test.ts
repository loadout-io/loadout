/* „Co ten agent wyprodukował" bierze się ze ZMIAN NA DYSKU, nigdy z tego, co agent o sobie mówi.
 *
 * SŁABA WERSJA: policzyć wiersze i sprawdzić, że jest ich tyle, ile linii `edit`. Przechodzi
 * dla implementacji, która przypisuje każdemu plikowi liczby z całej czynności (`+42 −8` przy
 * trzech ścieżkach naraz), i dla takiej, która oddaje cudze zmiany pod nazwą tego agenta.
 * Odróżniają to trzy przypadki: sumowanie po ścieżce, cudza zmiana i linia wielościeżkowa.
 *
 * Najważniejszy jest ostatni przypadek pliku: agent, który mówi, że wszystko naprawił,
 * i nie zmienił ani jednego pliku, ma tu PUSTO. To jest cicha porażka numer jeden całego
 * ekranu — deklaracja postawiona w rubryce faktów czyta się jak fakt [00-SYNTHESIS §2.2].
 */
import { describe, expect, it } from 'vitest';

import type { FeedLine } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { changesOf } from './changes';

const FORGE = 'Forge';
const NEEDLE = 'Needle';

/** Linia `edit` dotykająca kilku plików naraz — jedna czynność, jedna para liczb. */
function editMany(id: number, agent: string, paths: readonly string[]): FeedLine {
  return {
    kind: 'edit',
    agent,
    text: 'Edited ' + String(paths.length) + ' files',
    count: paths.length,
    paths: [...paths],
    added: 9,
    removed: 3,
    detailId: 7,
    id,
    at: id * 1_000,
  };
}

describe('what an agent produced is read off the changes, not off its own words', () => {
  it('sums the numbers of a file the agent touched more than once', () => {
    const lines: readonly FeedLine[] = [
      line.edit(1, 0, FORGE, 'src/parser.rs', 42, 8),
      line.read(2, 100, FORGE, 'src/parser.rs'),
      line.edit(3, 200, FORGE, 'src/parser.rs', 6, 1),
    ];

    const changes = changesOf(lines, FORGE);

    expect(
      changes.length,
      'three edits of one file are one row about that file. A row per edit answers "what ' +
        'happened", which is what the record of what was said is for; this block answers "what ' +
        'is different on disk now".',
    ).toBe(1);
    expect(changes.at(0)?.path, 'named by its path').toBe('src/parser.rs');
    expect(
      { added: changes.at(0)?.added, removed: changes.at(0)?.removed },
      'and carrying both edits, not the last one. Showing only the last says +6 −1 about a ' +
        'file that gained 48 lines, and nothing on screen says it is only part of the story.',
    ).toEqual({ added: 48, removed: 9 });
  });

  it('leaves another agent’s change with that other agent', () => {
    const lines: readonly FeedLine[] = [
      line.edit(1, 0, FORGE, 'src/parser.rs', 42, 8),
      line.edit(2, 100, NEEDLE, 'tests/parser.rs', 6, 0),
    ];

    expect(
      changesOf(lines, FORGE).map((change) => change.path),
      'the work of another agent belongs to that agent. Rolling every change of the run into ' +
        'the screen of whichever agent is open gives one agent credit for the whole run.',
    ).toEqual(['src/parser.rs']);
    expect(
      changesOf(lines, NEEDLE).map((change) => change.path),
      'and the other way round',
    ).toEqual(['tests/parser.rs']);
  });

  it('keeps a many-file change as one row, because the numbers describe all of it', () => {
    const paths = ['src/a.rs', 'src/b.rs'];
    const changes = changesOf([editMany(1, FORGE, paths)], FORGE);

    expect(
      changes.length,
      'splitting it per file would print +9 −3 next to each of two files, which is a number ' +
        'the data does not carry for either of them (invariant 17) — and it looks exactly like ' +
        'data.',
    ).toBe(1);
    expect(changes.at(0)?.path, 'so the row names every file it really touched').toBe(
      'src/a.rs, src/b.rs',
    );
  });

  it('gives an agent that only talked nothing at all', () => {
    const lines: readonly FeedLine[] = [
      line.read(1, 0, FORGE, 'src/parser.rs'),
      line.note(2, 100, FORGE, 'I fixed everything.'),
    ];

    expect(
      changesOf(lines, FORGE),
      'the agent changed no file, so there is nothing under what it produced. Feeding this ' +
        'block the last thing the agent said is the single failure the whole screen exists to ' +
        'prevent.',
    ).toEqual([]);
  });
});
