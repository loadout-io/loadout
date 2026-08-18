/* AC-7 dla T-38: każdy kafelek postawiony na płótnie ma za sobą panel — bez wyjątku.
 *
 * DWIE WADY, KTÓRE TO KRYTERIUM SĄDZI, obie zmierzone 2026-08-17 na wyladowanym trunku.
 *   1. `freshStep` daje `agent: ''` (świadomie: „jeszcze nie wybrano"), a `editor.tsx` montował
 *      panel dopiero po rozwiązaniu tego id w bibliotece agentów. `''` nie rozwiąże się nigdy,
 *      więc krok prosto z `＋ Add step` był TRWALE NIESKONFIGUROWALNY: ekran odpowiadał na niego
 *      zdaniem o niezaznaczonym kafelku.
 *   2. `step-panel/checkpoint-panel.tsx` miał w całym repo ZERO importerów. Punkt kontrolny
 *      dodany na płótno nie miał gdzie dostać ani nazwy, ani pytania — bieg stawał na nim
 *      i nie pytał o nic.
 *
 * SŁABA WERSJA, NAZWANA WPROST: zaimportować `CheckpointPanel` i wyrenderować go tutaj. To
 * przechodzi DZIŚ, bez zmiany choćby jednej linii produkcji, bo tamten plik istnieje i działa
 * poprawnie — brakuje mu wyłącznie miejsca montowania. Odróżnia nas jedno: renderujemy
 * `WorkflowEditor`, czyli CAŁY ekran z jego prawdziwymi propsami, i pytamy o markup, który
 * z niego wyszedł. Test, który renderuje komponent wprost, nie odróżnia „zamontowane" od
 * „istnieje" (nagłówek `editor.tsx`).
 *
 * SKĄD BIERZE SIĘ LISTA KAFELKÓW, i dlaczego nie z markupu. Zmierzone: `renderToStaticMarkup`
 * nie uruchamia efektów, a React Flow buduje kafelki dopiero po zmierzeniu kontenera — jego
 * `react-flow__nodes` wychodzi z serwera PUSTY. Wyrocznią jest więc `toCanvas` z `canvas/map.ts`:
 * jedyna funkcja produkcyjna, która odpowiada, ile kafelków płótno narysuje i o jakich
 * identyfikatorach. Lista nie jest tu przepisana z palca ani przycięta do dwóch wybranych —
 * pętla idzie po wszystkim, co ta funkcja odda, a osobna asercja pilnuje, że oddała cokolwiek
 * (porównanie dwóch pustych list przechodzi na niczym).
 *
 * SKĄD BIORĄ SIĘ KROKI. Z `freshStep` — tej samej funkcji, którą wołają oba przyciski płótna
 * (`canvas.tsx`, `add`) i upuszczenie strzałki. Napisanie kroku ręcznie w teście dałoby krok
 * poprawnie skonfigurowany, czyli dokładnie ten przypadek, który i tak działał.
 *
 * DLACZEGO EDYTOR DOSTAJE `openStep`. Zaznaczenie żyje w stanie Reacta, a w tym repo nie ma
 * jsdom: nie ma kliknięcia i nie biegną efekty, więc bez tego wejścia jedyną sprawdzalną
 * odpowiedzią na pytanie „czy zaznaczenie daje panel" byłoby wyrenderowanie panelu wprost,
 * czyli słaba wersja opisana wyżej. To ten sam wzorzec, którym powłoka bierze `screens`,
 * a sekcja `store`.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI. „Panel jest w markupie" przechodzi także wtedy, gdy ekran
 * rysuje panel ZAWSZE — a wtedy niezmiennik 16 leży, bo panel bez zaznaczonego kroku edytuje
 * cudzy krok albo nic. Dlatego ostatni `it` pyta o ekran bez zaznaczenia i wymaga, żeby panelu
 * tam NIE było.
 *
 * ATRAPY, DWIE, OBIE NA GRANICY: `./io` (w vitest nie ma okna Tauri, a autosave naprawdę
 * zapisuje) oraz `checkpoint-panel`, który jest atrapą PRZEPUSZCZAJĄCĄ — woła prawdziwy
 * komponent, oddaje jego prawdziwe drzewo do ekranu i tylko zapisuje je po drodze. Bez tego
 * drugiego nie da się dosięgnąć uchwytu `onChange`, który wisi w drzewie: `renderToStaticMarkup`
 * oddaje napis, a napis nie ma handlerów. Ten zapis jest jedynym sposobem, żeby „wpisane pytanie
 * wraca w pliku" znaczyło ścieżkę produkcyjną, a nie wywołanie, które test sam sobie zrobił.
 *
 * JEDNO ZDANIE WPISANE RĘCZNIE — `PLACEHOLDER`. Tak samo jak `No workflows yet.`
 * w `mounted.test.tsx`: to jest KONTRAKT tego kryterium, a nie wartość policzona gdzie indziej.
 * Zaimportowane z `editor.tsx` zgadzałoby się z ekranem zawsze, także wtedy, gdyby ekran
 * pokazywał je przy każdym kafelku.
 */
