/* CO EKRAN PRACY WYCZYTAŁ Z DYSKU O TYM, CO SIĘ ZARAZ STANIE — trzy fakty, jeden dom.
 *
 * DLACZEGO NA POZIOMIE MODUŁU, A NIE W `useState` EKRANU. Dwa powody i oba są zmierzone.
 *
 * PRODUKTOWY. `src/App.tsx` trzyma w drzewie DOKŁADNIE jedną sekcję, więc każde wyjście do
 * Agentów i powrót niszczy stan komponentu. Do 2026-08-31 znaczyło to, że wracający człowiek
 * widział przez dwa obroty IPC ekran bez obrazu planu i bez ostatniego biegu — czyli dokładnie
 * ten pusty prostokąt, którego to zadanie się pozbywa, tyle że migający. Fakty przeczytane
 * z dysku przeżywają teraz odmontowanie; efekty czytają je dalej przy KAŻDYM wejściu, więc
 * świeżość jest ta sama, co przedtem — znika wyłącznie pustka między jednym a drugim.
 *
 * DOWODOWY. To repo nie ma jsdom, a `renderToStaticMarkup` nie uruchamia efektów: odpowiedź
 * z dysku zamknięta w `useState` jest wartością, do której żadne kryterium nie ma jak dojść —
 * czyli obraz planu i karta ostatniego biegu byłyby mechanizmem bez ani jednej wyroczni
 * (niezmiennik 29). Ten sam ruch i ten sam powód, co w `./limits/chosen.ts`, `./lead.ts`
 * i `./graph/opened.ts`.
 *
 * CZEGO TU NIE MA: samego czytania dysku. Ten moduł niczego nie woła i nie zna ani jednej
 * nazwy komendy — dostaje odpowiedzi od ekranu, który je zamówił (`./index.tsx`). Krawędzie
 * mieszkają w `./io.ts`, `../workflows/io.ts` i `../agents/io.ts`, i tam zostają
 * (niezmiennik 23).
 */
import type { Choice } from './choices';
import type { PastRunRow } from './io';

export interface WhatIsReady {
  /**
   * Co leży w katalogu workflow — pozycje listy, czyli nazwa, kroki, ich pozycje i strzałki.
   *
   * NIE SAME NAZWY. Obraz planu rysuje się z pozycji i strzałek z PLIKU (niezmiennik 17), więc
   * lista nazw nie miałaby czym go nakarmić i to jest cały powód, dla którego ten fakt jest
   * tutaj w takim kształcie.
   */
  readonly choices: readonly Choice[];
  /**
   * Nazwa pliku workflow, którą WSKAZAŁ CZŁOWIEK — albo `null`, kiedy nikt jeszcze nie wskazywał.
   *
   * TO JEST TEN JEDEN NOŚNIK. Zgłoszenie właściciela z 2026-08-31 brzmiało: „czemu mi się ten
   * deep reaserch pojawia, przecież nie wybrałem żadnego workflow". Ekran ogłaszał wtedy
   * „READY TO RUN" nad plikiem, który wybrała za człowieka polityka domyślna (`./choices.ts`,
   * `firstRunnable`), a jedyne dwa miejsca mówiące o tym wyborze — nagłówek i przycisk startu —
   * czytały katalog OSOBNO i trzymały dwie własne odpowiedzi. Odkąd wskazanie mieszka tutaj,
   * pytają o nie obydwa (`./choices.ts`, `willRun`).
   *
   * `null`, A NIE PIERWSZY PLIK: „nikt nie wybierał" i „ktoś wybrał ten" to dwa różne fakty
   * i ekran mówi o nich dwa różne zdania. Wypełnienie tego pola z góry skasowałoby różnicę,
   * czyli dokładnie to, o co właściciel pytał.
   *
   * NAZWA PLIKU, NIE POZYCJA LISTY: pozycje przyjeżdżają z dysku przy każdym wejściu w sekcję,
   * a wskazanie ma je przeżyć. Kopia pozycji trzymana tutaj rozjechałaby się z plikiem przy
   * pierwszym zapisie w edytorze workflow (niezmiennik 4).
   */
  readonly chosen: string | null;
  /**
   * Ilu agentów leży w bibliotece. LICZBA, nie lista: pyta o to wyłącznie przewodnik pierwszego
   * uruchomienia, a lista agentów mieszka w sekcji Agents (niezmiennik 13).
   */
  readonly agents: number;
  /**
   * Folder, o którym mówi [`runs`] — albo `null`, kiedy nikt jeszcze o żaden nie pytał.
   *
   * JEDZIE RAZEM Z WIERSZAMI, bo biegi należą do folderu. Bez tego pola przełączenie zakresu
   * zostawiałoby na ekranie kartę biegu z projektu obok, i wyglądałaby dokładnie jak własna.
   */
  readonly folder: string | null;
  /** Biegi tego folderu, od najnowszego — tak, jak oddaje je `list_runs`. */
  readonly runs: readonly PastRunRow[];
}

