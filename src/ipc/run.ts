/* Podpięcie kanału po stronie frontu: paczka linii → JEDNA aktualizacja stanu.
 *
 * Reguła jest z T8 §6.3 i jest jedynym powodem, dla którego zysk z pompy w Ruście przeżywa
 * granicę: `appendLines(lines: Line[])` to **jeden** `set()`. Opakowanie, które robi
 * `batch.forEach(l => sink([l]))`, przywraca 500 renderów na paczkę i kasuje wszystko,
 * za co zapłaciliśmy po tamtej stronie.
 *
 * Sink jest WSTRZYKIWANY, żeby ten plik nie musiał znać magazynu stanu: pierścień 2000 linii
 * na agenta i wirtualizacja to T-08, nie to zadanie.
 *
 * STAN TEGO PLIKU: SZKIELET (2026-08-16). Ciało `wireChannel` rzuca, więc kryterium pada na
 * ZACHOWANIU, w czasie wykonania (`AGENTS.md` §2a p. 5).
 */
import type { Line } from './types';

/**
 * Minimalny kształt kanału, którego to opakowanie dotyka — dokładnie jedno pole, do zapisu.
 *
 * Wąsko z rozmysłem: `Channel<unknown[]>` z `@tauri-apps/api/core` pasuje do tego kształtu,
 * a test podstawia atrapę bez uruchamiania okna. Szerszy typ wciągnąłby do testu jednostkowego
 * całą powłokę i nie zmierzył ani jednej rzeczy więcej.
 */
export interface LineChannel {
  onmessage: (batch: unknown[]) => void;
}

/** Gdzie lądują gotowe wiersze. Jedno wywołanie na paczkę, nigdy na wiersz. */
export type LineSink = (lines: Line[]) => void;

/**
 * Wpina `sink` w kanał: każda paczka z drutu to jedno wywołanie `sink`.
 *
 * Wiersze rodzajów, których lustro nie zna, są po drodze porzucane (`parseLine`), bo
 * wywrócony `onmessage` zabiera cały widok, nie jedną linię.
 */
export function wireChannel(channel: LineChannel, sink: LineSink): void {
  // SZKIELET (2026-08-16). Świadomie brak zachowania: nic nie jest wpinane, więc paczka nie
  // dociera nigdzie. Stub, który by wpinał `sink` wprost, przechodziłby kryterium przy
  // paczkach zbudowanych z poprawnych wierszy — czyli dokładnie przy tych, którymi ono mierzy.
  throw new Error(`not implemented: wireChannel got ${typeof channel} and ${typeof sink}`);
}
