/* EKRAN PRACY POKAZUJE PRACĘ, ZANIM COKOLWIEK RUSZY — trzy fakty, każdy w markupie, który
 * widzi człowiek (niezmiennik 29).
 *
 * CO BYŁO, ZMIERZONE 2026-08-31 na zrzucie prawdziwego okna 1512×950 (`e2e/harness.ts`,
 * chromium, atrapa granicy). Człowiek z gotowym folderem, agentem i workflow dostawał:
 *   prawa kolumna   pusty prostokąt 268 px. Graf biegu — sygnatura tego produktu — rysował się
 *                   WYŁĄCZNIE w trakcie biegu, bo jedynym jego źródłem był `run.steps`.
 *   kolumna pracy   1010 × 760 px czerni ze znakiem `◇` i zdaniem „Nothing here yet: the work
 *                   shows up line by line." — czyli komunikatem o braku danych, którego
 *                   DESIGN §6 zabrania wprost.
 *   pasek           nazwa sekcji skrócona do „R..", a prawy koniec rzędu kontrolek poza kadrem.
 *
 * SŁABA WERSJA TYCH KRYTERIÓW: zapytać `planFor`/`firstRunnable` o zwróconą wartość. Przechodzi
 * ją stan, w którym funkcja składa plan bez zarzutu, a ekran dalej rysuje pustą kolumnę — czyli
 * klasa „kryterium zielone, funkcja martwa", dla której to repo powstało. Montowany jest więc
 * CAŁY `<Run />`, a fakty z dysku wchodzą tą samą drogą, którą wpisuje je produkcja
 * (`./whats-ready.ts`, `rememberWorkflows`/`rememberAgents`/`rememberRuns`).
 *
 * GRANICA TEGO PRZYRZĄDU ZNIKŁA 2026-08-31, RAZEM Z PŁÓTNEM. Do tego dnia stało tu ostrzeżenie:
 * `renderToStaticMarkup` nie jest przeglądarką, a React Flow mierzy kafelki dopiero w oknie,
 * więc plan Z UKŁADEM oddawał w markupie ramę płótna z PUSTYMI pojemnikami — i punkt o pliku ze
 * strzałkami mógł pytać wyłącznie o to, czy rama tam stoi. Płótno zeszło z ekranu biegu w całości
 * (`./graph/graph.tsx`, nagłówek): kroki rysują się jako jedna pionowa ścieżka, ZAWSZE. Oba
 * plany — ten bez strzałek i ten z nimi — dają więc dziś ten sam, w pełni czytelny markup, więc
 * punkt niżej pyta o kroki, ich kolejność i o zdanie „co po czym", a nie o obecność ramy.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';
import type { PastRunRow } from './io';

/* Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki. Efekty i tak nie biegną pod
 * `renderToStaticMarkup`, ale sam import `@tauri-apps/api/core` musi się rozwiązać, inaczej
 * plik przewraca się na ZBIERANIU i „nic nie znaleziono" czyta się jak zdana asercja. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: () => Promise.resolve(undefined),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('./index')).default;
const { useWorkspaces } = await import('../../state/workspaces');
const { useRun } = await import('../../state/run');
const { forgetWhatIsReady, rememberAgents, rememberRuns, rememberWorkflows } =
  await import('./whats-ready');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

function step(id: string, name: string, y: number): Choice['steps'][number] {
  return { id, name, state: 'pending', kind: 'agent', at: { x: 40, y } };
}

/** Plik workflow, który ktoś ułożył: cztery kroki, każdy ze swoim miejscem. */
const STEPS = [
  step('s1', 'Research the codebase', 40),
  step('s2', 'Architecture', 170),
  step('s3', 'Implement', 300),
  step('s4', 'Review', 430),
] as const;

const WITHOUT_ARROWS: Choice = {
  path: 'ship-a-feature.json',
  name: 'Ship a feature',
  steps: STEPS,
  links: [],
};

const WITH_ARROWS: Choice = {
  ...WITHOUT_ARROWS,
  links: [
    { from: 's1', to: 's2' },
    { from: 's2', to: 's3' },
    { from: 's3', to: 's4' },
  ],
};

const LAST: PastRunRow = {
  folder: '20260830-181200__0198a1f2-3b4c-7d5e-8f60-0000000000aa',
  when: '2026-08-30 18:12',
  title: 'Ship a feature',
  state: 'succeeded',
  steps: 4,
  costUsd: 3.41,
  said: null,
};

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

