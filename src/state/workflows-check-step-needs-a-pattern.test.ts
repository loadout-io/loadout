/* Sprawdzenie bez wzorca jest ODMOWĄ Z NAZWĄ POLA, nie cichym plikiem.
 *
 * DLACZEGO TO JEST NAJWAŻNIEJSZE POLE TEGO KAFELKA. Bez wzorca wynik liczyłby się z samego
 * powrotu komendy, a suita, która nie uruchomiła ani jednego testu, wraca szczęśliwa
 * (niezmiennik 19). Kafelek, który tak orzeka, jest gorszy niż jego brak: wygląda na dowód,
 * a jest życzeniem. Dlatego Rust odmawia ZAPISU takiego pliku, a nie dopiero uruchomienia —
 * i dlatego to okno ma o tej odmowie powiedzieć, zamiast zapisywać w tle plik, którego nie ma.
 *
 * ZDANIE JEST CZYTANE Z RUSTA, NIE PRZEPISANE TUTAJ. `check.rs` układa je z nazwy kafelka
 * i nazwy pola; przepisane tu z palca zgadzałoby się z ekranem zawsze, także w dniu, w którym
 * te dwie kopie by się rozjechały — a rozjazd znaczy, że człowiek szuka pola, którego nie widzi.
 * Ta sama droga i ten sam powód, co w `e2e/tests/skill-refusal-survives-a-real-click.spec.ts`.
 *
 * CZYM JEST ATRAPA GRANICY, powiedziane wprost. `vi.mock` stoi na `sections/workflows/io.ts`,
 * czyli na jedynym miejscu w sekcji, które zna nazwy komend Rusta — tą samą drogą, co atrapy
 * w `skills.test.ts` i `memory.test.ts`. Atrapa odpowiada JEDNĄ regułą walidatora, tą, o którą
 * pyta to kryterium (`check::a_command_step_left_empty`), i odpowiada JEJ WŁASNYMI zdaniami,
 * wyjętymi z jej pliku. `save` odmawia zdaniem PIERWSZEGO problemu i nie dotyka „dysku" —
 * dokładnie tak, jak `workflow::file::save` odmawia przed `fs::write`.
 *
 * SŁABĄ WERSJĄ jest reguła, która odmawia zawsze: przechodzi pierwszy przypadek i nie mówi nic
 * o produkcie. Rozstrzyga trzeci `it`, w drugą stronę — po wpisaniu wzorca plik ma wylądować,
 * ze wzorcem w środku.
 *
 * DRUGĄ SŁABĄ WERSJĄ jest zatrzymanie się na wartości zwróconej przez magazyn. Wartość dowodzi,
 * że mechanizm istnieje; zdanie na ekranie dowodzi, że produkt działa (niezmiennik 29). Dlatego
 * drugi `it` renderuje pasek, na którym człowiek to czyta, i pyta o kolor kropki oraz o to,
 * dokąd ta kropka prowadzi.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { freshId, freshStep } from '../sections/workflows/canvas/connect';
import { RunBar, focusNote } from '../sections/workflows/canvas/problems';
import * as disk from '../sections/workflows/io';
import type { Note, Step, WorkflowFile, WorkflowIo } from './workflows';
import { createWorkflowStore } from './workflows';

/** Korzeń repo: ten plik leży w `src/state/`, więc dwa katalogi wyżej. */
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const VALIDATOR = resolve(ROOT, 'src-tauri/src/workflow/check.rs');

/**
 * Zdanie walidatora, złożone tak, jak złoży je kompilator.
 *
 * Dwie rzeczy do zdjęcia i obie zmieniają treść: `\` na końcu linii skleja ją z następną razem
 * z jej wcięciem, a `\"` w środku jest cudzysłowem, który człowiek naprawdę zobaczy. `{}` na
 * miejscu nazwy kafelka zostaje — wypełnia je [`sentenceFor`].
 */
