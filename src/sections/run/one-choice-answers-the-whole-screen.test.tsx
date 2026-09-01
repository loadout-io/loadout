/* KTÓRY WORKFLOW RUSZY — jeden fakt, jedno miejsce, i widać, że ktoś go wybrał.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-08-31, słowo w słowo: „w sumie czemu mi sie ten deep reaserch
 * pojawia, to bez sensu przeciez nie wybralem zadnego workflow". Na zrzucie nagłówek pisze
 * „READY TO RUN · Deep reaserch", a przycisk obok — „Run workflow". To są dwa zdania o jednej
 * rzeczy i mówią co innego.
 *
 * ── CO ZMIERZONO, ZANIM POWSTAŁ TEN PLIK ─────────────────────────────────────────────────────
 *
 * Ten sam katalog był czytany DWA RAZY, dwoma niezależnymi wywołaniami `list_workflows`:
 * raz przez ekran (`./index.tsx`, do `./whats-ready.ts`), raz przez kontrolkę startu
 * (`./start.tsx`, do jej własnego `useState`). Zmierzone w prawdziwym chromium (`e2e/harness.ts`)
 * przez opakowanie `window.__TAURI_INTERNALS__.invoke` i odczyt śladu wywołań: po powrocie na
 * sekcję Run wychodzą DWA takie wywołania, jedno ze `start.tsx`, jedno z `index.tsx`.
 *
 * I nie jest to samo opóźnienie. Ta sama scena, w której granica odpowiada na każde wywołanie
 * INNĄ listą, daje po pełnym ustaniu okna nagłówek „Answer ten" i przycisk „Run Answer nine" —
 * dwa różne pliki naraz, na stałe, a nie przez chwilę. Dwa odczyty tej samej półki mogą po
 * prostu trafić na dwie różne odpowiedzi (niezmiennik 13).
 *
 * ── DLACZEGO MONTUJEMY CAŁY EKRAN ────────────────────────────────────────────────────────────
 *
 * SŁABĄ WERSJĄ tych punktów jest zapytanie `willRun()` o zwróconą wartość. Przechodzi ją stan,
 * w którym funkcja odpowiada bez zarzutu, a do człowieka jej odpowiedź nie dociera — czyli
 * dokładnie ta klasa wady, o której mówi zgłoszenie. Renderowany jest więc PRODUKCYJNY `<Run />`,
 * a fakty z dysku wchodzą tą samą drogą, którą wpisuje je produkcja (`./whats-ready.ts`).
 *
 * `renderToStaticMarkup` nie uruchamia efektów i to repo nie ma jsdom, więc wyboru nie da się
 * tu KLIKNĄĆ. Zmianę wyboru robi więc ta sama funkcja, którą woła handler kontrolki
 * (`pickWorkflow`), a to, że handler naprawdę wisi na kontrolce, sądzi punkt o `onChange`
 * w markupie razem z e2e w prawdziwej przeglądarce.
 *
 * ── 2026-09-01: JEDEN NOŚNIK NAZWY, I JEST NIM TYTUŁ ─────────────────────────────────────────
 *
 * Do dziś ta sama nazwa stała na tym ekranie TRZY RAZY: jako tytuł nagłówka, jako napis na
 * kontrolce, która bieg zaczyna („Run Murmur-1"), i jako zaznaczona pozycja listy wyboru. Jeden
 * fakt, trzy nośniki — czyli trzy miejsca, które mogą się nie zgodzić (niezmiennik 13), i to
 * dokładnie ta klasa wady, którą zgłosił właściciel.
 *
 * ZOSTAJE TYTUŁ, I TO ON JEST OD DZIŚ KONTROLKĄ WYBORU. Rozstrzygnięcie i jego powody stoją
 * w całości przy [`WhichWorkflow`] w `./index.tsx`. Tutaj liczy się skutek dla dwóch punktów
 * niżej, przepisanych CO DO LITERY i ani trochę co do sensu:
 *
 *   1. NAPIS NA KONTROLCE STARTU przestał nazywać workflow, więc `runControlSays(markup)` równe
 *      `'Run ' + headSays(markup)` sądziłoby dziś brzmienie, którego nie ma. Pytanie zostaje to
 *      samo — czy nagłówek i kontrolka mówią o DWÓCH RÓŻNYCH workflow — tylko zadane jest
 *      o PLIK, który ta kontrolka wyśle do Rusta, zamiast o napis na niej. To ta sama wada ze
 *      zgłoszenia (nagłówek „Answer ten", start „Answer nine") i przewraca ją ta sama mutacja:
 *      druga odpowiedź na pytanie „co ruszy" po stronie `./start.tsx`.
 *   2. `headSays` czytało `<h1[^>]*>([^<]*)<`, czyli „tytuł to goły napis". Tytuł, w którym stoi
 *      kontrolka, oddaje tej literze pusty napis — więc czyta się wnętrze `data-run-title`
 *      i zdejmuje z niego znaczniki.
 *
 * Punkt „nazwa stoi raz" pilnuje, żeby to nie było przestawieniem mebli: każdy workflow
 * z katalogu ma być na tym ekranie napisany DOKŁADNIE raz.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';

/* Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki. Efekty i tak nie biegną pod
 * `renderToStaticMarkup`, ale sam import `@tauri-apps/api/core` musi się rozwiązać, inaczej plik
 * przewraca się na ZBIERANIU i „nic nie znaleziono" czyta się jak zdana asercja. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: () => Promise.resolve(undefined),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const screen = await import('./index');
const Run = screen.default;
const { useWorkspaces } = await import('../../state/workspaces');
const { useRun } = await import('../../state/run');
const whatTheScreenRead = await import('./whats-ready');
const { forgetWhatIsReady, rememberAgents, rememberRuns, rememberWorkflows, whatIsReady } =
  whatTheScreenRead;

/**
 * Nowe wejścia do jednego nośnika i do jego polityki, czytane z PRZESTRZENI NAZW modułu, a nie
 * importem.
 *
 * Import nazwy, której jeszcze nie ma, przewraca CAŁY plik na zbieraniu — a „nie znaleziono ani
 * jednego punktu" wygląda w wyniku inaczej niż punkt, który padł, i nie niesie ani jednego zdania
 * o tym, czego brakuje. Tak każdy punkt niżej mówi wprost, czego nie zastał.
 */
