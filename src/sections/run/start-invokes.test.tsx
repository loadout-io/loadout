/* AC-4 dla T-30: Start w aplikacji naprawdę woła komendę biegu, z otwartym workflow.
 *
 * `@tauri-apps/api/core` jest podmieniony atrapą — żadnego żywego Tauri, żadnej przeglądarki.
 * Kryterium, które ich wymaga, nie umie być czerwone z właściwego powodu: `Failed to launch`
 * stoi na liście `NOT_A_REAL_RED`, i to jest dokładnie ten powód, dla którego szew między
 * warstwami przeżył dwadzieścia kilka zielonych zadań bez ani jednego dowodu.
 *
 * CO ZNACZY TU „KLIKNIĘCIE". To repo NIE MA jsdom — testy komponentów renderują statycznie
 * przez `renderToStaticMarkup`, a dopisanie `@testing-library/react` byłoby zmianą
 * `package.json`, czyli momentem na zatrzymanie się i zapytanie człowieka (AGENTS.md §7;
 * ta sama uwaga stoi w `src/sections/run/limits/at-once.test.tsx`). Klikamy więc to, co klika
 * przycisk: krawędź `start` z `src/sections/run/io.ts`. Rozstrzyga to, co jedzie do Rusta,
 * a nie to, jak wygląda kontrolka — a kontrolka bez handlera i tak nie wchodzi do repo
 * (niezmiennik 16).
 *
 * DLACZEGO FUNKCJA JEST WYKONYWANA, A NIE OGLĄDANA. Słaba wersja tego kryterium to grep po
 * `invoke(` w źródłach albo `expect(invoke).toHaveBeenCalled()`. Pierwsza przechodzi na
 * wywołaniu w martwej gałęzi, druga na przycisku, który wysyła pusty workflow albo wysyła go
 * dwa razy. Rozstrzygają: TREŚĆ argumentów i BRAK drugiego wywołania.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI. Krawędź rzuca dziś `not implemented`, więc każdy test, który
 * tylko liczy wystąpienia `invoke`, przechodziłby na niej bez zmiany ani jednej linii. Stąd
 * asercja jawna: tak odmówić nie wolno.
 */
import { readFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { start } from './io';

/* Atrapa jest podniesiona razem z `vi.mock`, żeby moduł sekcji dostał JĄ, a nie prawdziwy
 * transport. Nie rozwiązuje się sama: komenda biegu trwa tyle, co bieg, a „drugie kliknięcie
 * W TRAKCIE biegu" nie istnieje jako pytanie, kiedy pierwsze wywołanie kończy się natychmiast.
 * `release()` jest tu chwilą, w której bieg się kończy. */
const { invoked, release } = vi.hoisted(() => {
  const waiting: Array<() => void> = [];
  return {
    invoked: vi.fn(
      (..._sent: unknown[]) =>
        new Promise<undefined>((resolve) => {
          waiting.push(() => {
            resolve(undefined);
          });
        }),
    ),
    release: (): void => {
      while (waiting.length > 0) {
        waiting.pop()?.();
      }
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

/** Ta sama lista, którą po drugiej stronie granicy czyta `run_commands_registered.rs`. */
const GOLDEN = new URL('../../../src-tauri/commands.golden.txt', import.meta.url);

const known = new Set(
  readFileSync(GOLDEN, 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

/** Komenda, którą Start ma zawołać. Nazwa żyje na złotej liście, nie w tym pliku. */
const COMMAND = 'run_workflow';

/** Otwarty workflow. Wartość jest rozpoznawalna, żeby dało się ją znaleźć w tym, co pojechało. */
const OPEN = 'ship-a-feature.json';

/** „Ile naraz" ze stanu. Nie trójka: domyślną łatwo wpisać na sztywno i nie zauważyć. */
const AT_ONCE = 5;

/** Jedno kliknięcie Start. Odmowa wraca słowami zamiast wywracać przypadek. */
function press(): { going: Promise<void> | null; refusal: string } {
  try {
    return { going: start(OPEN, AT_ONCE), refusal: '' };
  } catch (error) {
    return { going: null, refusal: error instanceof Error ? error.message : String(error) };
  }
}

/** Odmowa, której ten szkielet nie ma prawa oddać, kiedy zadanie jest skończone. */
function notRefused(refusal: string, which: string): void {
  expect(
    refusal.includes('not implemented'),
    'src/sections/run/io.ts turned ' +
      which +
      ' down with "not implemented". That is what the edge does today, and it is why this file ' +
      'runs it instead of reading it: ' +
      refusal,
  ).toBe(false);
}

describe('Start in the app calls the run command for the open workflow', () => {
  beforeEach(() => {
    invoked.mockClear();
    release();
  });

  it('reaches Rust exactly once, under a name from the golden list', async () => {
    const first = press();
    notRefused(first.refusal, 'Start');

    expect(
      invoked.mock.calls.length,
      'Start has to reach Rust exactly once. Zero is the state this task exists to end — the ' +
        'button is wired to nothing and the engine never hears about it.',
    ).toBe(1);

    const sent = invoked.mock.calls.at(0);
    if (sent === undefined) {
      throw new Error('Start never reached Rust at all');
    }

    const name = sent.at(0);
    expect(
      typeof name === 'string' && known.has(name),
      'Start asked Rust for ' +
        String(name) +
        ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side is ' +
        'keeping that name alive, and the day it is renamed this call goes quiet.',
    ).toBe(true);
    expect(name, 'and the command it asks for is the one that starts a run').toBe(COMMAND);

    release();
    await Promise.allSettled([first.going]);
  });

  it('carries the open workflow and the how-many-at-once limit', async () => {
    const first = press();
    notRefused(first.refusal, 'Start');

    const sent = invoked.mock.calls.at(0);
    if (sent === undefined) {
      throw new Error('Start never reached Rust at all');
    }

    /* 2026-08-17 — POD KLUCZAMI, NIE GDZIEKOLWIEK W ŚRODKU. Wcześniejsza wersja tej asercji
     * zbierała wszystkie wartości proste z obiektu argumentów i pytała tylko, czy `OPEN`
     * i `AT_ONCE` gdzieś w nim są, uzasadniając to zdaniem „Tauri i tak przepisuje nazwy
     * argumentów". To zdanie było nieprawdziwe: Tauri dopasowuje argumenty PO NAZWIE, a jedyne,
     * co przepisuje, to camelCase okna na snake_case Rusta (`fileName` → `file_name`).
     * Wywołanie `{ wrongKey: OPEN, other: AT_ONCE }` przechodziło tamtą asercję i wywracało
     * się na żywym `run_workflow` — dokładnie ta klasa zieleni, dla której to kryterium
     * istnieje. Nazwy poniżej są nazwami parametrów `run_workflow` z `src-tauri/src/ipc.rs`. */
    const args = sent.at(1);
    const carried =
      typeof args === 'object' && args !== null ? (args as Record<string, unknown>) : {};

    expect(
      { fileName: carried['fileName'], howManyAtOnce: carried['howManyAtOnce'] },
      'Start called the right command and did not hand it the two answers it was given, under ' +
        'the names `run_workflow` takes them by. A call that reaches Rust without the open ' +
        'workflow starts an empty run, and one without the limit hands the scheduler a number ' +
        'nobody chose — that is invariant 11 broken quietly: the field is read, logged and ' +
        'never passed on, and the semaphore gets 1. A value sitting under some other key is ' +
        'the same thing: Tauri matches arguments by name, so `run_workflow` never sees it. ' +
        'It was called with ' +
        JSON.stringify(args),
    ).toEqual({ fileName: OPEN, howManyAtOnce: AT_ONCE });

    release();
    await Promise.allSettled([first.going]);
  });

  it('does not start a second run while the first one is still going', async () => {
    const first = press();
    notRefused(first.refusal, 'the first Start');
    expect(invoked.mock.calls.length, 'the first Start reaches Rust').toBe(1);

    // Drugie kliknięcie ZANIM pierwszy bieg wrócił. To jest jedyna chwila, w której to pytanie
    // istnieje: po powrocie biegu nie ma już czego dublować.
    const second = press();
    notRefused(second.refusal, 'the second Start');
    expect(
      invoked.mock.calls.length,
      'a second Start while the run is still going called the command again. Two runs of one ' +
        'workflow are two sets of agents writing over the same files — the thing the validator ' +
        'refuses at save time (invariant 12), except here nobody refuses, because from Rust ' +
        'both requests look perfectly good.',
    ).toBe(1);

    release();
    await Promise.allSettled([first.going, second.going]);
  });
});