/**
 * Ekran, na którym setup jest gotowy i NIC nie biegnie.
 *
 * Trzy odpowiedzi z dysku, każda tą samą drogą, którą wpisuje je produkcja. Agent musi być,
 * bo dopóki go nie ma, ta kolumna należy do przewodnika pierwszego uruchomienia — i tak ma
 * należeć.
 */
function screenWith(what: readonly Choice[], runs: readonly PastRunRow[] = []): string {
  useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
  useRun.setState({ workflow: '', steps: [], links: null });
  rememberWorkflows(what);
  rememberAgents(1);
  rememberRuns(HERE.folder, runs);
  return readable(renderToStaticMarkup(<Run />));
}

/** Fragment markupu należący do kolumny planu — od jej znacznika do końca dokumentu. */
function planColumn(markup: string): string {
  const at = markup.indexOf('data-plan-column');
  return at < 0 ? '' : markup.slice(at);
}

/**
 * Fragment markupu należący do kolumny strumienia — od jej znacznika do końca dokumentu.
 *
 * DOPISANE 2026-08-31, PO ZMIERZONYM ZDERZENIU, i jest to ZAWĘŻENIE kryterium, nie jego
 * poluzowanie. Dwa punkty niżej pytały o napis `>Ship a feature<` w CAŁYM markupie ekranu,
 * a `WITHOUT_ARROWS.name` i `LAST.title` to w tej fikstrze ten sam napis. Od chwili, w której
 * ekran biegu dostał własny nagłówek (`./strip/head.tsx` — nazywa bieg, a przy postoju workflow,
 * który ruszy), ta sama nazwa stoi na ekranie DRUGI RAZ, w miejscu, które z kartą ostatniego
 * biegu nie ma nic wspólnego. Punkt „karta nie wchodzi do cudzego folderu" robił się wtedy
 * czerwony na nagłówku, a punkt „karta nazywa bieg" byłby zielony także wtedy, gdyby karty nie
 * było wcale. Oba pytają teraz o kolumnę, którą ten `describe` nazywa po imieniu.
 */
function streamColumn(markup: string): string {
  const at = markup.indexOf('data-stream-column');
  return at < 0 ? '' : markup.slice(at);
}

afterEach(() => {
  forgetWhatIsReady();
  useRun.setState({ workflow: '', steps: [], links: null });
});

describe('the picture of the work is on the screen before the first line of the stream', () => {
  it('draws a step for every step of the workflow that Run will start', () => {
    const markup = screenWith([WITHOUT_ARROWS]);
    const column = planColumn(markup);

    expect(
      column,
      'the run screen renders no plan column at all, so there is nothing to look for in it',
    ).not.toBe('');

    const drawn = [...column.matchAll(/data-step="([^"]*)"/g)].map((hit) => hit[1]);
    expect(
      drawn,
      'nothing is running, one workflow with four steps is on disk, and the plan column draws ' +
        'none of them. That is the state measured on 2026-08-31: the picture of the work — the ' +
        'signature of this product — only ever appeared once a run was already going, so a ' +
        'person who opened the application and pressed nothing had no way of knowing that ' +
        'Loadout draws work at all. The steps, their places and their arrows are in the file ' +
        'before anybody presses anything.',
    ).toEqual(STEPS.map((one) => one.id));

    for (const one of STEPS) {
      expect(
        column,
        'the plan column draws a step for ' +
          one.id +
          ' but never says its name, so the picture cannot be read',
      ).toContain(one.name);
    }
  });

  it('names that column, so the picture says what it is', () => {
    expect(
      screenWith([WITHOUT_ARROWS]),
      'the column carries the plan and no heading over it. The heading used to be counted off ' +
        'the run store, so it disappeared over a full picture and said "there is nothing here" ' +
        'about four steps a person can see.',
    ).toContain('>Steps<');
  });

  it('draws the same path when the file carries places and arrows too', () => {
    const column = planColumn(screenWith([WITH_ARROWS]));

    expect(
      [...column.matchAll(/data-step="([^"]*)"/g)].map((hit) => hit[1]),
      'the workflow file carries four steps, their places AND three arrows, and the plan column ' +
        'draws a different picture than it draws for the same four steps without arrows. There ' +
        'is one answer to "what is this work": the steps top to bottom, in the order the file ' +
        'lists them. Until 2026-08-31 a file like this one got the canvas instead — tiles 40 px ' +
        'tall in a 1512x950 window — and every real workflow is a file like this one.',
    ).toEqual(STEPS.map((one) => one.id));

    expect(
      column,
      'the plan column falls back to the canvas as soon as the file says where a step stands. ' +
        'The canvas belongs to the workflow editor, where a person arranges it and where the ' +
        'places are the point; a run is read top to bottom.',
    ).not.toContain('loadout-run-canvas');

    expect(
      column,
      'the file joins the four steps with three arrows and the second step never says what it ' +
        'runs after. This picture draws no arrows, so the relation the file states arrives in ' +
        'words on the card or it does not arrive on the screen at all (rule 17 works both ways: ' +
        'nothing invented, and nothing that is in the file left out).',
    ).toContain('after ' + STEPS[0].name);
  });

  it('leaves the picture to the run itself the moment a run has steps', () => {
    useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
    rememberWorkflows([WITHOUT_ARROWS]);
    rememberAgents(1);
    rememberRuns(HERE.folder, []);
    useRun.setState({
      workflow: 'Nightly checks',
      steps: [{ id: 'n1', name: 'Run the checks', state: 'running' }],
      links: null,
    });

    const column = planColumn(readable(renderToStaticMarkup(<Run />)));
    expect(
      column,
      'a run is going with its own single step, and the plan column still draws the workflow ' +
        'that Start would have picked. The preview has to step aside for the real thing the ' +
        'moment the run store has anything, or the screen shows a plan nobody is working on.',
    ).toContain('Run the checks');
    expect(
      column,
      'the preview of the next workflow is still drawn beside the run that is actually going',
    ).not.toContain('Research the codebase');
  });
});

