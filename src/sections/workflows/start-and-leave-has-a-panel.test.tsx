/* Kafelek „uruchom i zostaw" da się WYPEŁNIĆ — inaczej przycisk, który go stawia, prowadzi donikąd.
 *
 * 2026-08-23, prośba właściciela wprost. Kryterium jest napisane w kształcie
 * `every-tile-opens-a-panel.test.tsx` i z tego samego powodu: tamta wada zdarzyła się już DWA
 * RAZY w tym repo (krok bez wybranego agenta, punkt kontrolny bez importera) i oba razy wyglądała
 * identycznie — kafelek stoi na płótnie, a ekran odpowiada na niego zdaniem o niezaznaczonym
 * kroku. Trzeci rodzaj kafelka dostaje więc kryterium od pierwszego dnia, a nie po zgłoszeniu.
 *
 * SŁABĄ WERSJĄ jest wyrenderowanie `ServePanel` wprost. To przechodzi bez ani jednej linii
 * produkcji, bo ten plik istnieje i działa — brakowałoby mu wyłącznie miejsca montowania.
 * Renderujemy więc CAŁY `WorkflowEditor` z jego prawdziwymi propsami i pytamy o markup, który
 * z niego wyszedł (nagłówek `editor.tsx`).
 *
 * DRUGĄ SŁABĄ WERSJĄ jest sam markup pola. Pole, które istnieje i nie dojeżdża do pliku, wygląda
 * na ekranie dokładnie tak samo jak działające — a ten kafelek URUCHAMIA to, co w nim stoi, więc
 * pusta komenda jest odmową w środku biegu. Dlatego drugie `it` przechodzi całą drogą: uchwyt
 * `onChange` z drzewa → magazyn → autosave → plik.
 *
 * ATRAPY, DWIE, OBIE NA GRANICY — tak samo jak w kryterium obok: `./io` (w vitest nie ma okna
 * Tauri, a autosave naprawdę zapisuje) oraz `step-panel/serve-panel`, atrapa PRZEPUSZCZAJĄCA,
 * która woła prawdziwy komponent i tylko zapisuje po drodze jego drzewo. Bez niej nie da się
 * dosięgnąć handlera: `renderToStaticMarkup` oddaje napis, a napis nie ma uchwytów.
 */
import { isValidElement } from 'react';
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ServeStep, WorkflowFile } from '../../state/workflows';
import { freshId, freshStep } from './canvas/connect';
import { WorkflowEditor } from './editor';
import type { ServePanelProps } from './step-panel/serve-panel';

const spy = vi.hoisted(() => ({
  /** Drzewa oddane przez panel — po jednym na jego zamontowanie. */
  shown: [] as ReactElement[],
  /** Co autosave posłał na dysk. Para, bo ścieżka jest połową odpowiedzi. */
  written: [] as { path: string; file: WorkflowFile }[],
}));

vi.mock('./io', () => ({
  write: (path: string, file: WorkflowFile) => {
    spy.written.push({ path, file });
    return Promise.resolve();
  },
  check: () => Promise.resolve([]),
}));

vi.mock('./step-panel/serve-panel', async (importOriginal) => {
  const real = await importOriginal<typeof import('./step-panel/serve-panel')>();
  return {
    ServePanel: (props: ServePanelProps): ReactElement => {
      const tree = real.ServePanel(props);
      spy.shown.push(tree);
      return tree;
    },
  };
});

const PATH = 'ship-a-feature.json';

/** Zdanie, którym ekran odpowiada, kiedy NIC nie jest zaznaczone. Kontrakt tego kryterium —
 * wpisany ręcznie, nie zaimportowany z `editor.tsx`: zaimportowany zgadzałby się z ekranem
 * zawsze, także wtedy, gdyby ekran pokazywał je przy każdym kafelku. */
const PLACEHOLDER = 'Pick a step to see what it was given.';

const COMMAND = 'npm run dev --workspace apps/web';

const noop = () => undefined;

const START: WorkflowFile = {
  format: 1,
  id: 'wf_ship_a_feature',
  name: 'Ship a feature',
  steps: [],
  links: [],
};

/** Kafelek prosto z przycisku `＋ Start something` — TĄ SAMĄ funkcją, którą woła płótno.
 *
 * Napisanie go tu ręcznie dałoby kafelek poprawnie wypełniony, czyli dokładnie ten przypadek,
 * który i tak działa. Ten wychodzi z pustą komendą, bo taki wychodzi z przycisku. */
const SERVE = freshStep('serve', freshId(START), { x: 24, y: 24 });

const DOC: WorkflowFile = { ...START, steps: [SERVE] };

function editorWith(openStep?: string): string {
  return renderToStaticMarkup(
    <WorkflowEditor
      path={PATH}
      document={DOC}
      agents={[]}
      onClose={noop}
      onRun={noop}
      onCreateAgent={noop}
      {...(openStep === undefined ? {} : { openStep })}
    />,
  );
}