const NOTHING_READ_YET: WhatIsReady = {
  choices: [],
  chosen: null,
  agents: 0,
  folder: null,
  runs: [],
};

let state: WhatIsReady = NOTHING_READ_YET;

const listening = new Set<() => void>();

function tell(next: WhatIsReady): void {
  state = next;
  for (const one of listening) one();
}

/** Migawka — ta sama dla okna i dla renderu statycznego. Ten fakt nie ma stanu serwerowego. */
export function whatIsReady(): WhatIsReady {
  return state;
}

export function subscribeToWhatIsReady(onChange: () => void): () => void {
  listening.add(onChange);
  return () => {
    listening.delete(onChange);
  };
}

/** Katalog workflow właśnie się przeczytał. */
export function rememberWorkflows(choices: readonly Choice[]): void {
  tell({ ...state, choices });
}

/**
 * Człowiek właśnie wskazał, który workflow ma ruszyć — `null` cofa to do wyboru domyślnego.
 *
 * WSKAZANIE NIE JEST WERYFIKOWANE TUTAJ i to jest celowe: czy ten plik dalej leży w katalogu,
 * rozstrzyga `willRun` w chwili pytania, bo między wskazaniem a kliknięciem katalog może się
 * zmienić. Sprawdzenie zrobione w tej linii opisywałoby dysk sprzed chwili (niezmiennik 4).
 */
export function pickWorkflow(path: string | null): void {
  tell({ ...state, chosen: path });
}

/** Biblioteka agentów właśnie się przeczytała. */
export function rememberAgents(agents: number): void {
  tell({ ...state, agents });
}

/**
 * Agent WŁAŚNIE wylądował na dysku — z galerii gotowych na ekranie pierwszego otwarcia.
 *
 * DLACZEGO PLUS JEDEN, A NIE PONOWNY ODCZYT KATALOGU. Bo katalog czyta `./index.tsx` w efekcie
 * przy wejściu w sekcję, a człowiek zostaje na tym samym ekranie: bez tej linii droga stałaby
 * w miejscu do najbliższego przełączenia sekcji, czyli przycisk „Use this agent" zapisywałby
 * agenta i NIE ZMIENIAŁ ani jednego piksela. Kliknięcie, po którym nic nie drgnie, czyta się
 * jak kliknięcie, które nie doszło (niezmiennik 16) — ta sama wada i ta sama naprawa, co
 * `justSaved` w `src/state/agents.ts`.
 *
 * WOŁANE ZA `await`, nigdy przed nim: liczba podniesiona przed zapisem opisuje agenta, którego
 * dysk mógł nie przyjąć (niezmiennik 4). Pilnuje tego `./starters.ts`, jedyny wołający.
 */
export function oneMoreAgentIsSaved(): void {
  tell({ ...state, agents: state.agents + 1 });
}

/**
 * Historia biegów TEGO folderu właśnie się przeczytała.
 *
 * Folder jedzie razem z wierszami jednym zapisem, więc nie ma chwili, w której na ekranie stoi
 * już nowa lista, a etykieta mówi jeszcze o starym projekcie.
 */
export function rememberRuns(folder: string | null, runs: readonly PastRunRow[]): void {
  tell({ ...state, folder, runs });
}

/**
 * Ostatni bieg TEGO folderu, albo `null`.
 *
 * PIERWSZY WIERSZ, bo `list_runs` oddaje od najnowszego (`./history-command.ts`,
 * `theOneThatIsGoing`, akapit „PIERWSZY, nie »jedyny«"). Sortowanie tutaj byłoby drugą
 * odpowiedzią na pytanie, które ta granica już rozstrzygnęła.
 *
 * `null` TAKŻE WTEDY, GDY PYTAMY O INNY FOLDER niż ten, który odpowiedział. Bieg z projektu
 * obok nie jest ostatnim biegiem tego projektu, a karta o nim jest zdaniem nieprawdziwym —
 * gorszym niż brak zdania (niezmiennik 17).
 */
export function lastRunIn(ready: WhatIsReady, folder: string | null): PastRunRow | null {
  if (folder === null || ready.folder !== folder) return null;
  return ready.runs[0] ?? null;
}

/** Wyłącznie dla kryteriów: przywraca stan sprzed pierwszego odczytu. */
export function forgetWhatIsReady(): void {
  tell(NOTHING_READ_YET);
}