import { isValidElement } from 'react';
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../state/agents';
import type { AgentStep, Point, WorkflowFile } from '../../state/workflows';
import { freshId, freshStep } from './canvas/connect';
import { toCanvas, toFile } from './canvas/map';
import { WorkflowEditor } from './editor';
import type { CheckpointPanelProps } from './step-panel/checkpoint-panel';

const spy = vi.hoisted(() => ({
  /** Drzewa oddane przez panel punktu kontrolnego — po jednym na jego zamontowanie. */
  shown: [] as ReactElement[],
  /** Co autosave posłał na dysk. Para, bo ścieżka jest połową odpowiedzi. */
  written: [] as { path: string; file: WorkflowFile }[],
}));

/* Granica Tauriego. Autosave jest częścią ścieżki, którą to kryterium sądzi, więc nie da się
 * go wyłączyć — a `invoke` bez okna odrzuca obietnicę, której magazyn świadomie nie łyka. */
vi.mock('./io', () => ({
  write: (path: string, file: WorkflowFile) => {
    spy.written.push({ path, file });
    return Promise.resolve();
  },
  check: () => Promise.resolve([]),
}));

/* Atrapa PRZEPUSZCZAJĄCA: prawdziwy komponent, prawdziwe drzewo, zapisane po drodze.
 * Gdyby ekran przestał ten plik montować, `spy.shown` zostaje puste i to jest ta czerwień,
 * dla której to kryterium powstało. */
vi.mock('./step-panel/checkpoint-panel', async (importOriginal) => {
  const real = await importOriginal<typeof import('./step-panel/checkpoint-panel')>();
  return {
    CheckpointPanel: (props: CheckpointPanelProps): ReactElement => {
      const tree = real.CheckpointPanel(props);
      spy.shown.push(tree);
      return tree;
    },
  };
});

const PATH = 'ship-a-feature.json';

/** Zdanie, którym ekran odpowiada, kiedy NIC nie jest zaznaczone. Kontrakt tego kryterium. */
const PLACEHOLDER = 'Pick a step to see what it was given.';

const QUESTION = 'Does this plan look right before anyone writes code?';

const AGENT: Agent = {
  schema: 1,
  id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
  name: 'Forge',
  summary: 'Writes code',
  color: 'clay',
  instructions: 'Write the smallest change that makes the checks pass.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  skills: [],
  connections: [],
  writeResultsTo: 'notes.md',
};

const noop = () => undefined;

/** Krok agenta prosto z „＋ Add step" — tą samą funkcją, którą woła przycisk płótna. */
function addedStep(file: WorkflowFile, at: Point): AgentStep {
  const step = freshStep('agent', freshId(file), at);
  if (step.kind !== 'agent') throw new Error('freshStep no longer makes an agent step');
  return step;
}

const START: WorkflowFile = {
  format: 1,
  id: 'wf_ship_a_feature',
  name: 'Ship a feature',
  steps: [],
  links: [],
};

/** Trzy kafelki, trzy różne stany, wszystkie postawione tak, jak stawia je płótno:
 * krok bez wybranego agenta, punkt kontrolny i krok z agentem z biblioteki. */
const FRESH = addedStep(START, { x: 24, y: 24 });
const STOP = freshStep('checkpoint', freshId({ ...START, steps: [FRESH] }), { x: 24, y: 168 });
const CHOSEN: AgentStep = {
  ...addedStep({ ...START, steps: [FRESH, STOP] }, { x: 24, y: 312 }),
  agent: AGENT.id,
};

const DOC: WorkflowFile = {
  ...START,
  steps: [FRESH, STOP, CHOSEN],
  links: [
    { from: FRESH.id, to: STOP.id },
    { from: STOP.id, to: CHOSEN.id },
  ],
};

function editorWith(openStep: string): string {
  return renderToStaticMarkup(
    <WorkflowEditor
      path={PATH}
      document={DOC}
      agents={[AGENT]}
      onClose={noop}
      onRun={noop}
      openStep={openStep}
    />,
  );
}

