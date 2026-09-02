/* Z-25: pytanie rejestru, na które przyszła TA SAMA odpowiedź, nie budzi ekranu.
 *
 * ZMIERZONY DEFEKT (audyt 2026-09-02, F-2). `refreshStarted` składało nową tablicę `next`
 * i robiło `held = next; publish()` BEZWARUNKOWO — także wtedy, gdy rejestr powiedział dokładnie
 * to, co okno już wiedziało, i także wtedy, gdy obie listy były puste. `startedThings` jest
 * migawką `useSyncExternalStore`, a React porównuje ją PO REFERENCJI, więc świeża tablica co
 * sekundę znaczyła render całego ekranu Bieg co sekundę: `../index.tsx` odczytuje ten magazyn
 * (wiersz 1113), pod nim stoi cały strumień, a w nim do dwóch tysięcy wypowiedzi, z których każda
 * proza leksowała markdown od nowa. Nic z tego nie zmieniało ani jednego piksela.
 *
 * DLACZEGO TO JEST KRYTERIUM O MAGAZYNIE, A NIE O LICZNIKU RENDERÓW. To repo nie ma jsdom, więc
 * nie ma jak wyrenderować drzewa dwa razy i policzyć, ile razy zawołał się komponent. Da się za to
 * zmierzyć DOKŁADNIE tę rzecz, którą React czyta, żeby zdecydować o przerysowaniu: referencję
 * migawki i liczbę obudzonych słuchaczy. Wszystko dalej — `useSyncExternalStore` nie renderuje,
 * kiedy migawka jest tą samą wartością — jest kontraktem Reacta, nie kodem tego repo.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: „tik nie budzi nikogo". Przechodzi ją implementacja, która nie
 * publikuje NIGDY — czyli kafelek zostający nad rzeczą, która zeszła, czyli dokładnie ta wada,
 * dla której `./processes.ts` w ogóle powstało. Dlatego ostatni przypadek jest odwrotnością:
 * rejestr zapomina tę rzecz i ekran MA się o tym dowiedzieć w tym samym tyknięciu.
 *
 * ATRAPĄ JEST WYŁĄCZNIE TRANSPORT. Krawędź (`../io.ts`) jest prawdziwa, więc literówka w nazwie
 * komendy albo w kluczu argumentu przewraca ten plik — ten sam wybór i ten sam wzór, co
 * w `./again-refusal-reaches-the-person.test.ts`.
 */
import { describe, expect, it, vi } from 'vitest';

/* Odpowiedź rejestru jedzie przez uchwyt, a nie przez domknięcie nad stałą: fabryka `vi.hoisted`
 * ląduje nad wszystkim, co jest w tym pliku niżej, więc nie widzi ani jednej z tych stałych. */
