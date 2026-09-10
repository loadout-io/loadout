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

vi.mock('./io', () => {
  const disk = {
    write: (path: string, file: WorkflowFile) => {
      spy.written.push({ path, file });
      return Promise.resolve();
    },
    check: () => Promise.resolve([]),
  };
  return { ...disk, forProject: () => disk };
});

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
const PLACEHOLDER = 'Pick a step to set up what it does.';

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

/** Ten sam kafelek z przycisku, dopełniony o pola, które ma tylko „uruchom i zostaw".
 *
 * Zawężenie, nie rzutowanie: `freshStep` oddaje `Step`, a `Step` to unia. Rzutowanie milczałoby
 * w dniu, w którym przycisk zacznie stawiać coś innego — a wtedy cały ten plik sądziłby panel,
 * którego ekran już nie montuje. */
function serveTile(extra: Partial<ServeStep>): WorkflowFile {
  if (SERVE.kind !== 'serve') {
    throw new Error('the canvas button no longer makes a tile that starts something');
  }
  return { ...DOC, steps: [{ ...SERVE, ...extra }] };
}

function editorWith(openStep?: string, document: WorkflowFile = DOC): string {
  return renderToStaticMarkup(
    <WorkflowEditor
      path={PATH}
      document={document}
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
  it('explicitly saves waiting for an allowed agent to start the configured app', async () => {
    vi.useFakeTimers();
    try {
      const markup = editorWith(SERVE.id);
      expect(markup).toContain('When to start');
      const choose = onChangeOf(spy.shown.at(0), 'serve-start-when');
      expect(choose, 'the real editor has no agent-start choice').not.toBeNull();
      choose?.({ target: { value: 'asked' } });
      await vi.advanceTimersByTimeAsync(5_000);
      expect(spy.written.at(-1)?.file.steps.find((step) => step.id === SERVE.id)).toMatchObject({
        startWhen: 'asked',
      });
    } finally {
      vi.useRealTimers();
    }
  });
  it('saves an app description source without overwriting the inactive manual command', async () => {
    vi.useFakeTimers();
    try {
      const serve = { ...SERVE, command: COMMAND, commandFrom: { field: 'launch' } };
      editorWith(SERVE.id, { ...DOC, steps: [serve] });
      const choose = onChangeOf(spy.shown.at(0), 'serve-command-format');
      expect(choose, 'the editor cannot select the typed app description').not.toBeNull();
      choose?.({ target: { value: 'launch-description' } });
      await vi.advanceTimersByTimeAsync(5_000);
      expect(spy.written.at(-1)?.file.steps.find((step) => step.id === SERVE.id)).toMatchObject({
        command: COMMAND,
        commandFrom: { field: 'launch', format: 'launch-description' },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('offers and saves the exact second copy by its readable step name', async () => {
    vi.useFakeTimers();
    try {
      const prepare = {
        ...freshStep('agent', 's_prepare', { x: 0, y: 0 }),
        name: 'Prepare app',
        copies: 2,
      };
      const serve = {
        ...SERVE,
        commandFrom: { field: 'launch', format: 'launch-description' as const },
      };
      const markup = editorWith(SERVE.id, {
        ...DOC,
        steps: [prepare, serve],
        links: [{ from: prepare.id, to: SERVE.id }],
      });
      expect(markup).toContain('Prepare app · copy 2');
      const choose = onChangeOf(spy.shown.at(0), 'serve-command-producer');
      expect(choose, 'the real editor has no exact result selector').not.toBeNull();
      choose?.({ target: { value: 's_prepare~2' } });
      await vi.advanceTimersByTimeAsync(5_000);
      expect(spy.written.at(-1)?.file.steps.find((step) => step.id === SERVE.id)).toMatchObject({
        commandFrom: { field: 'launch', format: 'launch-description', producer: 's_prepare~2' },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('can explicitly wait for HTTP readiness and saves that choice through the actual editor', async () => {
    vi.useFakeTimers();
    try {
      const markup = editorWith(SERVE.id);
      const choose = onChangeOf(spy.shown.at(0), 'serve-readiness');
      expect(markup).toContain('Wait until the app responds');
      expect(choose, 'the real panel has no readiness handler').not.toBeNull();
      choose?.({ target: { value: 'http' } });
      await vi.advanceTimersByTimeAsync(5_000);
      const written = spy.written.at(-1)?.file.steps.find((step) => step.id === SERVE.id);
      expect(written).toMatchObject({
        readiness: {
          kind: 'http',
          endpoint: 'web',
          path: '/',
          timeoutSeconds: 30,
          expectedStatus: 200,
        },
        endpoints: [{ name: 'web', host: '127.0.0.1', port: 3000, portEnv: 'PORT' }],
      });
    } finally {
      vi.useRealTimers();
    }
  });

  /* 2026-08-31 — TO KRYTERIUM ZMIENIŁO ZDANIE, na polecenie właściciela, i tą samą drogą, co
   * bliźniacze kryterium kafelka „sprawdź" (`canvas/check-tile-can-be-placed.test.ts`). Żądało
   * kafelka z pustą komendą i folderem `same-copy`; oba razem znaczyły plik, którego
   * `workflow::file::save` odmawia przed dotknięciem dysku, więc kliknięcie w przycisk kończyło
   * się czerwonym paskiem 400 ms później i cichą utratą reszty pracy na płótnie. Powód w całości
   * stoi przy `freshStep` w `canvas/connect.ts`. */
  it('comes out of the add button ready to be saved, in a folder it can be in', () => {
    const step: ServeStep =
      SERVE.kind === 'serve'
        ? SERVE
        : (() => {
            throw new Error('freshStep no longer makes a serve step');
          })();

    expect(
      step.command,
      'the tile arrives with no command, so the file it lands in cannot be saved at all — and ' +
        'the value it does arrive with has to be the one the panel already suggests in grey, ' +
        'not a second opinion invented for the file',
    ).toBe('npm run dev');
    expect(
      step.folder,
      'the button puts this tile down LOOSE, so nothing comes before it and "the same copy as ' +
        'the step before it" names a folder there is no way to work out. Rust refuses the save ' +
        'for exactly that (check::nothing_before_it). The moment somebody draws an arrow into ' +
        'this tile, that copy is one click away in the panel.',
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
      'this tile has no agent, so it inherits nothing and must not be shown the rows an agent ' +
        'step gets: half of them would answer a question nobody asked.',
    ).not.toContain('id="step-give-up-after"');
    expect(
      markup,
      'and no field for a proof: this tile judges nothing, so there is no output for a proof to ' +
        'match. Asking for one would be a field nobody can fill.',
    ).not.toContain('id="step-proof"');
    expect(
      markup,
      'and it has to say WHERE it runs. For a server that is not a detail: the wrong answer ' +
        'serves code without the work the step before it just wrote, and looks fine doing it.',
    ).toContain('name="serve-where"');
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
  /* CO TEN KAFELEK URUCHAMIA — pole, którego brak przez cały sierpień znaczył „serwer".
   *
   * Aplikacja z własnym oknem ma otwarty port, zanim narysuje pierwszy piksel, więc gotowość
   * mierzona portem melduje ją gotową do scenariusza, którego nie ma jak wykonać. Wybór musi
   * dojechać do PLIKU, bo to plik czyta bieg — pole zapisane wyłącznie w drzewie wygląda na
   * ekranie identycznie i zawodzi dopiero w biegu. */
  it('what this tile starts travels to the file, so a window is not judged ready by its port', async () => {
    vi.useFakeTimers();
    try {
      editorWith(SERVE.id);
      const choose = onChangeOf(spy.shown.at(0), 'serve-target-kind');
      expect(
        choose,
        'the panel never asks what this tile starts, so every tile is filled in as a server ' +
          'and an app with its own window is called ready the moment it holds a port.',
      ).not.toBeNull();
      choose?.({ target: { value: 'native' } });
      await vi.advanceTimersByTimeAsync(5_000);
      expect(spy.written.at(-1)?.file.steps.find((one) => one.id === SERVE.id)).toMatchObject({
        targetKind: 'native',
      });
    } finally {
      vi.useRealTimers();
    }
  });

  /* ODMOWA CZYTA SIĘ PRZY WYPEŁNIANIU, NIE W CZWARTEJ MINUCIE BIEGU (niezmiennik 29).
   *
   * Instancja testowa bez własnego katalogu danych pisze do prawdziwego — czyli do nagrań
   * człowieka — więc Loadout jej nie uruchamia. Kafelek, który o tym milczy, każe się tego
   * dowiedzieć z biegu, który zapłacił już za kroki przed tym. */
  it('an app with its own window says it will not start before its data is moved', () => {
    const shown = editorWith(SERVE.id, serveTile({ targetKind: 'native' }));
    expect(
      shown,
      'the panel takes no name for the setting that moves this app’s data, so the only place ' +
        'anybody learns it is missing is the run that refused to start it.',
    ).toContain('serve-test-data-env');
    expect(
      shown,
      'nothing on the panel says an app with its own window and no test data folder will not ' +
        'be started at all.',
    ).toContain('worse than no test at all');
  });

  it('once the setting is named, the panel says what it will do with it', async () => {
    vi.useFakeTimers();
    try {
      const shown = editorWith(
        SERVE.id,
        serveTile({ targetKind: 'native', testDataEnv: 'MURMUR_DATA_DIR' }),
      );
      expect(shown).toContain('MURMUR_DATA_DIR');
      expect(
        shown,
        'the panel repeats the refusal even though the setting is filled in, so the sentence ' +
          'says nothing about this tile.',
      ).not.toContain('worse than no test at all');

      const typeInto = onChangeOf(spy.shown.at(0), 'serve-test-data-env');
      expect(typeInto, 'the named setting cannot be changed once it is there.').not.toBeNull();
      typeInto?.({ target: { value: 'MURMUR_HOME' } });
      await vi.advanceTimersByTimeAsync(5_000);
      expect(spy.written.at(-1)?.file.steps.find((one) => one.id === SERVE.id)).toMatchObject({
        testDataEnv: 'MURMUR_HOME',
      });
    } finally {
      vi.useRealTimers();
    }
  });
});