const shelf: Record<string, unknown> = whatTheScreenRead;
const policy: Record<string, unknown> = await import('./choices');

function named<T>(
  from: Record<string, unknown>,
  name: string,
  missing: string,
  kind = 'function',
): T {
  const found = from[name];
  expect(typeof found, missing).toBe(kind);
  return found as T;
}

function pickWorkflow(path: string | null): void {
  named<(of: string | null) => void>(
    shelf,
    'pickWorkflow',
    'src/sections/run/whats-ready.ts carries no way to say which workflow a person chose, so ' +
      'the screen has nowhere to keep that answer. Everything below is about a choice being ' +
      'visible and changeable, and none of it can be true while the answer has no home.',
  )(path);
}

function offerFor(choice: Choice): string {
  return named<(one: Choice) => string>(
    policy,
    'offerFor',
    'src/sections/run/choices.ts says nothing about how a workflow is written on a list of ' +
      'them, so there is no rule for this point to compare the rendered list against.',
  )(choice);
}

function whoChoseIt(choices: readonly Choice[], picked: string | null): string {
  return named<(all: readonly Choice[], one: string | null) => string>(
    policy,
    'whoChoseIt',
    'src/sections/run/choices.ts has no sentence about who chose the workflow that is about to ' +
      'run. That sentence is the whole of the report this file answers: a name appeared over ' +
      'the work and the person reading it had never picked one.',
  )(choices, picked);
}

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

