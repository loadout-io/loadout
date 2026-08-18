/* Żywe modele widoku pracy — JEDEN NA WORKSPACE — i port przewijania, po którym jeżdżą.
 *
 * DLACZEGO NA POZIOMIE MODUŁU, A NIE W KOMPONENCIE. Bieg nie zatrzymuje się, kiedy człowiek
 * wejdzie do Agentów. Model stworzony w `useState` znika razem z ekranem sekcji, więc powrót
 * do Pracy pokazywałby pusty widok w środku biegu — i to jest awaria, którą widać dopiero
 * u użytkownika, bo w teście ekran montuje się raz i nigdy nie odmontowuje.
 *
 * DLACZEGO REJESTR, A NIE JEDEN MODEL — decyzja właściciela z 2026-08-18: „jak się przełączam
 * między workspace to nie tracę sesji". Do tego dnia stał tu JEDEN `createFeed` na poziomie
 * modułu, czyli jeden strumień na całą aplikację. Skutek jest dokładnie tym, co opisuje nagłówek
 * `src-tauri/src/workspace.rs`: „Pompa linii należy do KARTY, nie do widoku. Przełączenie karty
 * jest wyłącznie zmianą widoku, więc odbiornik strumienia nie ma prawa wisieć na tym, co akurat
 * widać. Wersja, w której wisi, przechodzi każdy test pisany na karcie aktywnej i gubi linie
 * dokładnie wtedy, kiedy człowiek zajrzy do innego folderu — a wraca do niej z pustą historią
 * albo z »Thinking…« sprzed dwóch minut."
 *
 * Sesja POWSTAJE NA ŻĄDANIE i ZOSTAJE. Nic jej nie kasuje: `feeds` nie ma usuwania i to jest
 * treść wymogu, nie przeoczenie — model zwolniony przy przełączeniu wyglądałby identycznie do
 * chwili powrotu, a wtedy oddawałby pustą historię biegu, który nadal idzie. Sufit pamięci
 * niesie sam model (`LINE_LIMIT` wierszy okna), więc rejestr rośnie o tyle, ile folderów
 * człowiek naprawdę otworzył.
 *
 * KTO PISZE, A KTO PATRZY — dwie różne odpowiedzi i to jest jedyna trudna rzecz w tym pliku:
 *   `feedFor(id)`   sesja TEGO workspace'a. Tym pisze POMPA — bieg należy do folderu, w którym
 *                   idzie, a nie do tego, na który człowiek akurat patrzy.
 *   `runFeed`       sesja NA WIERZCHU. Tym czyta EKRAN. Przełączenie workspace'a przestawia
 *                   wyłącznie ten uchwyt i budzi subskrybentów, więc widok przerysowuje się
 *                   z drugiej sesji, a pierwsza dalej przyjmuje linie.
 *
 * CZEGO TU NIE MA: odpowiedzi na pytanie „który workspace jest aktywny". Ten plik jej nie trzyma
 * i nie ma prawa trzymać — PYTA o nią magazyn zakresów (`activeWorkspace()`), przy każdym
 * odczycie (niezmiennik 13). Kopia trzymana tutaj i przestawiana osobnym wywołaniem wygląda
 * identycznie do pierwszego przełączenia, którego ktoś zapomni zgłosić — a wtedy okno pokazuje
 * sesję jednego folderu pod nazwą drugiego i nic o tym nie mówi.
 */
import type { Feed, FeedView } from './model';
import { createFeed } from './model';
import type { HistoryRow, Scroller } from './model';
import type { Incoming } from '../../../state/run';
import { activeWorkspace, useWorkspaces } from '../../../state/workspaces';

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

/**
 * JEDEN port na okno, dzielony przez wszystkie sesje, i to jest poprawne z definicji: port
 * wskazuje na element, który JEST NA EKRANIE, a na ekranie jest dokładnie jedna sesja. Port
 * per sesja trzymałby dziewięć uchwytów do jednego `<div>`, z których osiem opisywałoby
 * przewijanie, którego nikt nie widzi.
 */
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

/** Sesje, kluczowane identyfikatorem workspace'a. Rośnie; nic z niej nie wypada. */
const feeds = new Map<string, Feed>();