describe('the stream column carries the last run instead of a notice about missing data', () => {
  it('names the last run of this folder, with what it cost and how it ended', () => {
    const markup = screenWith([WITHOUT_ARROWS], [LAST]);

    expect(
      markup,
      'the setup is complete, this folder has one finished run, and the widest region on the ' +
        'most important screen of this application still says "Nothing here yet: the work shows ' +
        'up line by line." — a notice about missing data, which DESIGN §6 forbids in so many ' +
        'words. The transcript column is where a transcript belongs, and the last run is one.',
    ).not.toContain('Nothing here yet');

    expect(streamColumn(markup), 'the last run is not named at all in the stream column').toContain(
      '>Ship a feature<',
    );
    expect(
      markup,
      'the card names the run and says nothing about how it went, what it cost or when it ran',
    ).toContain('done · 4 steps · $3.41 · 2026-08-30 18:12');
    expect(markup, 'there is no way back into that run from the card').toContain('>Open it<');
  });

  it('invites the first run when this folder has none, in the imperative', () => {
    const markup = screenWith([WITHOUT_ARROWS], []);
    expect(
      markup,
      'nothing has run in this folder, so there is no card to draw — and the screen falls back ' +
        'to the sentence about missing lines instead of asking for the first run',
    ).not.toContain('Nothing here yet');
    expect(markup, 'the empty stream column says nothing at all').toContain(
      'Press Run to start the first one in this folder.',
    );
  });

  it('says nothing about the previous run once a run is going', () => {
    useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
    rememberWorkflows([WITHOUT_ARROWS]);
    rememberAgents(1);
    rememberRuns(HERE.folder, [LAST]);
    useRun.setState({
      workflow: 'Ship a feature',
      steps: [{ id: 's1', name: 'Research the codebase', state: 'running' }],
      links: null,
    });

    expect(
      readable(renderToStaticMarkup(<Run />)),
      'a run is going and the stream column still tells the story of the previous one. A card ' +
        'headed "Last run" over a run that just started is a sentence about the past standing ' +
        'where a person is looking for the present.',
    ).not.toContain('Last run');
  });

  it('keeps the card out of a folder that never answered', () => {
    useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
    useRun.setState({ workflow: '', steps: [], links: null });
    rememberWorkflows([WITHOUT_ARROWS]);
    rememberAgents(1);
    rememberRuns('/Users/x/somewhere-else', [LAST]);

    expect(
      streamColumn(readable(renderToStaticMarkup(<Run />))),
      'the runs on screen came from a different folder, and the card presents them as this ' +
        "folder's last run. A run from the project next door is not this project's history " +
        '(rule 17).',
    ).not.toContain('>Ship a feature<');
  });
});
