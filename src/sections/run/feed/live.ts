/* Żywy model widoku pracy tego okna i port przewijania, po którym jeździ.
 *
 * DLACZEGO NA POZIOMIE MODUŁU, A NIE W KOMPONENCIE. Bieg nie zatrzymuje się, kiedy człowiek
 * wejdzie do Agentów. Model stworzony w `useState` znika razem z ekranem sekcji, więc powrót
 * do Pracy pokazywałby pusty widok w środku biegu — i to jest awaria, którą widać dopiero
 * u użytkownika, bo w teście ekran montuje się raz i nigdy nie odmontowuje.
 *
 * DLACZEGO PORT JEST OSOBNY OD MODELU. Model nie zna DOM-u i nie ma prawa go poznać: całe
 * kryterium „widok nigdy nie przewija się sam" mierzy się atrapą portu, a atrapa działa tylko
 * dlatego, że po drugiej stronie jest interfejs, a nie `document`. Tutaj jest jedyne miejsce
 * w repo, w którym ten interfejs spotyka prawdziwy element.
 *
 * CZEGO TU NIE MA: wejścia dla paczek z kanału. Zdarzenia z Rusta dowozi T-07, a stemplowanie
 * wiersza z drutu numerem `id` i czasem `at` (`FeedLine = Line & Stamped`) jest decyzją tamtej
 * granicy — `parseLine` odrzuca dziś wiersz, który niesie `id`. Kiedy ta granica się domknie,
 * paczka wchodzi DWOMA wywołaniami i nigdzie indziej: `runFeed.appendLines(batch)` (wiersze
 * widoku) i `useRun.getState().appendLines(batch)` (okno linii, z którego bierze się
 * „Load earlier").
 */
import type { Feed, Scroller } from './model';
import { createFeed } from './model';

/**
 * Element, po którym jeździ port. Ustawia go ekran przez `ref`, zdejmuje przy odmontowaniu.
 *
 * `null` jest stanem normalnym, nie błędem: tak wygląda ten port na serwerze
 * (`renderToStaticMarkup`) i między odmontowaniem a ponownym wejściem do sekcji. Każda metoda
 * niżej jest wtedy pusta — przewinięcie okna, którego nie ma, nie jest awarią.
 */
const port: { current: HTMLElement | null } = { current: null };

/** Podpina port pod prawdziwy element historii. Wołane z `ref`, więc także z `null`. */
export function attachPort(element: HTMLElement | null): void {
  port.current = element;
}

const scroller: Scroller = {
  scrollTop(): number {
    return port.current?.scrollTop ?? 0;
  },
  scrollTo(top: number): void {
    port.current?.scrollTo({ top });
  },
  scrollIntoView(id: number): void {
    /* Wiersz nosi swój identyfikator w `data-line`, więc port nie musi znać ani modelu, ani
     * kolejności wierszy — pyta dokument o ten jeden wiersz i tyle. */
    port.current?.querySelector(`[data-line="${String(id)}"]`)?.scrollIntoView();
  },
};

/** Model widoku pracy tego okna. Jeden, na cały czas życia aplikacji. */
export const runFeed: Feed = createFeed(scroller);
