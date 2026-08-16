/* Kryterium 8 dla T-07: jedna aktualizacja stanu na paczkę, nigdy na wiersz.
 *
 * Reguła jest z T8 §6.3: `appendLines(lines: Line[])` to JEDEN `set()`. Po stronie Rusta
 * zapłaciliśmy za nią pompą sklejającą — 0,18 µs na wiersz i 100+ klatek na sekundę zamiast
 * 13,8 µs i 1,5 klatki [T8 §5.3]. Opakowanie robiące `batch.forEach(l => sink([l]))` oddaje
 * ten zysk w całości: 500 renderów na paczkę, po tym jak Rust wysłał JEDNĄ wiadomość.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(sink).toHaveBeenCalled()`. Przechodzi właśnie na tym
 * `forEach`. Rozróżniają je: `toHaveBeenCalledTimes(1)` przy paczce 500 wierszy oraz
 * sprawdzenie DŁUGOŚCI przekazanej tablicy — bo wywołanie z jednym wierszem też jest
 * wywołaniem.
 *
 * Kanał jest atrapą, a sink licznikiem. Nie ma tu okna, magazynu stanu ani Reacta: pierścień
 * 2000 wierszy na agenta i wirtualizacja to T-08, a to kryterium mierzy jedną rzecz — ile razy
 * front zostaje ruszony na jedną paczkę.
 */
import { describe, expect, it, vi } from 'vitest';
import golden from './line-wire.golden.json';
import type { LineChannel, LineSink } from './run';
import { wireChannel } from './run';

/** Złoty plik jako zwykłe obiekty — paczki budujemy z prawdziwych wierszy, nie z atrap. */
const entries = golden as unknown as Array<Record<string, unknown>>;

/** Wiersz, który na pewno przejdzie przez lustro: zwykła proza agenta. */
function prose(): Record<string, unknown> {
  const found = entries.find((entry) => entry['kind'] === 'note');
  if (found === undefined) {
    throw new Error('the golden file has no plain note line to build batches from');
  }
  return found;
}

/** Paczka o zadanej długości. */
function batch(size: number): unknown[] {
  const one = prose();
  return Array.from({ length: size }, () => ({ ...one }));
}

/** Atrapa kanału: jedno pole, do którego `wireChannel` ma coś wpisać. */
function channel(): LineChannel {
  return { onmessage: () => {} };
}

describe('wireChannel', () => {
  it('turns one batch into one call, however many lines it carries', () => {
    const wire = channel();
    const sink = vi.fn<LineSink>();
    wireChannel(wire, sink);

    wire.onmessage(batch(500));

    expect(sink).toHaveBeenCalledTimes(1);
    expect(sink.mock.calls[0]?.[0]).toHaveLength(500);
  });

  it('calls the sink once per batch, in order, and never once per line', () => {
    const wire = channel();
    const sink = vi.fn<LineSink>();
    wireChannel(wire, sink);

    wire.onmessage(batch(500));
    wire.onmessage(batch(3));

    expect(sink).toHaveBeenCalledTimes(2);
    expect(sink.mock.calls.map((call) => call[0]?.length)).toEqual([500, 3]);
  });
});
