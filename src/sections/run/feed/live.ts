/* Żywe modele widoku pracy — JEDEN NA TERMINAL — i port przewijania, po którym jeżdżą.
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
 * 2026-08-20 (T-71) — SESJI JEST TYLE, ILE TERMINALI, NIE ILE ZAKRESÓW. Klucz był folderem, bo
 * folder był najdrobniejszą rzeczą, jaką okno umiało nazwać: karta BYŁA folderem
 * (`../tabs/store.ts`). Właściciel poprosił o drugie miejsce do pracy w projekcie, który już
 * wybrał, więc dwie karty w jednym folderze są od dziś zwykłym stanem — a rejestr kluczowany
 * folderem oddaje im WSPÓLNY model widoku. To jest dokładnie ta cicha porażka, przed którą stoi
 * całe to zadanie: terminal, który wygląda na osobny i dzieli strumień. Człowiek wpisuje zdanie
 * w jedną kartę, widzi je w obu, i przestaje wierzyć, że cokolwiek na tym ekranie należy do
 * czegokolwiek.
 *
 * Sam rejestr nie zmienił się ani o linię — jego kluczem jest zwykły napis. Zmieniło się to,
 * CZYM ten napis jest, kiedy pyta o niego `runFeed`.
 *
 * KTO PISZE, A KTO PATRZY — dwie różne odpowiedzi i to jest jedyna trudna rzecz w tym pliku:
 *   `feedFor(id)`   sesja TEGO terminalu. Tym pisze POMPA — praca należy do miejsca, w którym
 *                   się dzieje, a nie do tego, na które człowiek akurat patrzy.
 *   `runFeed`       sesja NA WIERZCHU. Tym czyta EKRAN. Przełączenie karty ALBO zakresu
 *                   przestawia wyłącznie ten uchwyt i budzi subskrybentów, więc widok
 *                   przerysowuje się z drugiej sesji, a pierwsza dalej przyjmuje linie.
 *
 * CZEGO TU NIE MA: odpowiedzi na pytania „który zakres jest aktywny" i „która karta jest na
 * wierzchu". Ten plik nie trzyma ani jednej z nich i nie ma prawa trzymać — PYTA o nie magazyn
 * zakresów (`activeWorkspace()`) i magazyn kart (`cardOnTop`), przy każdym odczycie
 * (niezmiennik 13). Kopia trzymana tutaj i przestawiana osobnym wywołaniem wygląda identycznie
 * do pierwszego przełączenia, którego ktoś zapomni zgłosić — a wtedy okno pokazuje historię
 * jednego terminalu pod nazwą drugiego i nic o tym nie mówi.
 */
import type { Feed, FeedView } from './model';
import { createFeed } from './model';
import type { HistoryRow, Scroller } from './model';
import type { Incoming } from '../../../state/run';
import { activeWorkspace, useWorkspaces } from '../../../state/workspaces';
/* Magazyn kart, nie jego fabryka: pytanie brzmi „na którą kartę patrzy TO okno", a odpowiada na
 * nie egzemplarz. Import zamyka pętlę `./live` → `../tabs/store` → `../io` → `./live` i to jest
 * bezpieczne z konstrukcji, nie z nadziei: ani jeden z tych trzech modułów nie woła cudzej
 * funkcji w czasie wczytywania, a `runTabs` powstaje w `../tabs/store` z domknięcia, które
 * `../io` tylko przekazuje dalej. */
import { cardOnTop, runTabs } from '../tabs/store';

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

/** Sesje, kluczowane tożsamością terminalu. Rośnie; nic z niej nie wypada. */
const feeds = new Map<string, Feed>();