/**
 * Sesja tego workspace'a — powstaje przy pierwszym pytaniu i zostaje do końca życia okna.
 *
 * TĄ FUNKCJĄ PISZE POMPA, i to jest różnica, od której zależy cały wymóg. `start()`
 * (`src/sections/run/io.ts`) zna folder, do którego wysłał bieg — sam go podał `run_workflow`
 * czwartym argumentem — więc paczki mają dokąd trafić niezależnie od tego, na co człowiek
 * patrzy. Pompa pisząca przez `runFeed` przepisywałaby linie biegu z folderu A do sesji
 * folderu B w chwili przełączenia, i wyglądałoby to jak dwa pomieszane biegi.
 *
 * Identyfikator jest identyfikatorem workspace'a, czyli jego folderem (`id === folder`,
 * kontrakt granicy z 2026-08-18). Pusty napis znaczy „bieg bez wskazanego folderu" — Rust
 * bierze wtedy katalog, pod którym wstała aplikacja (`AppState::project_for`), i to też jest
 * jedna, konkretna sesja.
 */
export function feedFor(id: string): Feed {
  const known = feeds.get(id);
  if (known !== undefined) return known;
  const fresh = createFeed(scroller);
  feeds.set(id, fresh);
  return fresh;
}

/**
 * Sesja, którą okno POKAZUJE — czyli sesja aktywnego zakresu.
 *
 * Pusty napis, kiedy żadnego zakresu nie ma. To jest stan świeżej maszyny (magazyn startuje
 * z `activeId: null`, a `list_workspaces` oddaje wtedy pustą listę, nie błąd), więc musi być
 * zwykłą sesją, a nie gałęzią awaryjną: bieg puszczony przed wybraniem zakresu ma gdzie
 * wylądować i ma się gdzie pokazać.
 */
function shown(): string {
  return activeWorkspace()?.id ?? '';
}

/**
 * Sesja NA WIERZCHU — uchwyt, nie model.
 *
 * Nazwa zostaje ta, którą repo zna od T-07: ekran, kontrolka startu i trzy kryteria czytają
 * `runFeed` i nie mają prawa się o tej zmianie dowiedzieć (przepisanie cudzego kryterium przy
 * okazji przeprowadzki jest zmianą tego kryterium). Każda metoda rozstrzyga sesję W CHWILI
 * WYWOŁANIA, więc uchwyt nie może się rozjechać z tym, co widać.
 *
 * `appendLines` przez ten uchwyt trafia do sesji na wierzchu i jest dziś drogą DWÓCH kryteriów
 * (`live-reaches-the-feed.test.ts`, `rail-shows-agents.test.tsx`), które sieją strumień na
 * ekranie aktywnym. Pompa produkcyjna ma pisać `feedFor(folder)` — powód stoi wyżej.
 */
export const runFeed: Feed = {
  get view(): FeedView {
    return feedFor(shown()).view;
  },
  appendLines(batch: readonly Incoming[]): readonly HistoryRow[] {
    return feedFor(shown()).appendLines(batch);
  },
  jumpToNewest(): void {
    feedFor(shown()).jumpToNewest();
  },
  answer(questionId: number, option: string): void {
    feedFor(shown()).answer(questionId, option);
  },
  carriedOn(): void {
    feedFor(shown()).carriedOn();
  },
  runEnded(): void {
    feedFor(shown()).runEnded();
  },
  toggle(rowId: number): void {
    feedFor(shown()).toggle(rowId);
  },
  /**
   * Powiadomienie o zmianie TEGO, CO WIDAĆ — czyli o dwóch różnych rzeczach naraz: o nowej
   * linii w sesji na wierzchu i o tym, że wierzch się zmienił.
   *
   * Subskrypcja przewiązuje się przy przełączeniu i budzi słuchacza od razu. Wersja, która
   * subskrybuje sesję raz, na zawsze, przechodzi każdy test pisany na jednym workspace i po
   * przełączeniu rysuje historię z poprzedniego, dopóki nie napłynie linia — czyli pokazuje
   * cudzą pracę pod nazwą tego folderu.
   */
  subscribe(listener: () => void): () => void {
    let at = shown();
    let drop = feedFor(at).subscribe(listener);
    /* Magazyn zakresów woła nas przy KAŻDEJ swojej zmianie — także przy zapisie nazwy i przy
     * odmowie z dysku. Przewiązujemy się wyłącznie wtedy, gdy zmienił się aktywny zakres:
     * bezwarunkowe przewiązanie budziłoby ekran pracy przy każdym zapisie w bocznym menu. */
    const dropStore = useWorkspaces.subscribe(() => {
      const now = shown();
      if (now === at) return;
      at = now;
      drop();
      drop = feedFor(at).subscribe(listener);
      listener();
    });
    return () => {
      dropStore();
      drop();
    };
  },
};
