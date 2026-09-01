/* Zapisany strumień kroku → wiersze, które umie narysować `../feed/line.tsx`.
 *
 * DLACZEGO NIE `createFeed(...).appendLines(...)`, choć kusi. Model widoku pracy sklei sąsiednie
 * linie tego samego rodzaju drugi raz (`../feed/model.ts`, `grown`) — a te linie skleił już
 * kurator po stronie Rusta, w tym samym biegu, w którym powstały. Drugie sklejanie zamieniłoby
 * „Read 6 files" i „Read 3 files" w jeden wiersz „Read 9 files", czyli pokazałoby historię
 * inaczej, niż wyglądała, kiedy się działa. Bierzemy więc `rowFor` — jeden wiersz na jedną
 * linię — i nic poza nim.
 *
 * DLACZEGO NUMERY SĄ Z POZYCJI. `Line` z drutu nie niesie ani identyfikatora, ani czasu:
 * stempluje je granica przy odbiorze paczki (`../io.ts`), a tu paczki nie ma — jest plik.
 * Numer jest kluczem WIDOKU (rozwinięcie wiersza, `key` Reacta) i niczym więcej, więc pozycja
 * w liście jest dla niego jedyną uczciwą wartością: numer wzięty z zegara albo z licznika
 * granicy udawałby, że wiemy, kiedy to się stało, a nie wiemy — surowy strumień nie ma
 * znaczników czasu (`store::rebuild`, akapit o `events.ts`).
 *
 * OSOBNY PLIK OD PANELU, bo to jest funkcja czysta, a to repo nie ma jsdom: przeniesiona do
 * komponentu byłaby kodem, którego nie umie dotknąć żadne kryterium.
 */
import type { Line } from '../../../ipc/types';
import type { HistoryRow } from '../feed/model';
import { rowFor } from '../feed/model';

/**
 * Wiersze dla zapisanego strumienia, w kolejności z pliku.
 *
 * Linia bez zdania nie wchodzi. Dziś kurator żadnej takiej nie wypuszcza — `thinking` ma stały
 * slot na dole ekranu i do historii nie wchodzi nigdy (`docs/ARCHITECTURE.md` §6, reguła 5),
 * a `stepState` i `stepCarriedOn` przestawiają kafelek — ale ten plik czyta PLIK, a plik bywa
 * starszy albo nowszy od nas. Wiersz bez tekstu byłby pustym paskiem w historii, którego nie da
 * się odróżnić od zgubionej linii (niezmiennik 5: nieznaną linię porzucamy, biegu nie wywalamy).
 */
export function rowsOf(lines: readonly Line[]): readonly HistoryRow[] {
  return lines
    .map((line, index) => ({ ...line, id: index + 1, at: 0 }))
    .filter((line) => 'text' in line && line.text !== '')
    .map((line) => rowFor(line));
}
