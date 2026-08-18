/* `＋ Add step` stawia kafelek LUZEM: żadnej strzałki, `links` nietknięte.
 *
 * Rozstrzygnięcie właściciela z 2026-08-19. Do tego dnia przycisk doklejał strzałkę od
 * ostatniego kroku w pliku, więc płótno umiało zbudować wyłącznie łańcuch — żeby dostać trzy
 * gałęzie wchodzące do jednego kroku, człowiek musiał najpierw skasować strzałkę, o którą nie
 * prosił. Powód, dla którego tamta strzałka w ogóle powstała, i powód, dla którego wolno ją
 * dziś zdjąć, stoją przy `addStep` w `connect.ts` oraz przy `one_folder_two_steps`
 * w `src-tauri/src/workflow/check.rs`.
 *
 * SŁABĄ WERSJĄ TEGO KRYTERIUM JEST `expect(next.links).toHaveLength(0)`. Przechodzi dla
 * dokumentu, który wszedł tu bez strzałek — czyli dla implementacji KASUJĄCEJ cudze strzałki
 * przy każdym dołożeniu kafelka, co jest awarią gorszą od tej, którą naprawiamy: człowiek traci
 * połączenia, które sam narysował. Dlatego dokument wejściowy MA strzałkę, a asercja stoi na
 * całej tablicy `links` porównanej z wejściem.
 *
 * Druga słaba wersja to sprawdzenie samych `links` bez patrzenia na kroki: przechodzi dla
 * funkcji, która nie dodaje niczego. Stąd osobna asercja na tym, że krok jednak powstał
 * i że jest w wyniku dokładnie jeden nowy.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { addStep } from './connect';

function step(id: string, name: string, at: { x: number; y: number }): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the work.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at,
  };
}

/** Dokument, w którym KTOŚ JUŻ NARYSOWAŁ strzałkę — bez tego cały ten plik mierzy zero. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [step('s_plan', 'Plan', { x: 24, y: 24 }), step('s_build', 'Build', { x: 24, y: 168 })],
    links: [{ from: 's_plan', to: 's_build' }],
  };
}

describe('+ Add step', () => {
  it('places the tile with no arrow of its own', () => {
    const before = file();

    const { file: next, step: added } = addStep('agent', before);

    expect(next.steps).toHaveLength(before.steps.length + 1);
    expect(next.steps.at(-1)?.id).toBe(added.id);
    expect(
      next.links.some((link) => link.from === added.id || link.to === added.id),
      'a tile that arrives already wired to the last step is exactly the editor that can only ' +
        'build a chain: to fan three branches into one step the person first has to delete an ' +
        'arrow nobody asked for. The arrow is the human’s to draw.',
    ).toBe(false);
  });

  it('leaves the arrows the person already drew exactly as they were', () => {
    const before = file();

    const { file: next } = addStep('agent', before);

    expect(
      next.links,
      'adding a tile is not an opinion about the existing arrows. An implementation that ' +
        'rebuilds `links` here loses connections the person drew by hand, which is a worse ' +
        'failure than the one this change fixes.',
    ).toEqual(before.links);
  });

  it('places a checkpoint the same way', () => {
    const before = file();

    const { file: next, step: added } = addStep('checkpoint', before);

    expect(added.kind).toBe('checkpoint');
    expect(
      next.links,
      'the two creating buttons differ in what they make, not in what they wire up; a ' +
        'checkpoint that arrives pre-attached is the same trap in the other button.',
    ).toEqual(before.links);
  });
});
