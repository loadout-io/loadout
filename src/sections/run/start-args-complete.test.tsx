/* AC-1 dla T-38: zbiór kluczy, które Start wysyła do Rusta, jest CAŁY — a listę parametrów
 * czytamy z `src-tauri/src/ipc.rs` w tym samym biegu testu.
 *
 * SŁABA WERSJA STOI DZIŚ W REPO I JEST ZIELONA. `src/sections/run/start-invokes.test.tsx`
 * (kryterium T-30) rzutuje obiekt argumentów na dwa RĘCZNIE WPISANE klucze:
 *
 *     expect({ fileName: carried['fileName'], howManyAtOnce: carried['howManyAtOnce'] })
 *
 * Ten rzut jest strukturalnie niewidzący na BRAKUJĄCY trzeci argument. Jego komentarz cytuje
 * poprawną regułę („Tauri dopasowuje argumenty PO NAZWIE") i poprawnie wskazuje `ipc.rs` jako
 * źródło nazw — a potem przepisuje z tego źródła dwie nazwy z trzech. Skutek: `run_workflow`
 * wymaga `lines: Channel<Vec<Line>>`, okno go nie wysyła, Tauri odrzuca wywołanie na
 * deserializacji argumentów, zanim wejdzie w ciało komendy, i Start odbija się przy KAŻDYM
 * kliknięciu — przy zielonym kryterium.
 *
 * Odróżnia je jedna rzecz: tutaj oczekiwany zbiór jest CZYTANY z sygnatury. Kiedy `run_workflow`
 * dostanie czwarty argument, ten test zapali się sam, bez niczyjej pamięci.
 *
 * PUNKT (c) NIE JEST OZDOBĄ, i to jest ta sama pułapka o poziom głębiej. Parser sygnatury, który
 * cicho nic nie dopasuje, zwróci pustą listę — a wtedy punkt (a) porówna dwa puste zbiory
 * i przejdzie na niczym. Dlatego najpierw dowodzimy, że parser coś zobaczył.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoked, release } = vi.hoisted(() => {
  const waiting: Array<() => void> = [];
  return {
    invoked: vi.fn(
      (..._sent: unknown[]) =>
        new Promise<undefined>((resolve2) => {
          waiting.push(() => {
            resolve2(undefined);
          });
        }),
    ),
    release: (): void => {
      while (waiting.length > 0) waiting.pop()?.();
    },
  };
});

/* Atrapa transportu. `Channel` jest tu, bo implementacja ma go założyć w oknie i podać jako
 * czwarty argument — atrapa musi umieć go oddać, inaczej test mierzyłby brak atrapy zamiast
 * braku argumentu. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { start } = await import('./io');

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const IPC = resolve(ROOT, 'src-tauri/src/ipc.rs');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Wnętrze listy parametrów funkcji o podanej nazwie, z pliku Rusta. */
function signature(rust: string, fn: string): string {
  const at = rust.indexOf(`fn ${fn}(`);
  if (at < 0) return '';
  const from = rust.indexOf('(', at);
  let depth = 0;
  for (let i = from; i < rust.length; i += 1) {
    const ch = rust[i];
    if (ch === '(') depth += 1;
    else if (ch === ')') {
      depth -= 1;
      if (depth === 0) return rust.slice(from + 1, i);
    }
  }
  return '';
}

/** Dzieli listę parametrów po przecinkach NA POZIOMIE ZERO — `State<'_, AppState>` ma własny. */
function parameters(inside: string): readonly string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of inside) {
    if (ch === '<' || ch === '(' || ch === '[') depth += 1;
    else if (ch === '>' || ch === ')' || ch === ']') depth -= 1;
    if (ch === ',' && depth === 0) {
      out.push(current);
      current = '';
    } else current += ch;
  }
  out.push(current);
  return out.map((p) => p.trim()).filter((p) => p !== '');
}

function camel(snake: string): string {
  return snake.replace(/_([a-z])/g, (_all, letter: string) => letter.toUpperCase());
}

/**
 * Nazwy argumentów, które OKNO ma wysłać, pod nazwami, których Tauri oczekuje.
 * Odpada wyłącznie to, co Tauri wstrzykuje samo — parametr typu `State<…>`.
 */
function windowSideArguments(rust: string, fn: string): readonly string[] {
  return parameters(signature(rust, fn))
    .filter((p) => !/:\s*State\s*</.test(p))
    .map((p) => camel(p.split(':')[0]?.trim() ?? ''))
    .filter((name) => name !== '');
}

const rust = fileText(IPC);
const wanted = windowSideArguments(rust, 'run_workflow');

/** Otwarty workflow i „ile naraz" ze stanu. Nie trójka: domyślną łatwo wpisać i nie zauważyć. */
const OPEN = 'ship-a-feature.json';
const AT_ONCE = 5;

describe('Start hands Rust every argument run_workflow takes', () => {
  beforeEach(() => {
    invoked.mockClear();
    release();
  });

  it('reads at least three argument names out of ipc.rs, one of them the channel', () => {
    expect(
      rust,
      'src-tauri/src/ipc.rs could not be read, so the expected set below would come from ' +
        'nowhere and the comparison would pass on two empty sets.',
    ).not.toBe('');
    expect(
      wanted.length,
      'the run_workflow signature could not be parsed out of ipc.rs. Everything this file ' +
        'asserts hangs off that list — an empty one turns the check below into `[] equals []`, ' +
        'which is exactly the shape of green this criterion exists to end.',
    ).toBeGreaterThanOrEqual(3);
    expect(
      wanted,
      'ipc.rs no longer declares a `lines` parameter on run_workflow. Either the command ' +
        'changed shape, or the parser stopped seeing it — and this test cannot tell the ' +
        'difference, so it stops here rather than judge on a list it does not trust.',
    ).toContain('lines');
  });

  it('sends exactly that set — nothing missing, nothing extra', async () => {
    const going = start(OPEN, AT_ONCE);

    const sent = invoked.mock.calls.at(0);
    if (sent === undefined) throw new Error('Start never reached Rust at all');

    const args = sent.at(1);
    const carried =
      typeof args === 'object' && args !== null ? (args as Record<string, unknown>) : {};

    expect(
      Object.keys(carried).sort(),
      'Start and run_workflow disagree about the argument set. Tauri matches arguments BY NAME ' +
        'and deserializes them BEFORE the body runs, so a missing key is not a smaller call — ' +
        'it is a rejected one: `command run_workflow missing required key lines`. The user sees ' +
        'only "Loadout could not start that run", because start.tsx turns the rejection into a ' +
        'sentence that does not name the cause. Expected set is read from ipc.rs in this run.',
    ).toEqual([...wanted].sort());

    release();
    await Promise.allSettled([going]);
  });

  it('puts a real channel under `lines`, not a placeholder', async () => {
    const going = start(OPEN, AT_ONCE);

    const args = invoked.mock.calls.at(0)?.at(1);
    const carried =
      typeof args === 'object' && args !== null ? (args as Record<string, unknown>) : {};
    const lines = carried['lines'];

    expect(
      lines === undefined || lines === null,
      'the `lines` key is there but carries nothing. Tauri deserializes a Channel out of a ' +
        'required string; undefined or null fails that deserialization exactly like a missing ' +
        'key would, and the run never starts.',
    ).toBe(false);
    expect(
      typeof lines,
      'the `lines` argument has to be the Channel object the window opened, because the run ' +
        'feed is the only way a line ever reaches the view.',
    ).toBe('object');

    release();
    await Promise.allSettled([going]);
  });
});
