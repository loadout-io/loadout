/* AC-2 dla T-38: linia oddana przez kanał dochodzi do widoku pracy PRZEZ KOD PRODUKCYJNY.
 *
 * SŁABA WERSJA, i to nie jest hipoteza — dokładnie ona przeżyła w tym repo do 2026-08-18:
 * `expect(wireChannel).toBeDefined()`. Przechodziła, bo `wireChannel` był zdefiniowany
 * i poprawny; brakowało mu **wołającego**. Miał w całym drzewie jednego importera i był nim
 * jego własny test, a `new Channel(` nie występowało w produkcji ani razu — stało wyłącznie
 * w komentarzu w `io.ts`. Widok pracy nie mógł dostać ani jednego wiersza i żaden zielony test
 * tego nie mówił.
 *
 * Druga słaba wersja jest gorsza, bo wygląda na mocną: test, który SAM woła `appendLines`
 * i sprawdza, że stan się zmienił. On dowodzi własnego wywołania, nie ścieżki produkcyjnej —
 * przechodziłby także wtedy, gdy `start()` nigdy nie założy kanału. Dlatego punkt (d) niżej
 * czyta WŁASNE ŹRÓDŁO tego pliku i wymaga, żeby nazwa `appendLines` w nim nie padła. To jedyny
 * sposób, jaki znam, żeby zdanie „przez kod produkcyjny" znaczyło coś sprawdzalnego.
 *
 * Co ten test naprawdę robi: podmienia transport, przechwytuje CZWARTY argument, którym Start
 * woła `run_workflow` — czyli kanał, który okno założyło samo — i oddaje przez niego paczkę,
 * tak jak zrobiłby to Rust. Wszystko pomiędzy jest kodem produkcyjnym.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { beforeEach, describe, expect, it, vi } from 'vitest';

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
      while (waiting.length > 0) waiting.pop()?.();
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { start } = await import('./io');
const { useRun } = await import('../../state/run');

/** Kształt kanału, którym Rust oddaje paczki. Tyle, ile ten test dotyka. */
interface Port {
  onmessage: ((batch: unknown) => void) | null;
}

/** Wiersz z drutu — najprostszy, jaki lustro `src/ipc/types.ts` rozpoznaje. */
function wireLine(agent: string, text: string): Record<string, unknown> {
  return { kind: 'agent', agent, text };
}

/** Kanał, który Start podał Rustowi. To jest cała rzecz, o którą tu chodzi. */
function portFromStart(): Port {
  const args = invoked.mock.calls.at(0)?.at(1);
  const carried = typeof args === 'object' && args !== null ? (args as Record<string, unknown>) : {};
  const port = carried['lines'];
  if (port === null || typeof port !== 'object') {
    throw new Error('Start did not hand Rust a channel under `lines`');
  }
  return port as Port;
}

/** Oddaje paczkę tak, jak zrobiłby to Rust: jedna wiadomość, wiele wierszy. */
function deliver(port: Port, batch: readonly unknown[]): void {
  if (port.onmessage === null) {
    throw new Error('nothing is listening on the channel Start opened');
  }
  port.onmessage(batch);
}

describe('a line handed to the channel reaches the work view through production code', () => {
  beforeEach(() => {
    invoked.mockClear();
    release();
  });

  it('carries a batch into the run store, in the order it arrived', async () => {
    const going = start('ship-a-feature.json', 2);
    const port = portFromStart();

    deliver(port, [wireLine('forge', 'writing the parser'), wireLine('needle', 'running tests')]);

    const lines = useRun.getState().lines;
    expect(
      lines.length,
      'the batch never reached the run store. Every piece between the channel and the store is ' +
        'production code — if this is zero, the seam is open again.',
    ).toBeGreaterThanOrEqual(2);
    expect(
      lines.slice(-2).map((line) => ('text' in line ? line.text : '')),
      'the two lines came out in a different order than they went in. Order is the one thing ' +
        'a feed cannot reconstruct later.',
    ).toEqual(['writing the parser', 'running tests']);

    release();
    await Promise.allSettled([going]);
  });

  it('adds a second batch to the first instead of replacing it', async () => {
    const going = start('ship-a-feature.json', 2);
    const port = portFromStart();

    deliver(port, [wireLine('forge', 'first')]);
    const afterOne = useRun.getState().lines.length;
    deliver(port, [wireLine('forge', 'second')]);
    const afterTwo = useRun.getState().lines.length;

    expect(
      afterTwo,
      'the second batch replaced the first instead of adding to it. A run feed that forgets ' +
        'what it already showed is a feed that scrolls itself back to nothing.',
    ).toBe(afterOne + 1);

    release();
    await Promise.allSettled([going]);
  });

  it('does not touch the store for a batch with nothing in it', async () => {
    const going = start('ship-a-feature.json', 2);
    const port = portFromStart();

    deliver(port, [wireLine('forge', 'something')]);
    const before = useRun.getState().lines;
    deliver(port, []);

    expect(
      useRun.getState().lines,
      'an empty batch changed the state. The channel calls the sink even when nothing in the ' +
        'batch survived parsing, so a fresh array here is a re-render of the whole history ' +
        'for no reason at all.',
    ).toBe(before);

    release();
    await Promise.allSettled([going]);
  });

  it('never calls appendLines itself — that is what makes this proof about production', () => {
    const own = readFileSync(fileURLToPath(import.meta.url), 'utf8');
    const body = own.slice(own.indexOf('import {'));

    /* Igła jest SKLEJANA, i to nie jest sztuczka dla oszczędności. Test, który szuka we własnym
     * źródle napisu wpisanego wprost, znajduje ten napis w sobie i jest czerwony zawsze —
     * sprawdza wtedy własny tekst zamiast własnego zachowania. Rozdzielenie nazwy od nawiasu
     * sprawia, że szukana forma wywołania nie występuje w tym pliku jako ciągły napis, więc
     * jedyne, co może ją tu wprowadzić, to prawdziwe wywołanie. */
    const call = 'appendLines' + '(';
    expect(
      body.includes(call),
      'this file calls appendLines itself. The moment it does, it stops proving ' +
        'that `start()` wires the channel and starts proving its own call — which is exactly ' +
        'the shape of green that let the seam stay broken while every test was passing.',
    ).toBe(false);
  });
});
