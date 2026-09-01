/* Kafelek postawiony przyciskiem daje dokument, KTÓRY SIĘ ZAPISUJE.
 *
 * PO CO TO KRYTERIUM ISTNIEJE — zgłoszenie właściciela, 2026-08-31. `＋ Start something`
 * i `＋ Run a check` stawiały kafelek, po którym cały plik przestawał się zapisywać: 400 ms
 * później (`AUTOSAVE_MS`) na ekranie stał czerwony pasek „This workflow was not saved…".
 * Przycisk karał za własne użycie, a razem z tym kafelkiem na dysk przestawała docierać CAŁA
 * reszta pracy — przesunięcia kafelków, nazwa, strzałki narysowane minutę wcześniej.
 *
 * PRZYCZYNY BYŁY DWIE, nie jedna, i obie są problemem przy ZAPISIE (`workflow::file::save`
 * odmawia na pierwszym `Level::Problem`, jeszcze przed `fs::write`):
 *   `check::a_command_step_left_empty` — pusta komenda (i pusty wzorzec przy „sprawdź");
 *   `check::nothing_before_it` — kafelek `same-copy`, przed którym nic nie stoi. Przycisk
 *      stawia kafelek LUZEM (rozstrzygnięcie właściciela z 2026-08-19), więc przed nim nie
 *      stoi nic NIGDY, a „w tej samej kopii, co krok przede mną" nazywa wtedy katalog,
 *      którego nie ma jak wyliczyć.
 *
 * DLACZEGO REGUŁY SĄ TU PRZEPISANE, skoro liczy je Rust. W vitest nie ma po drugiej stronie
 * granicy niczego: `check_workflow` to nazwa komendy, a nie walidator. Ta sama droga, którą
 * idzie `src/state/workflows-check-step-needs-a-pattern.test.ts` — mirror JEDNEJ reguły,
 * przy której stoi asercja czytająca `check.rs` z dysku. Kiedy tamten plik przestanie nieść
 * to zdanie, pierwszy `it` gaśnie na czerwono zamiast po cichu sądzić regułę, której już nie ma.
 *
 * SŁABĄ WERSJĄ jest `expect(added.command).not.toBe('')`. Przechodzi dla kafelka z komendą
 * i dalej pustym wzorcem, czyli dla pliku, który tak samo się nie zapisuje. Dlatego pytanie
 * jest zadane CAŁEMU DOKUMENTOWI i brzmi „czy Rust odmówiłby tego zapisu", a nie „czy to
 * jedno pole jest niepuste".
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import type { AgentStep, Step, WorkflowFile } from '../../../state/workflows';
import { addStep } from './connect';

/** Korzeń repo: ten plik leży w `src/sections/workflows/canvas/`, więc cztery katalogi wyżej. */
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');
const VALIDATOR = resolve(ROOT, 'src-tauri/src/workflow/check.rs');

const RUST = existsSync(VALIDATOR) ? readFileSync(VALIDATOR, 'utf8') : '';

/** Kotwice obu reguł w źródle Rusta. Nie zdania — nazwy funkcji, bo to one są regułą. */
const RULES = ['fn a_command_step_left_empty', 'fn nothing_before_it'];

/**
 * Zdania, którymi `workflow::file::save` odmówiłby tego pliku — puste, kiedy zapis przechodzi.
 *
 * Przepisane są DWIE reguły z `src-tauri/src/workflow/check.rs`, te i tylko te, które dotykają
 * kafelka prosto z przycisku. Reszta walidatora nie ma tu czego sądzić: świeży kafelek nie ma
 * ani strzałek, ani kopii, ani przelotki.
 */
function refusals(file: WorkflowFile): string[] {
  const said: string[] = [];
  /* Strzałki BEZ powrotów — to na nich Rust pyta „czy coś stoi przed tym krokiem". */
  const pointedAt = new Set(
    file.links.filter((link) => link.max_turns === undefined).map((link) => link.to),
  );

  for (const step of file.steps) {
    if (step.kind === 'serve' || step.kind === 'check') {
      const waitsForIt = step.kind === 'serve' && step.commandFrom !== undefined;
      if (!waitsForIt && step.command.trim() === '') {
        said.push(`"${step.name}" does not say what to run (check::a_command_step_left_empty)`);
      }
    }
    if (step.kind === 'check' && step.proof.trim() === '') {
      said.push(`"${step.name}" has no pattern (check::a_command_step_left_empty)`);
    }
    if (step.kind !== 'checkpoint' && step.folder.use === 'same-copy' && !pointedAt.has(step.id)) {
      said.push(
        `"${step.name}" works in the copy of a step that is not there (check::nothing_before_it)`,
      );
    }
  }
  return said;
}