/** Uchwyt `onChange` elementu o danym `id`, wyjęty z drzewa Reacta.
 *
 * Drzewo, nie markup: `renderToStaticMarkup` oddaje napis, a napis nie niesie handlerów.
 * Oddaje `null`, kiedy takiego pola w drzewie nie ma — i wołający MA to sprawdzić, bo pole
 * nieznalezione i pole, które nic nie robi, wyglądają w teście identycznie. */
function onChangeOf(
  node: unknown,
  id: string,
): ((event: { target: { value: string } }) => void) | null {
  if (Array.isArray(node)) {
    for (const one of node) {
      const hit = onChangeOf(one, id);
      if (hit !== null) return hit;
    }
    return null;
  }
  if (typeof node !== 'object' || node === null) return null;
  if (!isValidElement<Record<string, unknown>>(node)) return null;

  const handler = node.props['onChange'];
  if (node.props['id'] === id && typeof handler === 'function') {
    return (event) => {
      handler(event);
    };
  }
  return onChangeOf(node.props['children'], id);
}

/** Komenda zapisana przy kafelku o danym id — albo `undefined`. */
function commandIn(file: WorkflowFile, id: string): string | undefined {
  const step = file.steps.find((one) => one.id === id);
  if (step === undefined || step.kind !== 'serve') return undefined;
  return step.command;
}

beforeEach(() => {
  spy.shown.length = 0;
  spy.written.length = 0;
});

describe('a tile that starts something and walks on can be filled in', () => {
  it('comes out of the add button empty, in the project folder', () => {
    const step: ServeStep =
      SERVE.kind === 'serve'
        ? SERVE
        : (() => {
            throw new Error('freshStep no longer makes a serve step');
          })();

    expect(
      step.command,
      'a made-up example like "npm run dev" looks on the canvas exactly like a decision the ' +
        'person made — and this tile RUNS what stands in it',
    ).toBe('');
    expect(
      step.folder,
      'the project folder, not a fresh copy: a server started in a copy of the tree serves code ' +
        'nobody is looking at',
    ).toEqual({ use: 'project' });
  });

  it('gets its own panel the moment it is picked, with the field for what to run', () => {
    const markup = editorWith(SERVE.id);

    expect(
      markup,
      'the screen answered a picked tile with the sentence for "nothing is picked". That is the ' +
        'defect this repo has already shipped twice: a tile you can put down and never set up.',
    ).not.toContain(PLACEHOLDER);
    expect(
      spy.shown.length,
      'the screen never mounted the panel for this tile. A file with zero importers is exactly ' +
        'how the checkpoint panel sat in this repo until 2026-08-18.',
    ).toBe(1);
    expect(
      markup,
      'the panel carries no field for the command, which is the only thing this tile does.',
    ).toContain('id="serve-command"');
    expect(markup, 'and no field for its name either').toContain('id="serve-name"');
    expect(
      markup,
      'this tile has no agent, so it inherits nothing and must not be shown the seven-row panel ' +
        'for agent steps: half of those rows would answer a question nobody asked.',
    ).not.toContain('id="step-give-up-after"');
    expect(
      markup,
      'and no field for a proof: this tile judges nothing, so there is no output for a proof to ' +
        'match. Asking for one would be a field nobody can fill.',
    ).not.toContain('id="step-proof"');
  });

  it('what somebody types into that field comes back in the file the canvas hands over', async () => {
    vi.useFakeTimers();
    try {
      editorWith(SERVE.id);
      const tree = spy.shown.at(0);
      expect(tree, 'the screen mounted no panel, so there is no field to type into.').toBeDefined();

      const typeInto = onChangeOf(tree, 'serve-command');
      expect(
        typeInto,
        'nothing in the rendered panel answers to the id of the command field, so this test ' +
          'would go on to assert nothing at all. Either the field is gone or it was renamed.',
      ).not.toBeNull();

      typeInto?.({ target: { value: COMMAND } });
      /* Autosave jest odliczaniem, nie zapisem na każdą literę: przewijamy zegar dobrze poza
       * jego ciszę, zamiast przepisywać tu jej długość. */
      await vi.advanceTimersByTimeAsync(5_000);

      const last = spy.written.at(-1);
      expect(
        last?.path,
        'the typed command never reached disk. The field is controlled, so text that does not ' +
          'travel this whole road lives in the DOM alone — the panel looks filled in and the ' +
          'run refuses on an empty command.',
      ).toBe(PATH);
      expect(
        last === undefined ? undefined : commandIn(last.file, SERVE.id),
        'something was written, but not this tile’s command.',
      ).toBe(COMMAND);
    } finally {
      vi.useRealTimers();
    }
  });
});
