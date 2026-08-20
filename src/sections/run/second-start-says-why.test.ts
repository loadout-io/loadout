/* AC-2 dla T-69: odmowa drugiego startu dochodzi do OKNA, nie tylko do dziennika.
 *
 * CZEGO TU BRAKUJE DZIŚ. Naciskasz Run, bieg idzie, naciskasz Run jeszcze raz — i nie dzieje
 * się nic, o czym dałoby się przeczytać. `src/sections/run/io.ts` ma zapadkę `going`, która
 * przy drugim naciśnięciu oddaje PIERWSZY bieg zamiast wołać Rusta drugi raz. Zapadka jest
 * słuszna (dwa biegi jednego workflow to dwa zestawy agentów piszących po tych samych plikach,
 * niezmiennik 12) i `start-invokes.test.tsx` pilnuje, żeby została. Ale to, co z niej wypada,
 * jest wynikiem PIERWSZEGO biegu: kiedy tamten się udał, drugie naciśnięcie kończy się `null`,
 * czyli ciszą. Rust ma o tej samej sytuacji całe zdanie („A run is already going…”,
 * `ALREADY_GOING` w `src-tauri/src/ipc.rs`) i wpisuje je do dziennika — a człowiek stojący
 * przed ekranem nie dostaje ani słowa.
 *
 * DWIE WARSTWY, DWA PYTANIA, I OBA SĄ TU POTRZEBNE.
 *
 * Pierwsze: czy `start` — krawędź z `io.ts`, ta sama, którą woła przycisk — ODDAJE WOŁAJĄCEMU
 * POWÓD, kiedy nie zaczyna biegu. Dziś oddaje mu bieg poprzedni, a to jest odpowiedź na cudze
 * pytanie („kiedy TO się skończy" zamiast „czy MOJE naciśnięcie coś zrobiło").
 *
 * Drugie: czy ten powód dochodzi na EKRAN. Odpowiada za to `launchRun` z `./launch` — ta jedna
 * funkcja ma kształt „zdanie albo `null`, nigdy nie rzuca" (nagłówek `src/sections/run/launch.ts`)
 * i jej wynik ląduje w `setSaid(...)` w `start.tsx` oraz wraca z `/run` w wierszu wejścia.
 * Dlatego `null` i zdanie porównujemy właśnie na niej.
 *
 * CZEGO ŚWIADOMIE NIE ZMIENIAMY: PODPISU `start`. `Promise<string | null>` nie jest
 * przypisywalne do `Promise<void>`, a `start-invokes.test.tsx` — plik, którego to zadanie nie
 * posiada — trzyma wynik `start` pod adnotacją `Promise<void> | null`. Zmierzone 2026-08-20:
 * TS2322, czyli `quick-types` na czerwono na cudzym pliku. Powód wraca więc do wołającego drogą
 * odmowy, tak jak wraca odmowa Rusta, a `launchRun` wyjmuje z niej zdanie tym samym `why()`,
 * którym wyjmuje każdą inną (niezmiennik 23: jeden adapter zna kształt drutu).
 *
 * SŁABA WERSJA TEGO KRYTERIUM: „sprawdź, że cokolwiek wróciło". Przechodzi dla surowego napisu
 * z Rusta wypchniętego na ekran i dla nazwy z drutu. Rozstrzygają dwa osobne przypadki niżej:
 * jeden pyta, czy zdanie NAZYWA NASTĘPNY RUCH, drugi — czy jest zdaniem po angielsku, a nie
 * nazwą (niezmiennik 14). Druga słaba wersja: implementacja, która odmawia ZAWSZE. Przechodzi
 * wszystko powyżej i zamienia Run w przycisk do jednorazowego użycia; rozstrzyga przypadek
 * kontrolny „a start with nothing going comes back with null".
 *
 * Granica jest atrapą — żadnego żywego Tauri i żadnej przeglądarki. Kryterium, które ich
 * wymaga, nie umie być czerwone z właściwego powodu: „Failed to launch" stoi na liście
 * podpisów, które bramka odrzuca jako nie-czerwień.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';
import type { WhatIsRunning } from './io';
import { start } from './io';
import { launchRun } from './launch';
import { runTabs } from './tabs/store';
import { saidBy } from '../../ipc/why';
import { useWorkspaces } from '../../state/workspaces';

/* Atrapa granicy razem z pokrętłem, którym ustawiamy odpowiedź Rusta. Trzy odpowiedzi, bo
 * każda z nich stawia inne pytanie:
 *   'takes it'      — komenda wróciła, bieg się skończył. Kontrola dodatnia.
 *   'holds it'      — komenda NIE wróciła, czyli bieg jeszcze idzie. To jedyna chwila, w której
 *                     istnieje pytanie „drugi start przy żywym pierwszym"; `release()` jest
 *                     chwilą, w której tamten bieg schodzi.
 *   'turns it down' — Rust odmówił zdaniem. Tak wygląda para `/ask` → Run: tam zapadka `going`
 *                     jest pusta (`/ask` jej nie stawia), więc wywołanie dochodzi do Rusta
 *                     i wraca jego odmowa. */
