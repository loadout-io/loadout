/* Warstwa, która maluje tło pod rozpoznaną nazwą workflow.
 *
 * # Dlaczego warstwa POD polem, a nie kolorowanie samego pola
 *
 * Bo `<input>` musi zostać. Przypina go osiem plików: siedem specyfikacji e2e pisze w niego przez
 * `[aria-label="Command line"]`, dwie z nich selektorem tagu, a `entry-row.test.tsx` gerpuje
 * `<input … placeholder=`. Zamiana na `contenteditable` to osiem czerwieni w chwili zapisu — i to
 * czerwieni w rzeczach, które z podświetlaniem nie mają nic wspólnego.
 *
 * # Dlaczego TŁO, a nie kolorowe litery
 *
 * Wersja z literami wymaga `text-transparent` na prawdziwym polu i jawnego `caret-color`
 * (w repo nie ma dziś ani jednego), a natywne zaznaczenie maluje po przezroczystych glifach —
 * więc przeciągany tekst po prostu znika. Tło jest tańsze i nic nie psuje: litery rysuje
 * PRAWDZIWE pole, leżące na wierzchu.
 *
 * # Dlaczego to nie jest nowy kolor
 *
 * `--accent-soft` jest w `docs/design/DESIGN.md` opisany dosłownie jako „tło elementu wybranego"
 * i maluje już te SAME nazwy dwa wiersze niżej, na liście podpowiedzi. `--live` odpada dwa razy:
 * znaczy „dzieje się teraz" (D1), a jego słownik form nie zawiera fragmentu tekstu. Piąty kolor
 * semantyczny jest w AGENTS.md §4 zakazany wprost.
 *
 * # Trzy rzeczy, bez których ta warstwa jest wadą, nie funkcją
 *
 * `aria-hidden` — bez niego czytnik ekranu czyta linię dwa razy. `pointer-events-none` — bez niego
 * kliknięcie w słowo przestaje trafiać w pole. I metryki co do piksela: ten sam krój, rozmiar
 * i zerowy padding po obu stronach, inaczej wash leży obok słowa. Ratuje nas monospace; przy
 * kroju proporcjonalnym tego kształtu nie dałoby się utrzymać.
 */
import type { ReactElement, RefObject } from 'react';

import type { Piece } from './highlight';

interface MarkProps {
  /** Kawałki linii, sklejane tu z powrotem znak w znak. */
  readonly pieces: readonly Piece[];
  /**
   * Uchwyt do tego elementu, żeby pole mogło zrównać jego przewinięcie ze swoim.
   *
   * Pole przewija się WEWNĘTRZNIE, gdy linia jest dłuższa niż kolumna, i robi to bez zdarzenia
   * `scroll` przy każdym ruchu kursora. Warstwa, która tego nie dogoni, pokazuje wash pod
   * słowem, którego już tam nie ma — czyli kłamie tym pewniej, im dłuższą linię człowiek pisze.
   */
  readonly hold?: RefObject<HTMLDivElement | null> | undefined;
}

/**
 * Podkład pod wierszem wejścia. Rysuje tekst PRZEZROCZYSTY — widać z niego wyłącznie tło.
 *
 * Pusta lista kawałków oddaje `null`, a nie pustą warstwę: widok domyślny nie ma prawa zyskać ani
 * jednego węzła tekstowego, bo zapadka gęstości `textElements` może tylko maleć
 * (`docs/ARCHITECTURE.md` §7, niezmiennik 18).
 */
export function Mark({ pieces, hold }: MarkProps): ReactElement | null {
  if (pieces.length === 0) return null;
  return (
    <div
      ref={hold}
      aria-hidden
      data-entry-mark
      className="pointer-events-none absolute inset-0 overflow-hidden whitespace-pre px-0 font-mono text-mono text-transparent"
    >
      {pieces.map((piece, at) => (
        <span
          /* Indeks jako klucz jest tu poprawny i nie jest skrótem: ta lista nie jest
             przestawiana ani filtrowana — powstaje od zera przy każdym naciśnięciu klawisza. */
          key={at}
          className={piece.known ? 'rounded-sm bg-accent-soft' : undefined}
        >
          {piece.text}
        </span>
      ))}
    </div>
  );
}