function step(id: string, name: string, y: number): Choice['steps'][number] {
  return { id, name, state: 'pending', kind: 'agent', at: { x: 40, y } };
}

/** Pierwszy BAJTOWO, i bez ani jednego kroku — dokładnie ten kształt, który psuł domyślny wybór. */
const NO_STEPS: Choice = {
  path: 'a-new-workflow.json',
  name: 'A new workflow',
  steps: [],
  links: [],
};

const DEEP: Choice = {
  path: 'deep-research.json',
  name: 'Deep research',
  steps: [step('plan', 'Plan steps', 40), step('read', 'Read the sources', 170)],
  links: [],
};

const SHIP: Choice = {
  path: 'ship-a-feature.json',
  name: 'Ship a feature',
  steps: [step('build', 'Build it', 40)],
  links: [],
};

const FOLDER = [NO_STEPS, DEEP, SHIP] as const;

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
 * Agent musi być, bo dopóki go nie ma, obszar pracy należy do przewodnika pierwszego
 * uruchomienia — i tak ma należeć.
 */
function screenWith(what: readonly Choice[]): string {
  useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
  useRun.setState({ workflow: '', steps: [], links: null });
  rememberWorkflows(what);
  rememberAgents(1);
  rememberRuns(HERE.folder, []);
  return readable(renderToStaticMarkup(<Run />));
}

/** Wnętrze elementu niosącego `data-run-title` — tytuł nagłówka biegu, razem ze znacznikami. */
function titleInside(markup: string): string {
  const opens = /<([a-z0-9]+)[^>]*\bdata-run-title="[^"]*"[^>]*>/.exec(markup);
  if (opens === null) return '';
  const from = opens.index + opens[0].length;
  const shuts = markup.indexOf('</' + (opens[1] ?? '') + '>', from);
  return shuts < 0 ? '' : markup.slice(from, shuts);
}

/**
 * Tytuł nagłówka biegu — to, co ekran OGŁASZA jako gotowe do uruchomienia.
 *
 * 2026-09-01 — CZYTANE Z WNĘTRZA `data-run-title`, PO ZDJĘCIU ZNACZNIKÓW. Do dziś stało tu
 * `<h1[^>]*>([^<]*)<`, czyli litera „tytuł to goły napis": klasa negatywna kończy dopasowanie
 * na pierwszym `<`, więc tytuł, w którym stoi cokolwiek prócz tekstu, oddawał jej pusty napis.
 * Nazwa workflow ma na tym ekranie jeden nośnik i jest nim tytuł, a nośnik, który ma dać się
 * zmienić, jest kontrolką — więc znaczniki w tytule stoją tam z powodu, a nie przez przypadek.
 *
 * TYTUŁ, KTÓRY JEST WYBOREM, OGŁASZA POZYCJĘ ZAZNACZONĄ: to ją widać, kiedy lista jest
 * zamknięta. Pozostałe pozycje są odpowiedzią na inne pytanie — „na co można to zmienić" — i to
 * one, a nie ona, są tu tłem. Tytuł, który jest napisem (bieg IDZIE, więc nie ma czego
 * wybierać), ogłasza ten napis; stąd druga gałąź ze zdjęciem znaczników.
 */
function headSays(markup: string): string {
  const inside = titleInside(markup);
  /* 2026-09-01 — KONTROLKA NIE JEST JUZ `<select>`, wiec „pozycja zaznaczona" to napis na
   * PRZYCISKU, a nie `<option selected>`. Lista stoi obok niego, schowana atrybutem `hidden`,
   * i jej pozycje sa odpowiedzia na inne pytanie — „na co mozna to zmienic". Czytanie calego
   * wnetrza tytulu skleilo by oba w jeden napis. */
  const button = /<button[^>]*\bdata-workflow-choice="[^"]*"[^>]*>([\s\S]*?)<\/button>/.exec(
    inside,
  );
  const shown = button?.[1] ?? inside;
  return shown.replace(/<[^>]*>/g, '').trim();
}

