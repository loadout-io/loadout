/* Powrót przeżywa podróż płótno → plik → płótno. To jest kryterium na UTRATĘ DANYCH.
 *
 * ZMIERZONE, ZANIM POWSTAŁO. `toFile` odbudowuje `links` **z krawędzi płótna**
 * (`edges.map(...)`), a `CanvasEdge` nie znało `max_turns`. Każdy gest przechodzący przez `toFile`
 * — skasowanie kafelka (`canvas.tsx`, zmiana `remove` na kafelkach) i skasowanie strzałki
 * (to samo na strzałkach) — kasował więc limit rund z pliku.
 *
 * Skutek nie był cichy, był gorszy od cichego: plik zostawał z cyklem BEZ oznaczenia, walidator
 * Rusta dawał na to `Problem` (`workflow::check::a_circle`), a `workflow::file::save` odmawiał
 * przed zapisem. Czyli **skasowanie jednej niepowiązanej strzałki zamieniało workflow z pętlą
 * w plik, którego nie da się zapisać** — na ekranie zostawał pasek „this workflow was not saved",
 * a płótno rozjeżdżało się z dyskiem przy każdej następnej zmianie.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie samej podróży w jedną stronę (`toCanvas` niesie pole). Przechodzi
 * ją implementacja, w której `toFile` dalej gubi klucz — czyli ta, którą naprawiamy. Dlatego
 * kryterium niżej robi PEŁNĄ pętlę i porównuje `links` z wejściem.
 *
 * DRUGĄ SŁABĄ WERSJĄ jest fikstura, w której KAŻDA strzałka jest powrotem. Nie odróżnia
 * implementacji poprawnej od takiej, która dopisuje `max_turns` do wszystkiego — a ta druga
 * przepisałaby każdy istniejący workflow na dysku przy pierwszym zapisie i zamieniła każdą
 * strzałkę w potencjalną pętlę. Fikstura ma więc obie: jedną zwykłą i jedną wsteczną.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { toCanvas, toFile } from './map';

function step(id: string): AgentStep {
  return {
    kind: 'agent',
    id,
    name: id,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the work.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

/** `implement → tester`, i tester z powrotem do implementera, do trzech rund. */
function withALoop(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_loop',
    name: 'Implement and test',
    steps: [step('s_impl'), step('s_test')],
    links: [
      { from: 's_impl', to: 's_test' },
      { from: 's_test', to: 's_impl', max_turns: 3 },
    ],
  };
}

describe('a way back survives the trip through the canvas', () => {
  it('comes back out of the mappers with its limit intact', () => {
    const before = withALoop();

    const { nodes, edges } = toCanvas(before);
    const after = toFile(before, nodes, edges);

    expect(
      after.links,
      'the file is the truth. A gesture that drops this number leaves a circle nobody marked, ' +
        'which the validator refuses — so deleting one unrelated arrow would turn a workflow ' +
        'with a loop into a file that cannot be saved at all.',
    ).toEqual(before.links);
  });

  it('does not put the key on an ordinary arrow', () => {
    const before = withALoop();

    const { nodes, edges } = toCanvas(before);
    const after = toFile(before, nodes, edges);

    expect(
      Object.hasOwn(after.links[0] ?? {}, 'max_turns'),
      'writing the key onto every arrow would rewrite every workflow on disk at its first save ' +
        'and make each plain arrow a potential loop. Absent means absent.',
    ).toBe(false);
    expect(after.links[1]?.max_turns).toBe(3);
  });

  it('hands the limit to the canvas, so it has something to draw', () => {
    const { edges } = toCanvas(withALoop());

    expect(edges[0]?.maxTurns).toBeUndefined();
    expect(
      edges[1]?.maxTurns,
      'the canvas draws a way back differently from an ordinary arrow, so the edge has to know ' +
        'it is one; a mapper that only carries the field one way leaves the drawing blind',
    ).toBe(3);
  });
});
