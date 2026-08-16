/* Kryterium 3 dla T-13: pozycja przyciąga się do 24 px na KAŻDEJ drodze zapisu, także po
 * „Tidy up".
 *
 * Słaba wersja tego kryterium to `expect(snap({x:241.4,y:95.2})).toEqual({x:240,y:96})`. Funkcja
 * bywa wtedy bez zarzutu, a plik i tak churnuje: układ automatyczny zwraca zmiennoprzecinkowe
 * środki węzłów i zapisuje je z pominięciem mappera, więc po każdym „Tidy up" plik ma inne
 * dziesiąte miejsce po przecinku i diff bez treści [T3 §8.2]. Dlatego sprawdzamy DOKUMENT PO
 * AKCJI, nigdy funkcji w izolacji, i po `tidyUp()` przechodzimy pętlą po WSZYSTKICH krokach.
 *
 * Cztery wartości z TASK.md, wszystkie cztery użyte niżej: 241.4 → 240, 95.2 → 96, 12 → 24
 * (połowa skoku idzie w górę), 11.9 → 0 (tuż pod połową idzie do zera).
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, Point, WorkflowFile } from '../../../state/workflows';
import { GRID } from '../../../state/workflows';
import { onNodeDragStop } from './map';
import { tidyUp } from './tidy';

function step(id: string, name: string, at: Point): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the part of the work this tile owns.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at,
  };
}

/* Plan stoi NIŻEJ niż oba kroki, które po nim idą, i żadna pozycja nie leży na siatce. Oba
 * szczegóły są celowe: układ z góry na dół musi te kafelki przestawić, więc „tidyUp, które
 * tylko przycina to, co zastało" nie przejdzie. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [
      step('s_plan', 'Plan', { x: 22.5, y: 300.4 }),
      step('s_build', 'Build', { x: 21.9, y: 8.3 }),
      step('s_check', 'Check', { x: 300.7, y: 8.9 }),
    ],
    links: [
      { from: 's_plan', to: 's_build' },
      { from: 's_plan', to: 's_check' },
    ],
  };
}

function at(doc: WorkflowFile, id: string): Point {
  const hit = doc.steps.find((one) => one.id === id);
  if (hit === undefined) throw new Error('the document no longer holds ' + id);
  return hit.at;
}

describe('a position lands on the grid on every road into the file', () => {
  it('snaps where the tile was dropped, in the document, not only on screen', () => {
    const doc = file();

    expect(
      at(onNodeDragStop({ id: 's_plan', position: { x: 241.4, y: 95.2 } }, doc), 's_plan'),
      'the drag ends between two grid lines and the document has to hold the grid line, not ' +
        'the pointer. A file that remembers 241.4 shows a changed line for a mouse twitch',
    ).toEqual({ x: 240, y: 96 });

    expect(
      at(onNodeDragStop({ id: 's_plan', position: { x: 12, y: 11.9 } }, doc), 's_plan'),
      'exactly half a step rounds up and a hair under half rounds down — both have to be ' +
        'decided once, here, or two roads into the file will disagree by one grid line',
    ).toEqual({ x: 24, y: 0 });
  });

  it('leaves every position whole and on the grid after Tidy up, not just the dragged one', () => {
    const before = file();
    const tidied = tidyUp(before);

    expect(tidied.steps, 'tidying up moves tiles, it does not lose them').toHaveLength(
      before.steps.length,
    );

    for (const one of tidied.steps) {
      expect(
        Number.isInteger(one.at.x) && Number.isInteger(one.at.y),
        'the automatic layout hands back the centre of a box, which is a fraction. "' +
          one.name +
          '" kept one, and a fraction in the file is a changed line on every Tidy up',
      ).toBe(true);
      expect(
        one.at.x % GRID === 0 && one.at.y % GRID === 0,
        '"' +
          one.name +
          '" is a whole number but not a multiple of the grid, so the next drag moves it ' +
          'sideways by itself',
      ).toBe(true);
    }
  });

  it('puts every step below the one it waits for, which is what top-to-bottom means', () => {
    const tidied = tidyUp(file());

    for (const link of tidied.links) {
      expect(
        at(tidied, link.to).y > at(tidied, link.from).y,
        link.to +
          ' waits for ' +
          link.from +
          ' and has to stand below it. Without this line a Tidy up that only rounds the ' +
          'positions it was given would pass the whole check above',
      ).toBe(true);
    }
  });
});