/** Znacznik otwierający jedynej kontrolki ręcznego startu, razem ze wszystkimi atrybutami. */
function runControl(markup: string): string {
  return /<button[^>]*data-workflow-run="manual"[^>]*>/.exec(markup)?.[0] ?? '';
}

/** Jak katalog nazywa ten plik. Pusto, kiedy takiego pliku w nim nie ma. */
function nameInFolder(folder: readonly Choice[], path: string): string {
  return folder.find((one) => one.path === path)?.name ?? '';
}

/**
 * Ile razy ten napis stoi na ekranie — w tym, co człowiek CZYTA, a nie w markupie.
 *
 * Znaczniki schodzą razem ze swoimi atrybutami: `title=` przy tytule jest podpowiedzią TEGO
 * SAMEGO elementu (nazwa ucięta na wąskim oknie zostaje do przeczytania pod kursorem), a nie
 * drugim zdaniem na ekranie, i policzony byłby jako nośnik, którego nikt nie widzi.
 */
function timesOnScreen(markup: string, said: string): number {
  /* 2026-09-01 — WNETRZE LISTY WYBORU NIE LICZY SIE JAKO DRUGIE MIEJSCE. Pytanie tego punktu
   * brzmi „czy jeden fakt ma na ekranie wiecej niz jeden nosnik" i powstalo, bo nazwa stala
   * naraz w tytule, na przycisku startu i w liscie — TRZY osobne obszary, ktore moga sie
   * rozjechac. Kontrolka i jej wlasne pozycje to JEDEN obszar: pozycja nie moze sie rozjechac
   * z przyciskiem, bo to ona go ustawia. Bez tego zawezenia punkt zadalby, zeby lista nie
   * wymieniala pliku, na ktorym stoisz — czyli zeby nie dalo sie zobaczyc, co jest wybrane. */
  const outside = markup.replace(/<ul[^>]*data-workflow-choice-list[\s\S]*?<\/ul>/g, '');
  return outside.replace(/<[^>]*>/g, '\n').split(said).length - 1;
}

/** Nazwa pliku, który ta kontrolka wyśle do Rusta. */
function runControlSends(markup: string): string {
  return /data-workflow="([^"]*)"/.exec(runControl(markup))?.[1] ?? '';
}

/**
 * Cała kontrolka wyboru workflow, od jej znacznika do zamknięcia.
 *
 * 2026-09-01 — KONTROLKA PRZESTAŁA BYĆ `<select>`. macOS rysuje jego listę WŁASNYM menu,
 * które dziedziczy stopień po kontrolce — a ta niesie `text-title` (22 px), bo zamknięta JEST
 * tytułem ekranu. Właściciel zgłosił to dwa razy („czcionka totalnie za duza"); ustawienie
 * `font-size` wprost na `<option>` macOS zignorował. Kontrolką jest dziś `<button>` z własną
 * listą, więc stopień pozycji należy do nas.
 *
 * `data-workflow-choice="`, a nie sam przedrostek: zdanie o tym, kto wybrał, niesie
 * `data-workflow-choice-said`, a sama lista `data-workflow-choice-list`. Szukanie przedrostka
 * łapałoby oba i punkt o kolejności czterech obszarów mierzyłby nie tę rzecz.
 */
function chooserAt(markup: string): number {
  return markup.indexOf('data-workflow-choice="');
}

/** Kontrolka razem z listą, kiedy ta jest otwarta. */
function chooser(markup: string): string {
  const opens = chooserAt(markup);
  if (opens < 0) return '';
  const from = markup.lastIndexOf('<', opens);
  const shuts = markup.indexOf('</span>', from);
  return shuts < 0 ? '' : markup.slice(from, shuts + '</span>'.length);
}