function agentStep(id: string, name: string, y: number): AgentStep {
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
    at: { x: 24, y },
  };
}

/** Dokument, w którym ktoś już pracuje — taki, na jaki naprawdę klika się te przyciski. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [agentStep('s_plan', 'Plan', 24), agentStep('s_build', 'Build', 168)],
    links: [{ from: 's_plan', to: 's_build' }],
  };
}

/** Pola, których brak jest odmową zapisu — czytane BEZ ani jednego rzutowania. */
function fieldsOf(step: Step): Record<string, unknown> | null {
  if (step.kind === 'check') {
    return { command: step.command, proof: step.proof, folder: step.folder };
  }
  if (step.kind === 'serve') {
    return { command: step.command, folder: step.folder };
  }
  return null;
}

describe('a tile put down by a canvas button gives a document that saves', () => {
  it('runs against the two rules the validator really carries', () => {
    for (const rule of RULES) {
      expect(
        RUST.includes(rule),
        'src-tauri/src/workflow/check.rs no longer carries ' +
          rule +
          ', so the rules mirrored in this file judge something that is not there any more. ' +
          'Read that file and bring this mirror back in step before trusting anything below.',
      ).toBe(true);
    }
  });

  it('control: the empty tile this button used to make really was refused', () => {
    const empty: WorkflowFile = {
      ...file(),
      steps: [
        ...file().steps,
        {
          kind: 'check',
          id: 's_check',
          name: 'Run a check',
          command: '',
          proof: '',
          folder: { use: 'same-copy' },
          whenItFails: 'stop',
          at: { x: 24, y: 312 },
        },
      ],
    };

    expect(
      refusals(empty),
      'the mirror finds nothing wrong with a tile that has no command, no pattern and a folder ' +
        'it cannot work out — so every assertion below would pass on an empty rule set.',
    ).toHaveLength(3);
  });

  it('puts down a check that the file can be saved with', () => {
    const { file: next, step: added } = addStep('check', file());

    expect(
      refusals(next),
      'clicking "＋ Run a check" left a document the disk turns down, so 400 ms later the ' +
        'person reads "This workflow was not saved" and everything else on the canvas stops ' +
        'landing with it. The button punishes its own use.',
    ).toEqual([]);
    expect(
      fieldsOf(added),
      'the tile has to arrive with something in every field the file refuses without: a ' +
        'command to run, a pattern that says it passed, and a folder that means something for ' +
        'a tile with nothing in front of it. The two commands are the very ones the panel ' +
        'already writes in grey under the cursor, so the file gets the value the product ' +
        'itself recommends, not one invented here.',
    ).toEqual({
      command: 'npm test',
      proof: String.raw`(\d+) passed`,
      folder: { use: 'project' },
    });
  });

  it('puts down a tile that starts something, and the file saves with that too', () => {
    const { file: next, step: added } = addStep('serve', file());

    expect(
      refusals(next),
      'clicking "＋ Start something" left a document the disk turns down, with the same red ' +
        'bar 400 ms later.',
    ).toEqual([]);
    expect(
      fieldsOf(added),
      'the same rule for the tile that starts something and walks on: the command the panel ' +
        'already suggests, and a folder a tile with nothing before it can actually be in.',
    ).toEqual({ command: 'npm run dev', folder: { use: 'project' } });
  });

  it('leaves the arrows the person already drew exactly as they were', () => {
    const before = file();

    expect(
      addStep('check', before).file.links,
      'putting a tile down is not an opinion about the arrows',
    ).toEqual(before.links);
    expect(addStep('serve', before).file.links, 'and the same for the other button').toEqual(
      before.links,
    );
  });
});