const { invoked, registry } = vi.hoisted(() => {
  const registry: { rows: readonly unknown[]; group: number } = { rows: [], group: 0 };
  return {
    registry,
    invoked: vi.fn((command: string): Promise<unknown> => {
      if (command === 'list_processes') return Promise.resolve(registry.rows);
      if (command === 'start_process') return Promise.resolve(registry.group);
      return Promise.resolve(null);
    }),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const {
  openStarted,
  openedStarted,
  refreshStarted,
  startFromLine,
  startedThings,
  subscribeToStarted,
} = await import('./processes');

/** Wiersz powłoki, który człowiek wpisał. To ON jest nazwą kafelka. */
const COMMAND = 'npm run dev';

/** Grupa, którą Rust odpowiada na start. Liczba bez znaczenia, byle ta sama po obu stronach. */
const GROUP = 4242;

registry.group = GROUP;

/** Ile razy magazyn obudził ekran, odkąd ostatnio zerowaliśmy licznik. */
let woke = 0;
subscribeToStarted(() => {
  woke += 1;
});

// ── TIK NAD PUSTĄ LISTĄ ───────────────────────────────────────────────────────────────────
// Okno nic nie uruchomiło, rejestr nic nie zna. To jest stan, w którym aplikacja spędza
// większość swojego życia — i to on kosztował render na sekundę.
const emptyBefore = startedThings();
await refreshStarted();
const emptyWoke = woke;
const emptyAfter = startedThings();

// ── COŚ RUSZA, REJESTR MÓWI O NIM TO SAMO CO OKNO ─────────────────────────────────────────
woke = 0;
await startFromLine(COMMAND);
registry.rows = [{ pgid: GROUP, command: COMMAND, alive: true, said: '' }];
/* Pierwszy tik po starcie wolno mieć cokolwiek do powiedzenia — okno dopiero co dopisało wpis
 * i to o NIM jest ten przebieg. Mierzymy dopiero następny, czyli ten, który nic nie wnosi. */
await refreshStarted();
woke = 0;
const upBefore = startedThings();
await refreshStarted();
const sameWoke = woke;
const upAfter = startedThings();

// ── CZŁOWIEK WCHODZI W JEJ WYJŚCIE ────────────────────────────────────────────────────────
// To, co robi kliknięcie w kafelek. Scena schodzi tędy z rozmysłu: rzecz, która kończy się
// PODCZAS oglądania, jest jedynym stanem, w którym okno zostaje z otwartym panelem nad czymś,
// czego nie ma na liście.
const WATCHED = upBefore[0]?.id ?? '';
openStarted(WATCHED);
const watching = openedStarted();

// ── RZECZ ZESZŁA ──────────────────────────────────────────────────────────────────────────
// Rejestr zapomina wpis dopiero razem z dowodem śmierci grupy, więc „nie wiem o niej" znaczy
// tu „już jej nie ma" — i TO ekran ma zobaczyć natychmiast.
woke = 0;
registry.rows = [];
await refreshStarted();
const stillOpen = openedStarted();
const downWoke = woke;
const downAfter = startedThings();

describe('asking again with the same answer leaves the started list exactly where it was', () => {
  it('asked the other side at all, and about the line the person typed', () => {
    expect(
      invoked.mock.calls.filter((call) => call[0] === 'list_processes').length,
      'nothing crossed the boundary under that name, so every case below would be about a window ' +
        'that never asked. Either the edge stopped calling it, or the name it calls moved.',
    ).toBe(4);
    expect(
      invoked.mock.calls.filter((call) => call[0] === 'start_process').length,
      'and the line was never started, so the scene has nothing live in it to leave alone.',
    ).toBe(1);
  });

  it('wakes nobody and hands back the very same list when nothing is running', () => {
    expect(
      emptyWoke,
      'an empty list came back and the window told every screen to redraw anyway. This is the ' +
        'state the application sits in almost all the time, and it was costing one full redraw ' +
        'of the run screen every second — the stream under it, every line of prose in it, and ' +
        'the markdown of each one read from scratch. Not one pixel changed.',
    ).toBe(0);
    expect(
      emptyAfter,
      'and what the store hands the screen has to be the SAME list, not an equal one: React ' +
        'compares it by reference, so a fresh empty array is a change as far as it is concerned. ' +
        'This is the assertion that keeps "we publish less often" from passing for "we rebuild ' +
        'the array quietly".',
    ).toBe(emptyBefore);
  });

  it('wakes nobody when the registry says exactly what the window already knew', () => {
    expect(
      upBefore.length,
      'the scene needs one live line here, or "the answer said the same thing" is a sentence ' +
        'about an empty list and the case above already covers that.',
    ).toBe(1);
    expect(
      upBefore[0]?.command,
      'and it has to be the line the person typed, character for character — that line is the ' +
        'name of the tile, and comparing anything else would compare two things nobody sees.',
    ).toBe(COMMAND);
    expect(
      sameWoke,
      'the answer repeated what the window already held — same group, same line, still up, same ' +
        'output — and the window woke every screen anyway. A tick that changes nothing is the ' +
        'commonest tick there is: this is the one that repeats for hours while a dev server runs.',
    ).toBe(0);
    expect(
      upAfter,
      'and again by reference, not by value: an implementation that rebuilds an equal list every ' +
        'second is the defect, not the fix.',
    ).toBe(upBefore);
  });

  it('still wakes the screen the moment that thing is gone', () => {
    expect(
      downWoke,
      'the registry stopped knowing about it — which means it is dead and proven dead — and no ' +
        'screen was told. "Running" over a line that went down two minutes ago is the lie this ' +
        'whole file stands against, and an implementation that never publishes passes every ' +
        'case above for free.',
    ).toBeGreaterThan(0);
    expect(
      downAfter.length,
      'and the tile goes with it: a thing that went down has no tile at all, not a greyed one ' +
        'and not one that says "done" (invariant 17).',
    ).toBe(0);
  });

  it('closes the panel of the thing it just dropped, in the same publication', () => {
    expect(
      watching,
      'the panel was never open, so the case below would be about a window that had nothing to ' +
        'close. It has to be open BEFORE the thing goes down: that is the only order in which ' +
        'this state can arise at all.',
    ).toBe(WATCHED);
    expect(
      WATCHED,
      'and it has to be a real key, or the line above compares two empty strings and passes on ' +
        'nothing.',
    ).not.toBe('');
    expect(
      stillOpen,
      'the thing ended while the person was reading its output, and the window still thinks it ' +
        'is looking at something. The screen already stops drawing the panel — it looks for the ' +
        'key in a list that no longer has it — so this is a state with no picture behind it, and ' +
        'THAT is what makes it expensive: "a panel is open" is one of the two answers that keep ' +
        'the every-second question to the registry alive. Left standing, it asks forever, about ' +
        'an empty list, for a panel nobody can see.',
    ).toBe(null);
  });
});