function editorWithNothingPicked(): string {
  return renderToStaticMarkup(
    <WorkflowEditor path={PATH} document={DOC} agents={[AGENT]} onClose={noop} onRun={noop} />,
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

/** Pytanie zapisane przy punkcie kontrolnym o danym id — albo `undefined`. */
function questionIn(file: WorkflowFile, id: string): string | undefined {
  const step = file.steps.find((one) => one.id === id);
  if (step === undefined || step.kind !== 'checkpoint') return undefined;
  return step.question;
}

beforeEach(() => {
  spy.shown.length = 0;
  spy.written.length = 0;
});

describe('every tile on the canvas opens its own panel', () => {
  it('a step straight from the add button can be set up the moment it is picked', () => {
    const markup = editorWith(FRESH.id);

    expect(
      markup,
      'a step added by the button answers with the sentence for "nothing is picked". That is ' +
        'the whole defect: the freshly added step names no agent yet, the screen would only ' +
        'open a panel for a step whose agent it could resolve, and so the one tile the person ' +
        'just made was the one tile they could never set up.',
    ).not.toContain(PLACEHOLDER);
    expect(
      markup,
      'no panel of any kind came out of the screen for a step that has just been added.',
    ).toContain('data-step-panel');
    expect(
      markup,
      'the panel for a step with nobody assigned has to offer the library, otherwise it is a ' +
        'panel that shows the hole without letting anyone fill it. The saved agent handed to ' +
        'the screen is not in the markup, so nothing on that panel can choose one.',
    ).toContain(AGENT.name);
    expect(
      markup,
      'the name the canvas gave the new step has to reach its panel — a panel showing somebody ' +
        "else's step is worse than no panel.",
    ).toContain(FRESH.name);
  });

  it('a checkpoint straight from the add button gets its own panel, with the field for what it asks', () => {
    const markup = editorWith(STOP.id);

    expect(
      spy.shown.length,
      'the screen never mounted the checkpoint panel. Until 2026-08-18 that file had zero ' +
        'importers in the whole repo, so a checkpoint dropped on the canvas had nowhere to get ' +
        'a name or a question from: the run would stand on it and ask nothing.',
    ).toBe(1);
    expect(
      markup,
      'the panel carries no field for the question, which is the only reason a run stops there ' +
        'at all.',
    ).toContain('id="checkpoint-question"');
    expect(markup, 'and no field for its name either').toContain('id="checkpoint-name"');
    expect(markup, 'the name the canvas gave this tile has to reach its panel').toContain(
      STOP.name,
    );
    expect(
      markup,
      'a checkpoint has no agent, so it inherits nothing and must not be shown the seven-row ' +
        'panel for agent steps: half of those rows would be answering a question nobody asked.',
    ).not.toContain('id="step-give-up-after"');
  });

  it('what somebody types into that field comes back in the file the canvas hands over', async () => {
    vi.useFakeTimers();
    try {
      editorWith(STOP.id);
      const tree = spy.shown.at(0);
      expect(
        tree,
        'the screen mounted no checkpoint panel, so there is no field to type into.',
      ).toBeDefined();

      const typeInto = onChangeOf(tree, 'checkpoint-question');
      expect(
        typeInto,
        'nothing in the rendered panel answers to the id of the question field, so this test ' +
          'would go on to assert nothing at all. Either the field is gone or it was renamed.',
      ).not.toBeNull();

      typeInto?.({ target: { value: QUESTION } });
      /* Autosave jest odliczaniem, nie zapisem na każdą literę: przewijamy zegar dobrze poza
       * jego ciszę, zamiast przepisywać tu jej długość. */
      await vi.advanceTimersByTimeAsync(5_000);

      expect(
        spy.written.map((one) => one.path),
        'the typed question never reached disk. The field is controlled, so text that does not ' +
          'come back out through the screen lives nowhere at all.',
      ).toEqual([PATH]);
      expect(
        spy.written.map((one) => questionIn(one.file, STOP.id)),
        'the file that went to disk carries no question on that checkpoint. A field that ' +
          'accepts text and drops it looks exactly like one that works.',
      ).toEqual([QUESTION]);
      expect(
        spy.written.map((one) => {
          const view = toCanvas(one.file);
          return questionIn(toFile(one.file, view.nodes, view.edges), STOP.id);
        }),
        'the question does not survive the trip through the canvas mapper, so the next save ' +
          'after any tile is dragged would quietly drop it.',
      ).toEqual([QUESTION]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('has no tile at all whose panel is missing, and shows none until one is picked', () => {
    const tiles = toCanvas(DOC).nodes;

    expect(
      tiles.length,
      'the canvas mapper drew no tiles for this document, so the loop below would pass without ' +
        'looking at anything. That is the empty comparison this criterion is built to avoid.',
    ).toBe(DOC.steps.length);
    expect(
      tiles.length,
      'one tile is not a walk over every tile: the defect this criterion is about lived in the ' +
        'kinds of tile nobody checked.',
    ).toBeGreaterThanOrEqual(3);

    const without = tiles
      .map((tile) => ({ id: tile.id, markup: editorWith(tile.id) }))
      .filter((one) => !one.markup.includes('data-step-panel'))
      .map((one) => one.id);

    expect(
      without,
      'these tiles can be picked on the canvas and answer with no panel. A tile that cannot be ' +
        'set up is a tile that cannot run, and the person who put it there has no way to find ' +
        'that out except by starting the run.',
    ).toEqual([]);

    expect(
      editorWithNothingPicked(),
      'with nothing picked the screen still shows a panel. That panel is either editing ' +
        'somebody else, or nothing — and either way it makes the assertions above pass on a ' +
        'screen that draws a panel no matter what.',
    ).not.toContain('data-step-panel');
    expect(
      editorWithNothingPicked(),
      'with nothing picked the screen has to say so, in the one sentence it keeps for it.',
    ).toContain(PLACEHOLDER);
  });
});