/**
 * Sesja tego terminalu — powstaje przy pierwszym pytaniu i zostaje do końca życia okna.
 *
 * TĄ FUNKCJĄ PISZE POMPA, i to jest różnica, od której zależy cały wymóg. `start()`
 * (`src/sections/run/io.ts`) zna folder, do którego wysłał bieg — sam go podał `run_workflow`
 * czwartym argumentem — więc paczki mają dokąd trafić niezależnie od tego, na co człowiek
 * patrzy. Pompa pisząca przez `runFeed` przepisywałaby linie biegu z folderu A do sesji
 * folderu B w chwili przełączenia, i wyglądałoby to jak dwa pomieszane biegi.
 *
 * Identyfikator jest tożsamością terminalu (`../tabs/terminal.ts`). Kiedy w zakresie nie stoi
 * ani jedna karta, jest nią identyfikator zakresu, czyli jego folder (`id === folder`, kontrakt
 * granicy z 2026-08-18) — czyli folder nazywa DOMYŚLNY terminal tego zakresu, dokładnie tym
 * samym ruchem, co po stronie Rusta (`commands::chat::Threads::lines_go_to`). Pusty napis znaczy
 * „bieg bez wskazanego folderu": Rust bierze wtedy katalog, pod którym wstała aplikacja
 * (`AppState::project_for`), i to też jest jedna, konkretna sesja.
 */
export function feedFor(id: string): Feed {
  const known = feeds.get(id);
  if (known !== undefined) return known;
  const fresh = createFeed(scroller);
  feeds.set(id, fresh);
  return fresh;
}

/**
 * Sesja, którą okno POKAZUJE — czyli sesja terminalu na wierzchu.
 *
 * DWA PYTANIA, DWA MAGAZYNY, ANI JEDNEJ KOPII. „Gdzie pracujemy" odpowiada magazyn zakresów,
 * „na którą kartę patrzymy" — magazyn kart, przez `cardOnTop`, czyli przez to samo wyrażenie,
 * z którego ekran rysuje podświetlenie karty (niezmiennik 13). Druga kopia tego wyboru dałaby
 * pasek podświetlający jedną kartę nad historią należącą do drugiej.
 *
 * BEZ ANI JEDNEJ KARTY ODPOWIADA ZAKRES, i to nie jest gałąź awaryjna: świeże okno nie ma kart
 * (zakłada je `＋` albo start biegu), a bieg puszczony w takim oknie ma gdzie wylądować i ma się
 * gdzie pokazać. Folder nazywa wtedy domyślny terminal tego zakresu — ten sam ruch, co po
 * stronie Rusta.
 *
 * Pusty napis, kiedy nie ma ani karty, ani zakresu. To jest stan świeżej maszyny (magazyn
 * startuje z `activeId: null`, a `list_workspaces` oddaje wtedy pustą listę, nie błąd), więc
 * musi być zwykłą sesją.
 */
function shown(): string {
  const here = activeWorkspace();
  const { tabs, activeId } = runTabs.getState();
  return cardOnTop(tabs, activeId, here?.folder ?? null) ?? here?.id ?? '';
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
   * WIERZCH ZMIENIAJĄ DWA MAGAZYNY, więc słuchamy obu. Przełączenie zakresu w bocznym menu
   * zmienia zbiór kart, które widać; kliknięcie w kartę na pasku zmienia to, która z nich jest
   * na wierzchu. Wersja słuchająca tylko zakresów przechodzi każdy test pisany na jednej karcie
   * i po przełączeniu karty trzyma na ekranie historię poprzedniej, dopóki nie napłynie linia —
   * czyli pokazuje pracę jednego terminalu pod nazwą drugiego.
   *
   * Subskrypcja przewiązuje się przy przełączeniu i budzi słuchacza od razu. Wersja, która
   * subskrybuje sesję raz, na zawsze, oddaje `useSyncExternalStore` migawkę bez powiadomienia,
   * a to znaczy widok, który się sam nie naprawi.
   */
  subscribe(listener: () => void): () => void {
    let at = shown();
    let drop = feedFor(at).subscribe(listener);
    /* Oba magazyny wołają nas przy KAŻDEJ swojej zmianie — także przy zapisie nazwy zakresu,
     * przy odmowie z dysku i przy podniesieniu liczby pracujących agentów na karcie.
     * Przewiązujemy się wyłącznie wtedy, gdy zmieniła się sesja NA WIERZCHU: bezwarunkowe
     * przewiązanie budziłoby ekran pracy przy każdej linii biegu, bo licznik agentów na karcie
     * jedzie tą samą drogą. */
    const rebind = (): void => {
      const now = shown();
      if (now === at) return;
      at = now;
      drop();
      drop = feedFor(at).subscribe(listener);
      listener();
    };
    const dropScopes = useWorkspaces.subscribe(rebind);
    const dropCards = runTabs.subscribe(rebind);
    return () => {
      dropScopes();
      dropCards();
      drop();
    };
  },
};
