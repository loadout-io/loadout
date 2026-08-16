/* Podpięcie kanału po stronie frontu: paczka linii → JEDNA aktualizacja stanu.
 *
 * Reguła jest z T8 §6.3 i jest jedynym powodem, dla którego zysk z pompy w Ruście przeżywa
 * granicę: `appendLines(lines: Line[])` to **jeden** `set()`. Opakowanie, które robi
 * `batch.forEach(l => sink([l]))`, przywraca 500 renderów na paczkę i kasuje wszystko,
 * za co zapłaciliśmy po tamtej stronie.
 *
 * Sink jest WSTRZYKIWANY, żeby ten plik nie musiał znać magazynu stanu: pierścień 2000 linii
 * na agenta i wirtualizacja to T-08, nie to zadanie.
 */
import { parseLine, type Line } from './types';

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
  channel.onmessage = (batch) => {
    // Pętla po paczce, a NIE `batch.forEach(l => sink([l]))`. Ta druga postać oddaje w całości
    // to, za co zapłaciliśmy pompą po stronie Rusta: paczka 500 wierszy staje się 500
    // aktualizacjami stanu, czyli 500 renderami, po tym jak Rust wysłał jedną wiadomość.
    // Filtrowanie idzie tutaj, bo `flatMap` z `?? []` alokowałby tablicę na każdy wiersz.
    const lines: Line[] = [];
    for (const value of batch) {
      const line = parseLine(value);
      if (line !== null) {
        lines.push(line);
      }
    }

    // Jedno wywołanie na paczkę — także wtedy, gdy z paczki nie przeżył ani jeden wiersz.
    // Liczba dotknięć frontu ma zależeć od liczby WIADOMOŚCI, nie od tego, ile z nich lustro
    // akurat zrozumiało; warunek „wołaj tylko, gdy coś zostało" wprowadza drugą regułę tam,
    // gdzie ma być jedna.
    sink(lines);
  };
}