function validatorSays(anchor: string): string {
  const source = existsSync(VALIDATOR) ? readFileSync(VALIDATOR, 'utf8') : '';
  const joined = source.replace(/\\\r?\n\s*/g, '');
  for (const found of joined.matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
    const body = found[1] ?? '';
    if (body.includes(anchor)) return body.replace(/\\"/g, '"');
  }
  return '';
}

const NO_PATTERN = validatorSays('does not say how to tell that the work really ran');
const NO_COMMAND = validatorSays('does not say what to run, so there would be nothing to start');

function sentenceFor(template: string, name: string): string {
  return template.replace('{}', name);
}

const spy = vi.hoisted(() => ({
  /** Każda próba zapisu, także ta, która skończyła się odmową. */
  tried: [] as { path: string; file: WorkflowFile }[],
  /** Tylko to, co naprawdę wylądowało — odmowa pada PRZED dotknięciem dysku. */
  written: [] as { path: string; file: WorkflowFile }[],
  /** Dokumenty, które przeszły przez granicę do walidatora. */
  checked: [] as WorkflowFile[],
}));

/* Jedna reguła walidatora, jej własnymi zdaniami. Deklaracje funkcji, nie stałe: fabryka atrapy
 * jest wynoszona ponad importy, więc wolno jej domykać się wyłącznie na tym, co też jest
 * wyniesione. Same zdania czytamy dopiero w chwili wywołania. */
function notesFrom(workflow: WorkflowFile): Note[] {
  const notes: Note[] = [];
  for (const step of workflow.steps) {
    if (step.kind !== 'check') continue;
    if (step.command.trim() === '') {
      notes.push({
        level: 'problem',
        stepId: step.id,
        message: sentenceFor(NO_COMMAND, step.name),
      });
    }
    if (step.proof.trim() === '') {
      notes.push({
        level: 'problem',
        stepId: step.id,
        message: sentenceFor(NO_PATTERN, step.name),
      });
    }
  }
  return notes;
}

/** Zdanie, którym zapis odmawia, albo `null`. Pierwszy problem, jak po tamtej stronie. */
function refusalFor(workflow: WorkflowFile): string | null {
  return notesFrom(workflow).at(0)?.message ?? null;
}

vi.mock('../sections/workflows/io', () => ({
  write: (path: string, workflow: WorkflowFile) => {
    spy.tried.push({ path, file: workflow });
    const said = refusalFor(workflow);
    /* Odmowa jedzie NAPISEM, bo tak odrzuca Tauri: skorupy komend robią `to_string()`, więc
     * `error instanceof Error` po tej stronie jest zawsze fałszywe. */
    if (said !== null) return Promise.reject(said);
    spy.written.push({ path, file: workflow });
    return Promise.resolve();
  },
  check: (workflow: WorkflowFile) => {
    spy.checked.push(workflow);
    return Promise.resolve(notesFrom(workflow));
  },
}));

const PATH = 'ship-a-feature.json';
const COMMAND = './verify.sh full';
const PROOF = String.raw`(\d+) passed`;

const START: WorkflowFile = {
  format: 1,
  id: 'wf_ship_a_feature',
  name: 'Ship a feature',
  steps: [],
  links: [],
};

/** Kafelek prosto z przycisku płótna — TĄ SAMĄ funkcją, którą woła przycisk. */
const PLACED = freshStep('check', freshId(START), { x: 24, y: 24 });

/** Ten sam kafelek z wpisaną komendą i bez wzorca: jedyny stan, w którym brakuje DOKŁADNIE
 * jednej rzeczy, więc odmowa ma o czym mówić pojedynczo. */
const TYPED: Step = PLACED.kind === 'check' ? { ...PLACED, command: COMMAND } : PLACED;

/** I ten sam z wpisanym wzorcem — kontrola w drugą stronę. */
const FILLED: Step = TYPED.kind === 'check' ? { ...TYPED, proof: PROOF } : TYPED;

const noop = () => undefined;

function docWith(step: Step): WorkflowFile {
  return { ...START, steps: [step] };
}

/** Magazyn wpięty w granicę dokładnie tak, jak wpina go ekran edytora. */
function openWith(step: Step) {
  const io: WorkflowIo = {
    save: (file) => disk.write(PATH, file),
    check: disk.check,
    /* Nietykany: edycja kroku nie ma prawa zapisać pliku agenta. */
    saveAgent: () => Promise.resolve(),
  };
  return createWorkflowStore(io, docWith(step));
}

/** Markup po zdjęciu ucieczek, których React nakłada na cudzysłowy i apostrofy.
 *
 * Zdanie walidatora niesie cudzysłowy wokół nazwy kafelka i wokół nazwy pola, a React zamienia
 * je w encje — porównanie surowego markupu z surowym zdaniem nie mogłoby więc przejść nigdy. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&amp;/g, '&');
}

/** Wzorzec zapisany przy kafelku o danym id — albo `null`, kiedy to nie jest sprawdzenie. */
function proofIn(file: WorkflowFile | undefined, id: string): string | null {
  const step = file?.steps.find((one) => one.id === id);
  return step === undefined || step.kind !== 'check' ? null : step.proof;
}

beforeEach(() => {
  spy.tried.length = 0;
  spy.written.length = 0;
  spy.checked.length = 0;
});

describe('a check tile with no pattern is refused by name, not saved in silence', () => {
  it('runs on the wording the validator really carries', () => {
    expect(
      NO_PATTERN,
      'nothing was read out of src-tauri/src/workflow/check.rs, so every comparison below would ' +
        'run against an empty string and pass on nothing. Either that file moved, or the ' +
        'refusal stopped being made of one sentence.',
    ).not.toBe('');
    expect(
      NO_PATTERN.includes('{}'),
      'the sentence read out of the validator has no place for the name of the tile, so it ' +
        'could not tell a person WHICH tile to open. It reads: ' +
        NO_PATTERN,
    ).toBe(true);
    expect(
      NO_COMMAND,
      'nothing was read out of the validator for a tile with no command either, so the fake ' +
        'boundary below would answer one half of the rule and stay quiet about the other.',
    ).not.toBe('');
  });

  it('carries this kind of tile across the boundary and is told what is missing', async () => {
    const store = openWith(TYPED);

    await store.getState().recheck();

    expect(
      spy.checked.at(0)?.steps.map((step) => step.kind),
      'the document the window handed to the validator carries no check tile at all, so nothing ' +
        'on the other side had anything to say about one. A kind the window does not know is a ' +
        'kind that leaves through the same door it came in by, and the person is left with a ' +
        'tile that draws on the canvas and means nothing.',
    ).toEqual(['check']);
    expect(
      store.getState().notes,
      'the answer from the validator did not reach the state the screen reads. One problem, on ' +
        'this tile, in the words the validator wrote: the window neither counts these nor ' +
        'translates them.',
    ).toEqual([
      { level: 'problem', stepId: TYPED.id, message: sentenceFor(NO_PATTERN, TYPED.name) },
    ]);
  });

  it('puts that sentence where a person reads it, with a dot that leads to the tile', async () => {
    const store = openWith(TYPED);
    await store.getState().recheck();
    const notes = store.getState().notes;

    expect(
      notes.length,
      'nothing came back for this tile, so there is no sentence to draw and the rest of this ' +
        'case would look at an empty bar.',
    ).toBe(1);

    const markup = readable(
      renderToStaticMarkup(createElement(RunBar, { notes, onRun: noop, onFocusNote: noop })),
    );

    expect(
      markup,
      'the sentence is in the state and not on the screen. A value proves the mechanism is ' +
        'there; the sentence a person reads proves the product works, and the whole class of ' +
        'defect this repo exists for lives between the two.',
    ).toContain(sentenceFor(NO_PATTERN, TYPED.name));
    expect(
      markup,
      'the dot beside it is not the colour of something that stops the work. A problem and a ' +
        'warning look the same then, and only one of them keeps Run from working.',
    ).toContain('text-fail');
    expect(
      markup,
      'Run is still live next to a file that cannot even be written. The one control that ' +
        'starts everything has to go dim while this stands.',
    ).toContain('disabled');

    const opened: string[] = [];
    const first = notes.at(0);
    if (first !== undefined) {
      focusNote(first, { fitView: noop, openPanel: (id) => opened.push(id) });
    }
    expect(
      opened,
      'clicking that sentence does not open the tile it is about. Then the dot is decoration: ' +
        'the person is told something is wrong and left to find it by hand.',
    ).toEqual([TYPED.id]);
  });

  it('refuses the save while the pattern is missing, and writes the file once it is there', async () => {
    vi.useFakeTimers();
    try {
      const store = openWith(TYPED);

      /* Człowiek pracuje dalej — autosave rusza po każdej zmianie, także po zmianie nazwy. */
      store.getState().rename('Ship a feature, checked');
      await vi.advanceTimersByTimeAsync(5_000);

      expect(
        spy.tried.length,
        'autosave never even tried, so nothing below says anything about what happens when it ' +
          'is refused.',
      ).toBe(1);
      expect(
        spy.written,
        'the file landed on disk without a pattern. Then the tile would run and call a command ' +
          'that did nothing at all a success — which is the one thing this field exists to stop.',
      ).toEqual([]);
      expect(
        store.getState().couldNotSave,
        'the save was refused and the screen was told nothing. That is the worst of the three ' +
          'possible answers: the canvas shows work the file does not have, and says everything ' +
          'is fine.',
      ).toBe(sentenceFor(NO_PATTERN, TYPED.name));

      /* I w drugą stronę: wpisany wzorzec zdejmuje odmowę. Bez tego przypadku reguła, która
       * odmawia zawsze, przeszłaby wszystko powyżej. */
      store.getState().commit({ ...store.getState().document, steps: [FILLED] });
      await vi.advanceTimersByTimeAsync(5_000);

      expect(
        proofIn(spy.written.at(0)?.file, TYPED.id),
        'with the pattern typed in, the file still did not land carrying it. Either the save is ' +
          'refused for something else, or the field the person filled in never travelled.',
      ).toBe(PROOF);
      expect(
        store.getState().couldNotSave,
        'the sentence about the refused save stayed on the screen after the save that worked. A ' +
          'refusal that outlives its cause sends the person looking for a fault that is gone.',
      ).toBeNull();
      expect(
        store.getState().notes,
        'and there is nothing left to fix on this tile once it says what it runs and how to ' +
          'tell that it ran.',
      ).toEqual([]);
    } finally {
      vi.useRealTimers();
    }
  });
});