/** Pozycje tej kontrolki: para „nazwa pliku, napis dla człowieka". */
function offered(markup: string): readonly (readonly [string, string])[] {
  return [
    ...markup.matchAll(/<button[^>]*\bdata-choice-path="([^"]*)"[^>]*>([\s\S]*?)<\/button>/g),
  ].map((hit) => [hit[1] ?? '', (hit[2] ?? '').replace(/<[^>]*>/g, '').trim()] as const);
}

/** Nazwa pliku zaznaczona w tej kontrolce. */
function offerTaken(markup: string): string {
  const taken = /<button[^>]*\bdata-choice-path="([^"]*)"[^>]*\baria-selected="true"/.exec(markup);
  return taken?.[1] ?? '';
}

afterEach(() => {
  forgetWhatIsReady();
  useRun.setState({ workflow: '', steps: [], links: null });
});

describe('one answer to which workflow will run, and it looks like somebody chose it', () => {
  it('names the same workflow in the head and on the control that starts it', () => {
    const markup = screenWith(FOLDER);

    expect(
      headSays(markup),
      'the run screen heads itself with no workflow at all, so there is nothing for the ' +
        'control beside it to agree or disagree with, and the point below would pass on two ' +
        'empty strings.',
    ).not.toBe('');
    expect(
      runControl(markup),
      'the run screen carries no manual start control, so nothing on it can be started by ' +
        'hand and the comparison below would pass on nothing.',
    ).not.toBe('');

    expect(
      runControlSends(markup),
      'the control that starts a run by hand sends no file name at all, so the comparison ' +
        'below would be asking the folder what it calls an empty string and passing on two ' +
        'empty answers.',
    ).not.toBe('');

    expect(
      nameInFolder(FOLDER, runControlSends(markup)),
      'the head of this screen announces one workflow and the control that starts it would ' +
        'start another. Measured on the reported window: the head read "Deep reaserch" while ' +
        'the button read "Run workflow", because the two read the workflows folder separately ' +
        'and kept two answers. A person looking at this screen cannot tell which of the two is ' +
        'the one that will actually start. The head announces ' +
        JSON.stringify(headSays(markup)) +
        ' and the control sends ' +
        JSON.stringify(runControlSends(markup)) +
        '.',
    ).toBe(headSays(markup));
    expect(
      runControlSends(markup),
      'the head names a workflow and the control sends a different file name to Rust, so what ' +
        'starts is not what the screen said would start.',
    ).toBe(DEEP.path);
  });

  it('writes every workflow in the folder exactly once', () => {
    const markup = screenWith(FOLDER);

    expect(
      headSays(markup),
      'the run screen announces no workflow at all, so the counting below would be about a ' +
        'screen that names nothing and would pass on three zeroes turned into three ones by ' +
        'the list alone.',
    ).toBe(DEEP.name);
    expect(
      chooser(markup),
      'the run screen carries no control for picking a workflow, so the one place every name ' +
        'is allowed to stand is not on the screen and the counting below would be measuring ' +
        'the wrong screen.',
    ).not.toBe('');

    expect(
      FOLDER.map((one) => timesOnScreen(markup, one.name)),
      'a workflow is written on this screen more than once, and the one written most often is ' +
        'the one that will run: it stood as the title, again on the button that starts it and ' +
        'again as the taken entry of the list — one fact with three places to say it, which is ' +
        'three places to disagree (invariant 13). The owner read exactly that disagreement: ' +
        '"READY TO RUN . Deep reaserch" over a button saying "Run workflow". Counted here, in ' +
        'the order the folder lists them: ' +
        JSON.stringify(FOLDER.map((one) => [one.name, timesOnScreen(markup, one.name)])),
    ).toEqual(FOLDER.map((one) => (one.name === headSays(markup) ? 1 : 0)));
  });

  it('shows that this was a choice, and offers every workflow in the folder', () => {
    const markup = screenWith(FOLDER);

    expect(
      chooser(markup),
      'the run screen announces "ready to run" over a workflow nobody picked and offers no way ' +
        'to see or change that. The owner reported exactly this: a workflow appeared in the ' +
        'head and they had never chosen one. A decision made for a person, with nothing on the ' +
        'screen saying it was a decision, reads as the only thing that could have happened.',
    ).not.toBe('');

    expect(
      offered(markup).map((one) => one[0]),
      'the control offers a set of workflows that is not what lies in the folder. Every file ' +
        'the screen read has to be on that list, including the one with no steps: a folder ' +
        'entry that the screen silently leaves out is a file a person cannot find.',
    ).toEqual(FOLDER.map((one) => one.path));
    expect(
      offerFor(NO_STEPS),
      'the list writes a workflow with no steps exactly the way it writes one that can be ' +
        'started, so the single entry a person cannot pick reads as every other entry. Without ' +
        'that difference the comparison below would pass on a rule that says nothing.',
    ).not.toBe(NO_STEPS.name);
    expect(
      offered(markup).map((one) => one[1]),
      'the control names its choices by something other than the name each workflow gives ' +
        'itself, so the list a person reads and the head above it are written two different ' +
        'ways.',
    ).toEqual(FOLDER.map(offerFor));

    expect(
      markup,
      'the workflow with no steps is offered as if it could be started. Rust refuses that file ' +
        'with "There are no steps yet.", so an offer to pick it is an offer of a refusal ' +
        '(invariant 16).',
    ).toMatch(/<button[^>]*data-choice-path="a-new-workflow\.json"[^>]*disabled/u);

    expect(
      markup,
      'nothing on the screen says who chose this workflow. That is the whole of the report: a ' +
        'name appeared over the work and the person reading it had never picked one. A ' +
        'decision taken for somebody, with nothing saying it was a decision, reads as the only ' +
        'thing that could have happened.',
    ).toContain(whoChoseIt(FOLDER, null));
    expect(
      whoChoseIt(FOLDER, null),
      'the screen says the same thing whether Loadout chose the workflow or a person did, so ' +
        'the one sentence that answers the report answers it the same way in both states.',
    ).not.toBe(whoChoseIt(FOLDER, SHIP.path));

    expect(
      offerTaken(markup),
      'the control shows no workflow as the chosen one, so the screen offers a list and says ' +
        'nothing about which entry of it the head above is naming.',
    ).toBe(DEEP.path);

    /* HANDLER, NIE OBIETNICA PO NIM. `renderToStaticMarkup` nie zapisuje handlerow w markupie,
         wiec sam atrybut dowodzi tylko tego, ze ktos go napisal. Punkt czyta wiec ZNAK na
         kontrolce, a potem wola te sama droge, ktora idzie klikniecie pozycji, i patrzy, czy
         nosnik wyboru sie po niej ruszyl.

         2026-09-01 — DROGA TO `pickWorkflow`, nie `CHOICE_IS_LIVE.onChange`. Kontrolka przestala
         byc `<select>` (macOS rysuje jego liste wlasnym menu i w stopniu tytulu — dwa zgloszenia
         wlasciciela o zbyt duzej czcionce), a wybor robi dzis `onClick` pozycji wlasnej listy.
         Handler przeniosl sie, znak zostal. */
    expect(
      chooser(markup),
      'the control that picks a workflow carries no handler, so it can be moved and nothing ' +
        'happens. A control that accepts a decision and drops it is worse than no control ' +
        '(invariant 16).',
    ).toContain('data-workflow-choice-live="yes"');

    pickWorkflow(SHIP.path);
    expect(
      whatIsReady().chosen,
      'the handler on that control runs and the one place that keeps the choice does not move. ' +
        'A control wired to something that forgets what it was told is the same dead control ' +
        'with one more step in front of it.',
    ).toBe(SHIP.path);
  });

  it('moves the head, the control and the plan together when the choice changes', () => {
    const first = screenWith(FOLDER);
    expect(
      headSays(first),
      'nothing was read out of the head before the choice changed, so the comparison after it ' +
        'would run against an empty string.',
    ).toBe(DEEP.name);

    pickWorkflow(SHIP.path);
    const after = screenWith(FOLDER);

    expect(
      headSays(after),
      'a person picked a different workflow and the head of the screen still announces the one ' +
        'Loadout had picked for them. The choice reached somewhere, and the sentence a person ' +
        'reads is not it.',
    ).toBe(SHIP.name);
    expect(
      runControlSends(after),
      'a person picked a different workflow and the control still sends the old file name to ' +
        'Rust, so pressing it starts the workflow they moved away from.',
    ).toBe(SHIP.path);
    expect(
      FOLDER.map((one) => timesOnScreen(after, one.name)),
      'a person picked a different workflow and one of the names is now written on the screen ' +
        'twice. That is what a copy looks like when the choice moves: the one place that has ' +
        'to say which workflow will run followed the pick, and a second place kept saying what ' +
        'it was told once. Counted after the pick, in the order the folder lists them: ' +
        JSON.stringify(FOLDER.map((one) => [one.name, timesOnScreen(after, one.name)])),
    ).toEqual(FOLDER.map((one) => (one.name === headSays(after) ? 1 : 0)));
    expect(
      offerTaken(after),
      'a person picked a different workflow and the control does not show their pick as taken, ' +
        'so the next glance at it says the choice never landed.',
    ).toBe(SHIP.path);

    const drawn = [
      ...after.slice(after.indexOf('data-plan-column')).matchAll(/data-step="([^"]*)"/g),
    ].map((hit) => hit[1]);
    expect(
      drawn,
      'the picture of the work still draws the steps of the workflow a person moved away from. ' +
        'The head, the control and the picture answer one question, so a change to the answer ' +
        'has to move all three.',
    ).toEqual(SHIP.steps.map((one) => one.id));
  });

  it('stands in the head of the run, never in the loadout strip', () => {
    const markup = screenWith(FOLDER);
    const head = markup.indexOf('data-run-head');
    /* SAMA KONTROLKA, NIE PRZEDROSTEK. `data-workflow-choice` pasuje też do zdania o tym, kto
       wybrał (`data-workflow-choice-said`), a zdanie nie jest wyborem. Zmierzone mutacją: przy
       odczycie po przedrostku ten punkt był ZIELONY na nagłówku, z którego zdjęto całą listę. */
    const choice = chooserAt(markup);
    const plan = markup.indexOf('data-plan-column');
    const controls = markup.indexOf('data-workflow-controls');

    expect(
      Math.min(head, choice, plan, controls),
      'one of the four regions this point compares is not on the screen at all, so the ordering ' +
        'below would be comparing against a -1 and passing on nothing.',
    ).toBeGreaterThanOrEqual(0);

    expect(
      controls < head && head < choice && choice < plan,
      'the workflow choice is not in the head of the run. It cannot go in the loadout strip: ' +
        'that row is full to the pixel at 1512px and docs/ARCHITECTURE.md §7 leaves 3 of its 96 ' +
        'pixels of chrome unspent (8 + 1 + 32 + 52 = 93). The head of the run is content, it ' +
        'is where the screen already announces which workflow is ready, and it is where the ' +
        'answer to that announcement belongs.',
    ).toBe(true);
  });

  it('offers no choice while a run is going, because there is nothing it could change', () => {
    useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
    rememberWorkflows(FOLDER);
    rememberAgents(1);
    rememberRuns(HERE.folder, []);
    useRun.setState({ workflow: DEEP.name, steps: [...DEEP.steps], links: null });

    expect(
      chooser(readable(renderToStaticMarkup(<Run />))),
      'a run is going and the screen still offers to choose which workflow will run. Which one ' +
        'is chosen is read once, at the start, so a control taking that decision mid-run ' +
        'promises to change something it cannot change (invariant 16) — the same reason the ' +
        'task field and both limits beside it go quiet while a run is going.',
    ).toBe('');
  });
});
