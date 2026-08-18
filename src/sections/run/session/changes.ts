/* Co po agencie ZOSTAŁO na dysku — wyliczone z linii `edit`, nie z tego, co agent o sobie mówi.
 *
 * To jest jedyne wejście bloku „co wyprodukował", jakie ta aplikacja dziś naprawdę ma, i cały
 * powód, dla którego ten plik istnieje osobno: `layout.ts` przyjmuje gotowe `Change[]` i nie ma
 * prawa wiedzieć, skąd się biorą, a ekran nie ma prawa ich liczyć u siebie (niezmiennik 23).
 *
 * DLACZEGO Z LINII `edit`, A NIE Z WIERSZY HISTORII. Wiersz historii gubi ścieżki: model
 * sklejenia oddaje `Edited 6 files` i metrykę, a nazw plików w nim nie ma. Tu potrzebna jest
 * ścieżka, więc czytamy linie z magazynu biegu (`RunState.lines`), gdzie stoją pola z drutu:
 * `paths`, `added`, `removed` (`src/ipc/types.ts`, lustro `engine::line`).
 *
 * DLACZEGO SUMUJEMY PO ŚCIEŻCE. Agent poprawiający jeden plik trzy razy nadaje trzy linie
 * `edit`. Trzy wiersze o tym samym pliku odpowiadają na pytanie „co się działo", a ten blok
 * odpowiada na inne: „co zostało zmienione i o ile". Suma jest arytmetyką na faktach, nie
 * zgadywaniem — a lista rosnąca z każdą zmianą przewraca przy dłuższym biegu sufit gęstości
 * ekranu [ARCHITECTURE §7] i chowa pierwszy plik pod czterdziestym.
 *
 * CZEGO NIE ROBIMY: nie rozbijamy linii wielościeżkowej na plik po pliku. `added`/`removed`
 * opisują CAŁĄ czynność, więc przypisanie tych samych liczb każdej z trzech ścieżek jest
 * relacją, której w danych nie ma (niezmiennik 17) — a wygląda dokładnie jak dane. Taka linia
 * dostaje jeden wiersz z wypisanymi ścieżkami, czyli mówi tyle, ile naprawdę wie.
 */
import type { FeedLine } from '../../../state/run';
import type { Change } from './layout';

/** Jak wypisujemy ścieżki jednej czynności. Ta sama fraza, co w metryce wiersza strumienia. */
const JOIN = ', ';

/**
 * Zmiany tego agenta — w kolejności PIERWSZEGO dotknięcia ścieżki.
 *
 * Kolejność pierwszego dotknięcia, nie ostatniego: blok ma być stabilny w trakcie biegu.
 * Lista przesortowana ostatnią zmianą podskakuje pod ręką człowieka, który właśnie ją czyta.
 *
 * `detailId` zostaje ten z NAJŚWIEŻSZEJ linii tej ścieżki — panel szczegółów pokazuje stan
 * pliku, a najświeższy jest tym, który jest na dysku teraz. Pierwszy opisywałby wersję sprzed
 * dwóch poprawek i nie mówiłby, że jest sprzed.
 */
export function changesOf(lines: readonly FeedLine[], agent: string): readonly Change[] {
  const byPath = new Map<string, Change>();

  for (const line of lines) {
    if (line.kind !== 'edit') continue;
    if (line.agent !== agent) continue;
    /* Linia `edit` bez ani jednej ścieżki nie ma o czym powiedzieć w tym bloku. Wiersz
     * z pustą nazwą pliku i prawdziwymi liczbami czyta się jak zmiana w niczym. */
    if (line.paths.length === 0) continue;

    const path = line.paths.join(JOIN);
    const before = byPath.get(path);
    byPath.set(path, {
      agent,
      path,
      added: (before?.added ?? 0) + line.added,
      removed: (before?.removed ?? 0) + line.removed,
      detailId: line.detailId,
    });
  }

  return [...byPath.values()];
}