const { invoked, boundary } = vi.hoisted(() => {
  const held: Array<() => void> = [];
  const boundary = {
    answer: 'takes it' as 'holds it' | 'takes it' | 'turns it down',
    said: '',
    release(): void {
      while (held.length > 0) {
        held.pop()?.();
      }
    },
  };
  return {
    boundary,
    invoked: vi.fn((..._sent: unknown[]): Promise<undefined> => {
      if (boundary.answer === 'turns it down') {
        /* Napisem, nie `Error`em, i to nie jest skrót: skorupy komend robią
         * `.map_err(|e| e.to_string())`, Tauri woła `reject(e)` z tym napisem, a
         * `@tauri-apps/api/core` przekazuje go dalej bez opakowania (`src/ipc/why.ts`).
         * Atrapa rzucająca `Error` mierzyłaby kształt, którego na tym drucie nie ma. */
        return Promise.reject(boundary.said);
      }
      if (boundary.answer === 'takes it') {
        return Promise.resolve(undefined);
      }
      return new Promise<undefined>((resolve) => {
        held.push(() => {
          resolve(undefined);
        });
      });
    }),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/** Zakres, w którym pracujemy. Bez niego Run odmawia o folderze i nigdy nie dochodzi do biegu. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/** Workflow z krokami, czyli taki, którego da się uruchomić. */
const CHOICE: Choice = {
  path: 'ship.json',
  name: 'Ship it',
  steps: [
    { id: 's1', name: 'Build', state: 'pending' },
    { id: 's2', name: 'Look at it', state: 'pending' },
  ],
};

/** Nazwa i plan biegu — to, co krawędź zapisuje w magazynie, kiedy woła ją przycisk. */
const WHAT: WhatIsRunning = { name: CHOICE.name, steps: CHOICE.steps };

/** Ile agentów naraz. Wartość bez znaczenia dla tych pytań, ale musi jechać. */
const AT_ONCE = 3;

/* Zdanie, którym Rust odmawia startu przy żywym biegu — kopia tego, co niesie `ALREADY_GOING`
 * w `src-tauri/src/ipc.rs`. Jest tu jako WEJŚCIE atrapy, nie jako oczekiwanie: sprawdzamy, czy
 * krawędź oddaje je co do znaku, zamiast zamieniać na własne zdanie zapasowe. */
const SAID_BY_RUST =
  'A run is already going, and Loadout leads one at a time so that Stop always reaches the one ' +
  'that is working. Press Stop first, then ask again.';

/** Nazwa z drutu wygląda tak i nie ma prawa stanąć na ekranie (niezmiennik 14). */
const A_NAME_NOT_A_SENTENCE = /(?:[a-z][a-z0-9]*_[a-z]|::|^[a-z]+\.[a-z_]+$)/;

/** Co krawędź `start` oddała wołającemu: zdanie, albo „poszła w ciszę". */
const WENT_QUIET = '';

/**
 * Drugie naciśnięcie Run przy żywym pierwszym — na krawędzi z `io.ts`, bez polityki nad nią.
 *
 * Oddaje to, co ta krawędź powiedziała WOŁAJĄCEMU: zdanie, jeśli powód doszedł jakąkolwiek
 * drogą (odmową albo wynikiem), i [`WENT_QUIET`], jeśli krawędź skończyła bez ani jednego słowa.
 * Pytamy o powód, nie o kształt: implementacja, która oddaje go inaczej niż przez odmowę,
 * przechodzi tak samo — bo pytanie brzmi „czy naciskający dowie się, dlaczego nic nie ruszyło".
 */
async function whatTheEdgeSaidOnTheSecondPress(): Promise<string> {
  boundary.answer = 'holds it';
  const first = start(CHOICE.path, AT_ONCE, WHAT, HERE.folder, null);
  const second = start(CHOICE.path, AT_ONCE, WHAT, HERE.folder, null)
    .then((value: unknown) => saidBy(value))
    .catch((refusal: unknown) => saidBy(refusal));
  boundary.release();
  const said = await second;
  await first;
  return said;
}

/**
 * Co ekran dostaje po drugim naciśnięciu Run, kiedy pierwszy bieg jeszcze idzie.
 *
 * Pierwszy bieg TRZYMA komendę (`'holds it'`), bo tylko wtedy istnieje pytanie tego kryterium:
 * po powrocie pierwszego biegu nie ma już czego dublować. `release()` przed odczytem zdejmuje go
 * z ekranu — także wtedy, gdy drugie naciśnięcie odpowiedziało od razu, bo inaczej zapadka
 * zostałaby postawiona do końca pliku i następny przypadek mierzyłby ślad po tym.
 */
async function pressRunTwice(): Promise<string | null> {
  boundary.answer = 'holds it';
  const first = launchRun(CHOICE, AT_ONCE);
  const second = launchRun(CHOICE, AT_ONCE);
  boundary.release();
  const said = await second;
  await first;
  return said;
}

describe('a second Start while a run is going says why, instead of saying nothing', () => {
  beforeEach(() => {
    invoked.mockClear();
    boundary.answer = 'takes it';
    boundary.said = '';
    boundary.release();
    useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
    runTabs.setState({ tabs: [], activeId: null, pendingClose: null });
  });

  it('gives the caller of the run edge a reason, not the run that was already going', async () => {
    const said = await whatTheEdgeSaidOnTheSecondPress();

    expect(
      said,
      'the run edge in src/sections/run/io.ts answered the second press with the FIRST run and ' +
        'not one word about it. That is an answer to a different question — "when will this ' +
        'finish" instead of "did my press do anything" — and it is why the button feels dead: ' +
        'whoever pressed it cannot tell a run that started from a run that never did.',
    ).not.toBe(WENT_QUIET);
  });

  it('hands the screen a sentence, not null, and starts nothing second', async () => {
    const said = await pressRunTwice();

    expect(
      said,
      'the second Run came back with nothing to show. That is the whole defect: a person pressed ' +
        'the button and the application answered with silence, so the only readable trace of ' +
        'what happened is a line in a log file nobody opens. Loadout leads one run at a time on ' +
        'purpose — that answer has to reach the screen, in words.',
    ).not.toBe(null);
    expect(
      typeof said,
      'the second Run came back with something that is not a sentence at all',
    ).toBe('string');
    expect(
      invoked.mock.calls.length,
      'the second Run reached Rust again. Two runs of one workflow are two sets of agents ' +
        'writing over the same files — the thing the validator turns down at save time ' +
        '(invariant 12) — and an answer on the screen does not make a second run harmless.',
    ).toBe(1);
  });

  it('names the next move: press Stop, or wait for the run that is going', async () => {
    const said = (await pressRunTwice()) ?? '';
    const move = said.toLowerCase();

    expect(
      move.includes('stop') || move.includes('wait'),
      'the answer has to name what to do next — press Stop, or wait for the run that is going. ' +
        'An answer with no way out of it leaves a person exactly where they were (DESIGN §8), ' +
        'and here there is a way out and it is one click away. It said: ' +
        JSON.stringify(said),
    ).toBe(true);
  });

  it('says it in English, never as a name off the wire', async () => {
    const said = (await pressRunTwice()) ?? '';

    expect(
      A_NAME_NOT_A_SENTENCE.test(said),
      'the answer carries a name off the wire rather than a sentence. A name from the boundary ' +
        'never reaches a screen (invariant 14): it reads as a fault in Loadout to everybody ' +
        'except the person who wrote it. It said: ' +
        JSON.stringify(said),
    ).toBe(false);
    expect(
      said.trim().split(/\s+/).length,
      'the answer is too short to tell anybody anything. It said: ' + JSON.stringify(said),
    ).toBeGreaterThanOrEqual(6);
    expect(
      /[.!]$/.test(said.trim()),
      'the answer is not a finished sentence, and half a thought on a screen reads like a fault. ' +
        'It said: ' +
        JSON.stringify(said),
    ).toBe(true);
  });

  it('comes back with null when there is nothing going and the run is taken', async () => {
    boundary.answer = 'takes it';

    const said = await launchRun(CHOICE, AT_ONCE);

    expect(
      said,
      'a Run with a folder chosen, a workflow with steps in it and nothing else going has ' +
        'nothing to turn down — and an edge that answers with words no matter what is an edge ' +
        'that turns Run into a button which works once. Every assertion above would hold for it.',
    ).toBe(null);
    expect(invoked.mock.calls.length, 'that Run never reached Rust at all').toBe(1);
  });

  it('passes on the words Rust used when Rust is the one that turned the run down', async () => {
    boundary.answer = 'turns it down';
    boundary.said = SAID_BY_RUST;

    const said = await launchRun(CHOICE, AT_ONCE);

    /* Ta para to `/ask` → Run: zapadka `going` jest wtedy pusta, bo `/ask` jej nie stawia, więc
     * wywołanie DOCHODZI do Rusta i wraca jego zdanie. Ekran ma pokazać dokładnie je, a nie
     * zdanie zapasowe wołającego („Loadout could not start that run."): to Rust jako jedyny
     * wie, dlaczego nie da się zacząć, a zdanie ogólne w miejscu, gdzie znamy powód, jest
     * gorsze niż jego brak. */
    expect(
      said,
      'the words Rust wrote for the person were dropped on the way and replaced by a general ' +
        'one. Every precise answer Loadout produces dies at that line, and what a person reads ' +
        'is the same sentence for every possible reason.',
    ).toBe(SAID_BY_RUST);
  });
});
