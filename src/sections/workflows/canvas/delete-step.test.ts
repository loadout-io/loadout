/* Kasowanie kafelka — dwa zgłoszenia właściciela z 2026-08-22, jedna przyczyna.
 *
 * PIERWSZE: „usunę kafelek, niby OK, ale po wyjściu i wejściu w workflow dalej tam jest".
 * DRUGIE: uwaga `"Backend check" points at a step that is not in this workflow (s_11)`.
 *
 * Przyczyna obu jest ta sama i mieszka poza tym plikiem: kafelki i strzałki są na płótnie DWOMA
 * stanami Reacta, a `deleteElements` z React Flow wysyła `onNodesChange` i `onEdgesChange`
 * w jednej paczce. Każdy z tamtych handlerów składał wtedy CAŁY dokument ze swojej świeżej
 * połowy i cudzej nieświeżej: pierwszy dawał plik ze strzałkami w nic, drugi — plik z kafelkiem,
 * którego człowiek się właśnie pozbył. Wygrywał ten drugi.
 *
 * Dlatego kasowanie ma jedno wejście (`deleteFrom`) i liczy się Z DOKUMENTU. Kryteria niżej sądzą
 * tę funkcję oraz drugą połowę naprawy: mapper, który nie ma prawa wypuścić strzałki bez obu
 * końców, niezależnie od tego, którędy przyszła.
 *
 * SŁABĄ WERSJĄ jest policzenie kroków po skasowaniu. Przechodzi ją implementacja, która zostawia
 * w pliku strzałki po skasowanym kroku — czyli dokładnie ta, na którą właściciel zgłosił drugą
 * uwagę.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { deleteFrom, healed, toFile } from './map';

function step(id: string, name: string, y: number): AgentStep {
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
    at: { x: 24, y },
  };
}

/** `plan → build → checked`, plus powrót `checked → build`. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [
      step('plan', 'Plan', 24),
      step('build', 'Backend implementation', 168),
      step('checked', 'Backend check', 312),
    ],
    links: [
      { from: 'plan', to: 'build' },
      { from: 'build', to: 'checked' },
      { from: 'checked', to: 'build', max_turns: 3 },
    ],
  };
}

describe('a deleted tile leaves the document, and takes its arrows with it', () => {
  it('drops the step itself', () => {
    const after = deleteFrom({ steps: ['checked'] }, file());

    expect(
      after.steps.map((one) => one.id),
      'the tile the person deleted has to be gone from the DOCUMENT, not only from the view. ' +
        'While this was computed from two halves of canvas state, the half that still held the ' +
        'tile won and it came back the next time the workflow was opened',
    ).toEqual(['plan', 'build']);
  });

  it('drops every arrow that touched it, in both directions', () => {
    const after = deleteFrom({ steps: ['checked'] }, file());

    expect(
      after.links,
      'an arrow whose end no longer exists is what the validator reports as "points at a step ' +
        'that is not in this workflow", and the person never drew anything of the sort',
    ).toEqual([{ from: 'plan', to: 'build' }]);
  });

  it('drops a lone arrow without touching the steps it joined', () => {
    const after = deleteFrom({ arrows: [{ from: 'checked', to: 'build' }] }, file());

    expect(after.steps, 'deleting the way back is not deleting the check that writes it').toEqual(
      file().steps,
    );
    expect(
      after.links.filter((link) => link.max_turns !== undefined),
      'and the loop is gone, because that arrow WAS the loop',
    ).toEqual([]);
  });

  it('leaves the document it was handed exactly as it found it', () => {
    const before = file();
    const untouched = structuredClone(before);

    deleteFrom({ steps: ['checked'] }, before);

    expect(
      before,
      'pushing into the document it was given is how the view and the file end up disagreeing ' +
        'about what was deleted',
    ).toEqual(untouched);
  });

  it('takes the arrow pointing at nothing off a file that already carries one', () => {
    /* Dokładnie to, co leżało na dysku właściciela: krok skasowany, strzałka po nim została. */
    const broken: WorkflowFile = {
      ...file(),
      links: [...file().links, { from: 'checked', to: 's_11' }],
    };

    expect(
      healed(broken).links,
      'the note about an arrow pointing at a step that is not in this workflow hangs on the ' +
        'screen until somebody edits the file by hand, and the person never drew that arrow',
    ).toEqual(file().links);
  });

  it('hands back the very same document when there is nothing to heal', () => {
    const sound = file();

    expect(
      healed(sound),
      'the canvas decides whether anything changed by comparing references, so a fresh object ' +
        'on every open would save a document nobody touched',
    ).toBe(sound);
  });

  it('never writes an arrow whose end is not among the tiles, whichever way it got there', () => {
    const orphaned = toFile(
      file(),
      [
        { id: 'plan', position: { x: 24, y: 24 }, data: step('plan', 'Plan', 24) },
        { id: 'build', position: { x: 24, y: 168 }, data: step('build', 'Backend', 168) },
      ],
      [
        { id: 'plan->build', source: 'plan', target: 'build' },
        { id: 'build->gone', source: 'build', target: 'gone' },
      ],
    );

    expect(
      orphaned.links,
      'the mapper is the only road from the canvas into the file, so the narrowing lives here ' +
        'too: no future way of deleting can produce an arrow pointing at nothing',
    ).toEqual([{ from: 'plan', to: 'build' }]);
  });
});
