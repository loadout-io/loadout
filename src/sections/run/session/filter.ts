/* Widok jednego agenta to TEN SAM strumień z filtrem [T2 §9.1].
 *
 * To jest największa oszczędność zakresu w całym ekranie i zarazem jego jedyna nietrywialna
 * własność. Implementacja, która przelicza strumień od nowa dla jednego agenta, jest
 * napisana szybciej, wygląda czyściej i kłamie: przy dwóch agentach czytających pliki
 * w tym samym oknie widok agenta pokazuje inny podział na grupy niż strumień główny,
 * a użytkownik dostaje dwie różne odpowiedzi na jedno pytanie i nie ma jak zgadnąć,
 * która jest prawdziwa.
 *
 * Dlatego wejściem jest gotowy `FeedView`, nie surowe linie. Wiersz, którego tam nie ma,
 * nie ma prawa pojawić się tutaj — łącznie z jego licznikiem sklejania i flagą rozwinięcia,
 * którą człowiek mógł przed chwilą przestawić ręcznie.
 */
import type { Who } from '../../../state/run';
import type { FeedView, HistoryRow } from '../feed/model';
import { authorityOf } from '../rail/say';

/**
 * Wiersz strumienia plus jedno słowo o tym, kto to powiedział.
 *
 * Autorytet jest polem wiersza, nie kolorem i nie kursywą: „I fixed everything" napisane
 * przez agenta i „3 of 40 tests failed" policzone przez Loadouta czytają się identycznie,
 * dopóki nic ich nie rozdziela [00-SYNTHESIS §2.2].
 */
export interface TranscriptLine extends HistoryRow {
  readonly who: Who;
}

/**
 * Wiersze tego agenta — podciąg `view.history`, nigdy druga derywacja.
 *
 * Linia pod-agenta trafia do widoku dziecka i jednym wierszem echa do strumienia głównego
 * [T2 §9.3]; do widoku dziecka nie trafia dwa razy, a do widoku rodzica nie trafia wcale.
 */
export function sessionFeed(view: FeedView, agent: string): readonly TranscriptLine[] {
  const mine: TranscriptLine[] = [];
  for (const row of view.history) {
    if (row.agent !== agent) continue;
    /* Rozłożenie wiersza, nie zbudowanie go od nowa: `id`, `count`, `ids`, `expanded`
     * i `label` przechodzą TAKIE, jakie stoją w strumieniu głównym. Derywacja licząca od
     * nowa umie mieć rację co do każdego wiersza z osobna i mimo to postawić granice grup
     * gdzie indziej — wtedy widok agenta mówi „Read 3 files", strumień „Read 6 files",
     * i nic na ekranie nie mówi, które z nich jest prawdą.
     *
     * `expanded` jest tu przypadkiem granicznym, na którym widać to najostrzej: rozwinięcie
     * postawił CZŁOWIEK i nie ma go w niczym, z czego dałoby się wiersz policzyć jeszcze raz. */
    mine.push({ ...row, who: authorityOf(row.kind) });
  }
  /* Świeży obiekt na wiersz, bo `who` jest polem wiersza, a wiersz historii jest niezmienny.
   * Reactowi to nie szkodzi: klucz jest `id`, a ten przychodzi ze strumienia niezmieniony. */
  return mine;
}
