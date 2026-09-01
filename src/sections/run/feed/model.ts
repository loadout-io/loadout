/* Model widoku pracy: decyzja jest tutaj, render jest głupi.
 *
 * Wszystko, co produkt obiecuje w DESIGN §1 — dwie strefy o różnej fizyce, historia, która
 * przyrasta, i strefa TERAZ, która się nadpisuje — jest rozstrzygnięte w tym pliku, w czystym
 * TypeScripcie. Komponent dostaje gotowy model i go rysuje: nie filtruje, nie zwija, nie liczy.
 * Powód jest mierzalny, nie estetyczny: kuracja w CSS-ie da się zepsuć zmianą arkusza stylów,
 * a wtedy „czysty widok" jest wrażeniem, nie własnością (niezmiennik 15).
 *
 * Dwie rzeczy, których ten plik NIE robi, i to jest jego najważniejsza cecha:
 *
 * 1. NIE PRZEWIJA. Model nigdy nie woła portu przewijania z własnej woli. Przypięcie do dołu
 *    robi układ (`column-reverse`), nie skrypt. `el.scrollTop = el.scrollHeight` w efekcie na
 *    każdą paczkę wygląda idealnie na demie z dwudziestoma liniami i po dziesięciu minutach
 *    pracy czterech agentów wyrywa użytkownikowi zdanie spod oczu, zanim je doczyta.
 *    Jedyne legalne wywołanie imperatywne to `jumpToNewest()`, które ma swój przycisk.
 *
 * 2. NIE PRZELICZA HISTORII OD NOWA. `view.history` zmienia tożsamość dokładnie wtedy, kiedy
 *    coś do niej weszło. Paczka złożona z samych `thinking` zostawia tę samą tablicę, bo
 *    `Thinking…` nie jest linią. Przy czterech agentach przemapowanie całej historii co paczkę
 *    jest poprawne co do wartości i katastrofalne dla Reacta.
 */
import type { Answer, FeedLine, Incoming } from '../../../state/run';
import { LINE_LIMIT, stepIsOver } from '../../../state/run';
import type { Kind } from './kinds';
import { kinds } from './kinds';

/**
 * Port przewijania — jedyna droga modelu do prawdziwego elementu.
 *
 * `scrollTop` jest METODĄ, nie polem, i to nie jest kosmetyka: atrapa w teście zapisuje wtedy
 * także ODCZYT pozycji. Implementacja, która „przewija tylko wtedy, gdy jesteś na dole",
 * musi najpierw zapytać, gdzie jesteś — więc kryterium „zero wywołań" łapie ją, zanim zdąży
 * cokolwiek przewinąć.
 */
export interface Scroller {
  scrollTop(): number;
  scrollTo(top: number): void;
  scrollIntoView(id: number): void;
}

/** Jeden agent, jedna linia, przepisywana. Jak `top`, nie jak `tail -f` [DESIGN §1]. */
export interface NowRow {
  readonly agent: string;
  /** Co ten agent robi teraz — jedno zdanie po angielsku. */
  readonly text: string;
}

export interface NowZone {
  /**
   * Jeden wiersz na agenta biegu, który IDZIE. Nigdy wycinek historii — wycinek pełznie.
   *
   * Dwa rodzaje wiersza tu nie wchodzą i oba mówiłyby o pracy, której nikt nie wykonuje
   * (niezmiennik 17): wiersz złożony przez samo okno (patrz [`windowWrote`]) i każdy wiersz
   * biegu, który już zszedł (patrz `Feed.runEnded`). Pusta lista jest zwykłym stanem tej
   * strefy — tak wygląda aplikacja, w której nic nie biegnie.
   */
  readonly rows: readonly NowRow[];
  /**
   * JEDNO pole, nigdy tablica: `Thinking…` to status, nie linia [T2 §7.3 reguła 5].
   * Trzyma nazwę agenta, którego slot jest żywy, albo `null`, gdy padła prawdziwa linia.
   */
  readonly thinking: string | null;
}

/** Wiersz historii. Jeden wiersz może stać za kilkoma liniami — patrz `ids`. */
export interface HistoryRow {
  /** Identyfikator wiersza: identyfikator PIERWSZEJ linii grupy. */
  readonly id: number;
  /**
   * Kiedy ta linia napłynęła, w milisekundach — ten sam stempel, który granica nadaje wierszowi
   * z drutu (`../../../state/run.ts`, `Stamped.at`).
   *
   * 2026-08-31 — DOPISANE, BO ZEGAR WIERSZA NIE MIAŁ DROGI NA EKRAN. Makieta strumienia podpisuje
   * KAŻDĄ wypowiedź godziną (`14:00:44`), a widok nie miał jej skąd wziąć: stempel dojeżdżał do
   * modelu, model liczył z niego okno sklejania i tam go zostawiał. Napisanie godziny w
   * komponencie z `Date.now()` byłoby czasem RENDERU, nie czasem zdarzenia — czyli liczbą, która
   * zmienia się przy każdym przerysowaniu i nie mówi nic o biegu (niezmiennik 17).
   *
   * WIERSZ SKLEJONY TRZYMA STEMPEL PIERWSZEJ LINII, tak samo jak trzyma jej identyfikator: to
   * jest chwila, w której ta czynność się ZACZĘŁA, i to ją podpisuje wiersz mówiący o całej
   * grupie.
   */
  readonly at: number;
  readonly kind: Kind;
  readonly agent: string;
  /** Tekst po angielsku z zamkniętej tabeli; licznik jest zawsze w środku [T2 ryzyko 3]. */
  readonly label: string;
  readonly count: number;
  /** Identyfikatory sklejonych linii w kolejności napłynięcia — rozwinięcie oddaje je. */
  readonly ids: readonly number[];
  readonly expanded: boolean;
  /**
   * Prawa kolumna wiersza: liczba, którą ta czynność zostawiła po sobie. Puste, kiedy żadnej nie ma.
   *
   * Makieta (`.ln .m`) ma tam `+42 −8` przy zmianie pliku i `3 of 40` przy sprawdzeniu, które
   * padło — i to jest jedyna metryka, jaką ten widok pokazuje przy wierszu. Do 2026-08-18
   * `line.tsx` rysował całą prawą kolumnę jednym szarym `<p>`, więc liczby z drutu (`added`,
   * `removed`, `preview`) nie miały gdzie wylądować i nie docierały nigdzie.
   *
   * SKŁADANE TUTAJ, nie w komponencie: wiersz sklejony stoi za kilkoma liniami, więc „co
   * pokazać z ostatniej" jest decyzją modelu. Komponent, który liczyłby to sam, potrzebowałby
   * całej linii z drutu i byłby drugim miejscem, w którym powstaje ta sama fraza.
   */
  readonly metric: string;
  /** Ostatnie 20 linii wyjścia; niepuste tylko dla `ran`, które padło [T2 §7.3 reguła 3]. */
  readonly output: readonly string[];

  /**
   * Cała proza tego wiersza, kiedy nie zmieściła się w nim — pusta, kiedy się zmieściła.
   *
   * OSOBNE POLE OD `output`, i to nie jest podwójna odpowiedź na jedno pytanie. `output` jest
   * wyjściem maszyny i widok rysuje je monospacem z czerwoną krawędzią, bo mówi o czymś, co
   * padło. To jest zdanie agenta i czyta się je jak tekst. Jedno pole na oba znaczyłoby, że
   * wiersz nie wie, co rysuje, a nazwa `output` przy odpowiedzi agenta byłaby po prostu
   * nieprawdziwa.
   */
  readonly body: readonly string[];
  /**
   * Tylko na wierszu `done`: jak agent skończył — lustro `engine::line::Ended`.
   *
   * 2026-08-22 — niesie to szyna agentów, żeby kafelek nie musiał zgadywać stanu ani czytać go
   * ze zdania. `Done` / `Didn't work` / `Stopped` są prozą dla człowieka i wolno je przepisać;
   * ta wartość jest decyzją, która za nimi stoi.
   */
  readonly ended?: 'well' | 'badly' | 'stopped';
  /** Numer, o który poprosi panel szczegółów. Sam panel jest poza tym zadaniem. */
  readonly detailId: number | null;
  /**
   * Komenda, którą przyniósł wiersz propozycji — znak w znak taka, jaką napisał lider.
   *
   * PRZEPISANA Z LINII, NIGDY WYCIĘTA Z `label`. Tekst przyjeżdża z drutu sklejony do jednej
   * linii (reguła 1), więc granica między komendą a powodem, dla którego lider ją podaje, jest
   * po tej stronie nieodtwarzalna — a okno, które składa komendę z powrotem z prozy, jest tym
   * samym oknem, które samo szuka `/run` w akapicie, tylko o krok dalej (niezmiennik 15).
   * Rust wysyła ją osobnym polem dokładnie po to (`engine::line::Line::Suggested`).
   *
   * BEZ TEGO POLA PRZYCISK PROPOZYCJI JEST MARTWY W DZIAŁAJĄCEJ APLIKACJI: `./line.tsx` rysuje
   * go wyłącznie wtedy, gdy dostanie komendę, a wiersz jest jedyną rzeczą, którą widok dostaje.
   * Komenda kończąca bieg w modelu daje kontrolkę, którą umie narysować tylko test — czyli tę
   * samą rodzinę, dla której istnieje `checks/quick-wired.sh`, po stronie Reacta.
   *
   * Nieobowiązkowe, bo „nie ma komendy" i „ten wiersz nie jest propozycją" to jedno i to samo:
   * pole wymagane kazałoby każdemu wierszowi odpowiadać na pytanie, które dotyczy jednego
   * rodzaju.
   */
  readonly command?: string | undefined;
}

/** Pytanie do człowieka. Przyklejone, dopóki nie ma odpowiedzi [T2 §7.2 wiersz 10]. */
export interface Question {
  readonly id: number;
  readonly text: string;
  readonly options: readonly string[];
  /**
   * Podpis, pod którym to pytanie stanęło na ekranie.
   *
   * 2026-08-30 — DOPISANE, BO ODPOWIEDŹ MA DWIE DROGI. W jednym strumieniu stoją dwa różne
   * pytania: to od lidera, na którym stoi zablokowana tura agenta, i to z kafelka kontrolnego,
   * na którym stoi bieg. Okno nie ma jak ich rozróżnić — więc podaje podpis dalej, a rozstrzyga
   * strona, która wie (`commands::chat::Threads::answer_in`). Bez tego pola odpowiedź na kafelek
   * odblokowywałaby przy okazji pytanie lidera, zdaniem, które go nie dotyczy.
   */
  readonly agent: string;
}

/** Czyja jest teraz kolej. `you` maluje się kolorem `--attend` [DESIGN §3]. */
export type Attention = 'agents' | 'you';

export interface FeedView {
  readonly history: readonly HistoryRow[];
  readonly now: NowZone;
  readonly pinned: Question | null;
  /**
   * Czy bieg STOI na punkcie kontrolnym i czeka, żeby go puścić dalej.
   *
   * 2026-08-18 — PO CO TO POLE, ZMIERZONE. Kontrolka „Continue" renderowała się dokładnie przy
   * `pinned !== null`, a `answer()` zdejmuje przypięcie — więc odpowiedź na pytanie ODMONTOWYWAŁA
   * jedyną kontrolkę wołającą `continue_run` i bieg parkował NA ZAWSZE. To są dwa różne fakty
   * i dlatego są dwoma polami: `pinned` mówi „jest pytanie bez odpowiedzi" (i to ono rysuje blok
   * z opcjami), `parked` mówi „bieg czeka na człowieka i nie ruszy, dopóki go nie puścisz".
   * Odpowiedź gasi pierwsze i **nie** rusza drugiego — bo po stronie Rusta
   * (`commands::run::wait_for_a_person`) bieg dalej stoi, dopóki nie podbije się licznik zgód.
   *
   * Gaśnie w dwóch chwilach i w żadnej innej: kiedy bieg zostanie puszczony (`carriedOn`) i kiedy
   * bieg się skończy (`runEnded`). Kontrolka bez roboty nie ma prawa zostać na ekranie
   * (niezmiennik 16).
   */
  readonly parked: boolean;
  /**
   * Odpowiedź, która ma POJECHAĆ DO AGENTA razem z puszczeniem biegu — albo pusty napis.
   *
   * 2026-08-18 — PO CO TO POLE. Człowiek pisze zdanie w karcie „Needs your answer", a agent po
   * drugiej stronie nie dostaje z niego ani litery: `continue_run` bierze dziś samo `State`
   * i podbija licznik zgód, więc treść zostawała w oknie i nigdzie nie jechała. Karta pytania
   * przyjmująca zdanie, którego nikt nie przeczyta, jest kontrolką bez skutku (niezmiennik 16) —
   * gorszą od jej braku, bo wygląda na rozmowę.
   *
   * TO NIE JEST DRUGA KOPIA `answers` (niezmiennik 13). `answers` jest ZAPISEM tego, co człowiek
   * odpowiedział, i zostaje na zawsze; `toCarry` jest KOLEJKĄ WYSYŁKOWĄ o pojemności jednego
   * zdania i gaśnie w chwili, w której bieg ruszył (`carriedOn`) albo zszedł (`runEnded`).
   * Jedno pole na oba fakty wysyłałoby przy drugim punkcie kontrolnym odpowiedź na pierwszy.
   *
   * Pusty napis, nie `null`: „nic do przewiezienia" i „przewieź puste zdanie" to ta sama rzecz
   * dla strony, która to odbiera, a dwa kształty na jeden stan dają gałąź, której nie da się
   * przejść inaczej niż przez pomyłkę. Na drucie stoi `Option<String>`
   * (`src-tauri/src/ipc.rs`, `continue_run(answer)`), więc przełożenie pustego napisu na `null`
   * należy do krawędzi sekcji (`../io.ts`) — model nie zna kształtów drutu (niezmiennik 23).
   */
  readonly toCarry: string;
  readonly attention: Attention;
  readonly answers: readonly Answer[];
}

export interface Feed {
  readonly view: FeedView;
  /**
   * Przyjmuje paczkę z kanału i oddaje wiersze, które WESZŁY DO HISTORII — te same obiekty,
   * które od tej chwili siedzą w `view.history`. Paczka bez ani jednej linii historii oddaje
   * pustą tablicę i nie rusza `view.history`.
   */
  appendLines(batch: readonly Incoming[]): readonly HistoryRow[];
  /** Jedyna legalna droga imperatywna do portu przewijania. Ma swój przycisk. */
  jumpToNewest(): void;
  /**
   * Odpowiedź człowieka: zdejmuje przypięcie tego pytania i zapisuje ją z `who: 'you'`.
   *
   * NIE ODPARKOWUJE BIEGU. Po stronie Rusta odpowiedź nie jest zgodą na dalszą pracę — bieg
   * stoi w `wait_for_a_person`, dopóki nie podbije się licznik zgód (`continue_run`), więc
   * `parked` zostaje i kontrolka „dalej" zostaje razem z nim.
   *
   * Zdanie ląduje też w `view.toCarry`, czyli w kolejce wysyłkowej do agenta. Zapis bez wysyłki
   * jest tym, czym była ta karta do 2026-08-18: miejscem, w którym człowiek pisze do nikogo.
   */
  answer(questionId: number, option: string): void;
  /**
   * Bieg został puszczony dalej: gasi `parked`.
   *
   * Wołane po tym, jak `continue_run` WRÓCIŁO — komenda rozwiązuje się dopiero wtedy, kiedy bieg
   * naprawdę ruszył (`wait_until_moving`), więc gaszenie wcześniej pokazywałoby ruszający bieg
   * na sekundę przed tym, jak ruszył.
   */
  carriedOn(): void;
  /**
   * Bieg zszedł — koniec, odmowa albo zatrzymanie. Gasi KAŻDE pole, które opisywało żywy bieg.
   *
   * LISTA JEST ZAMKNIĘTA I WYPISANA, i to jest jedyna postać tej reguły, której nie trzeba pisać
   * piąty raz: strefa TERAZ (`NowZone.rows`, `NowZone.thinking`), pytanie bez odpowiedzi
   * (`pinned`, a przez nie `attention`), stanie na punkcie kontrolnym (`parked`) i kolejka
   * wysyłkowa (`toCarry`). Nowe pole opisujące żywy bieg dopisuje się do tej listy w tej samej
   * zmianie, w której powstaje — pilnuje tego `./nothing-live-survives-the-run.test.ts`,
   * porównując klucze widoku z dwiema wypisanymi listami, więc pole nienazwane na żadnej z nich
   * zapala kryterium, zanim ktoś napisze piąty przypis.
   *
   * Bieg, którego nie ma, nie stoi na niczyim pytaniu. Bez tego kontrolka „dalej" zostawałaby
   * po biegu zaparkowanym i odpowiedzianym, wołając `continue_run` w próżnię — a Rust podbija
   * wtedy licznik zgód i NASTĘPNY punkt kontrolny przelatuje bez pytania.
   *
   * ZOSTAJĄ DOKŁADNIE DWA POLA i oba są ZAPISEM, nie stanem: `history` i `answers`. To, co się
   * stało, zostaje do przeczytania — transkrypt biegu, który właśnie zszedł, jest jedyną rzeczą,
   * po którą człowiek na ten ekran wraca.
   */
  runEnded(): void;
  /**
   * Przełącza rozwinięcie JEDNEGO wiersza — to, co robi `+` przy zwiniętej linii.
   *
   * Jest w modelu, a nie w komponencie, z tego samego powodu, co reszta: stan rozwinięcia
   * jest polem wiersza, więc przycisk, który trzymałby go u siebie, byłby drugim miejscem
   * prawdy o tym samym (niezmiennik 13). Wiersz, którego nie ma, nie robi nic — kliknięcie
   * w wiersz wypchnięty z okna nie ma prawa przewrócić widoku.
   */
  toggle(rowId: number): void;
  /**
   * Powiadomienie o zmianie widoku; oddaje funkcję, która je odwołuje.
   *
   * Dokładnie tyle, ile bierze `useSyncExternalStore`, i ani pola więcej. Model jest żywy
   * dłużej niż ekran — bieg nie zatrzymuje się, kiedy człowiek wejdzie do Agentów — więc to
   * ekran subskrybuje model, nie model trzyma ekran.
   */
  subscribe(listener: () => void): () => void;
}

/** Ile linii wyjścia widać, kiedy niepowodzenie rozwinie się samo [T2 §7.3 reguła 3]. */
const OUTPUT_LINES = 20;

/**
 * Okno sklejania [T2 §7.3 reguła 4]. Liczone od PIERWSZEJ linii grupy.
 *
 * Od pierwszej, nie od ostatniej, i to jest cała różnica: okno liczone od ostatniej linii
 * przy równym strumieniu odczytów nie zamyka się nigdy, więc cały bieg schodzi do jednego
 * wiersza „Read 400 files" i widok przestaje mówić, co się kiedy stało.
 */
const WINDOW_MS = 2_000;

/**
 * Rodzaje, które wolno skleić — i etykieta z licznikiem dla każdego [T2 ryzyko 3].
 *
 * Zbiór jest wąski z jednego powodu: sklejamy wyłącznie to, co NIE niesie wyniku. `ran` niesie
 * `ok`, więc dwa `ran` w jednym wierszu chowają niepowodzenie za sukcesem sąsiada — czyli
 * dokładnie tę rzecz, której użytkownik w tym widoku szuka. Proza, pytania i struktura nie
 * sklejają się, bo reguła 2 każe je pokazywać, a wiersz „3 notes" nie jest prozą, tylko jej
 * brakiem.
 *
 * `read` liczy od JEDNEGO: `Read 6 files` jest jego postacią kanoniczną [T2 §7.2 wiersz 5],
 * więc wiersz stojący za jednym odczytem brzmi `Read 1 file`, a nie `Read src/parser.rs`.
 * Reszta przy jednej linii zostawia zdanie, które napisał mapper — `Edited src/parser.rs`
 * niesie ścieżkę, a `Edited 1 file` ją gubi i nie daje w zamian nic.
 */
const FOLDED: Partial<Record<Kind, (count: number) => string>> = {
  read: (count) => `Read ${count} ${count === 1 ? 'file' : 'files'}`,
  edit: (count) => `Edited ${count} files`,
  search: (count) => `Searched ${count} times`,
  memory: (count) => `Saved ${count} notes`,
};

/** Rodzaje, których etykieta liczy od jednego, a nie dopiero od dwóch. */
const COUNTS_FROM_ONE: ReadonlySet<Kind> = new Set<Kind>(['read']);

/**
 * Co robi w strefie TERAZ agent, który właśnie o coś zapytał.
 *
 * Nie treść pytania: pytanie ma JEDNO żywe miejsce — blok przyklejony z przyciskami — a wiersz
 * w strefie TERAZ odpowiada na inne pytanie („co robi ten agent"), więc powtórzenie tam tego
 * samego zdania daje dwa żywe regiony na jeden fakt, przy limicie 1 (niezmiennik 13). Zdanie
 * mówi też, gdzie ta decyzja czeka, zamiast zostawiać agenta w ostatniej czynności sprzed
 * pytania — a to jest ta wersja, która wygląda, jakby dalej pracował.
 */
const WAITING_ON_YOU = 'Waiting for your answer';

/** Rejestr jest stały na czas życia modułu — czytamy go raz, nie przy każdej linii. */
const REGISTRY = kinds();

/**
 * Klucze rejestru jako zbiór.
 *
 * `Set`, a nie `line.kind in REGISTRY`: `'constructor' in obiekt` jest prawdą, więc wiersz
 * z drutu o rodzaju `constructor` wjechałby do widoku jako rodzaj, którego nikt nigdy nie
 * zadeklarował. To ta sama pułapka, dla której `src/ipc/types.ts` trzyma kształty w `Map`.
 */
const KNOWN: ReadonlySet<string> = new Set(Object.keys(REGISTRY));

/** Otwarta grupa sklejania jednego agenta. */
interface Group {
  readonly kind: Kind;
  /** Gdzie w historii stoi wiersz grupy. */
  readonly index: number;
  /** Czas PIERWSZEJ linii grupy — od niego liczy się okno. */
  readonly startedAt: number;
}

/**
 * Czy to jest wiersz rodzaju, który to repo umie nazwać.
 *
 * Odpowiedź `false` znaczy „porzuć", nigdy „rzuć": vendorzy dokładają typy zdarzeń co tydzień
 * i po cichu, a wyjątek tutaj zabiera cały widok zamiast jednej linii (niezmiennik 5 w duchu).
 */
function known(line: Incoming): line is FeedLine {
  return KNOWN.has(line.kind);
}

/** Zdanie, które niesie ta linia. `thinking` nie niesie żadnego [T2 §7.2 wiersz 4]. */
function sentence(line: FeedLine): string {
  return 'text' in line ? line.text : '';
}

/**
 * Czy ten wiersz złożyło samo okno — czyli czy za nim NIE stoi niczyja praca.
 *
 * 2026-08-20 — PO CO TO ISTNIEJE, ZMIERZONE. Do dziś każda linia trasy `history` szła do mapy
 * `doing`, a ta mapa JEST strefą TERAZ. Po T-58 wiersz wejścia dopisuje do tej samej historii
 * echo wpisanej komendy i odpowiedź, którą daje sam sobie (`../entry/echo.ts`) — więc pierwszy
 * `/stop` przy niczym niebiegnącym stawiał w strefie „co się dzieje teraz" wpis
 * „Loadout — Nothing is running.", nieodróżnialny od pracującego agenta, i zostawiał go tam do
 * końca pracy. Agent, który nie pracuje, nie ma prawa stać w tej strefie (niezmiennik 17), a jest
 * to jeden z dwóch regionów, którym ARCHITECTURE §7 pozwala się ruszać — czyli dokładnie to
 * miejsce, w które człowiek patrzy, żeby wiedzieć, czy cokolwiek żyje.
 *
 * Pyta o POCHODZENIE wiersza, nigdy o to, jak nazywa się jego autor. Numer ujemny wydaje wyłącznie
 * `../entry/echo.ts` i wydaje go właśnie dlatego, że obie pompy — biegu i rozmowy — stemplują od 1
 * każda z osobna, więc dodatni licznik w oknie zderzyłby się z ich numerami. Lista zakazanych nazw
 * byłaby drugą tabelą prawdy o tym samym (niezmiennik 13) i myliłaby się w obie strony: skasowałaby
 * pierwszego agenta nazwanego „Loadout", a wiersz okna podpisany cudzą nazwą przepuściłaby jako
 * cytat agenta, który tego zdania nie wypowiedział.
 *
 * Odsiew jest TYLKO na strefie TERAZ. Do historii te wiersze wchodzą dalej i to jest cały sens
 * T-58: terminal, w którym wpisana komenda nie zostawia śladu, jest nieodróżnialny od terminala,
 * który tej komendy nie przyjął.
 *
 * Ta sama reguła stoi drugi raz w `../rail/roster.ts` (T-66), bo szyna agentów czyta historię, nie
 * tę mapę. Jedno wspólne miejsce na nią byłoby `../entry/echo.ts` — moduł, który te numery wydaje —
 * i jest poza blokiem OWNS tego zadania.
 */
function windowWrote(line: Incoming): boolean {
  return line.id < 0;
}

/** Numer dla panelu szczegółów; większość rodzajów nie ma czego pokazać pod kliknięciem. */
function detailOf(line: FeedLine): number | null {
  return 'detailId' in line ? line.detailId : null;
}

/**
 * Komenda, którą niesie ta linia — albo nic, bo niesie ją dokładnie jeden rodzaj.
 *
 * PO RODZAJU, nie po obecności pola: `'command' in line` przepuściłoby każdy przyszły rodzaj,
 * który akurat nazwie swoje pole tak samo, a o tym, czy proza jest propozycją, rozstrzygnął już
 * Rust w mapowaniu zdarzenie → linia (niezmiennik 15). Model przewozi tę odpowiedź, nie wydaje
 * jej po raz drugi.
 */
function commandOf(line: FeedLine): string | undefined {
  return line.kind === 'suggested' ? line.command : undefined;
}

/** Czy ta linia jest niepowodzeniem, które rozwija się samo [T2 §7.3 reguła 3]. */
function failed(line: FeedLine): boolean {
  return line.kind === 'ran' && !line.ok;
}

/**
 * Prawa kolumna wiersza — liczba, którą ta czynność zostawiła po sobie, albo nic.
 *
 * Zamknięta tabela dwóch rodzajów, nie gałąź `default`: piętnasty rodzaj dopisany po stronie
 * Rusta dostaje pustą metrykę, a nie zgadniętą. `edit` niesie `added`/`removed`, czyli fakt
 * z dysku; `ran`, które padło, niesie `preview` — pierwszą linię wyjścia, którą mapper po
 * tamtej stronie składa właśnie jako streszczenie w rodzaju `3 of 40` (`engine/line.rs`).
 *
 * `ran`, które się udało, NIE ma metryki: jego zdanie już mówi, że zadziałało, a liczba obok
 * niego byłaby drugim opisem tego samego. Sklejone odczyty też nie — ich licznik jest
 * w etykiecie (`Read 6 files`), a ta sama liczba dwa razy w jednym wierszu to dwa żywe
 * miejsca na jeden fakt (niezmiennik 13).
 */
function metricOf(line: FeedLine): string {
  if (line.kind === 'edit') {
    /* Znak minus U+2212, nie łącznik: makieta (`+42 −8`) i mapper po stronie Rusta piszą
     * właśnie tak, a łącznik przy liczbie czyta się jak przedział. */
    return '+' + String(line.added) + ' −' + String(line.removed);
  }
  if (line.kind === 'ran' && !line.ok) return line.preview;
  return '';
}

/** Etykieta wiersza stojącego za `count` liniami tego rodzaju. */
function labelFor(line: FeedLine, count: number): string {
  const folded = FOLDED[line.kind];
  if (folded === undefined) return sentence(line);
  if (count > 1 || COUNTS_FROM_ONE.has(line.kind)) return folded(count);
  return sentence(line);
}

/**
 * Świeży wiersz historii dla tej linii.
 *
 * 2026-08-23 — WYEKSPORTOWANA, bo pytających jest dwóch. Żywy strumień pyta o nią przez
 * [`Feed.appendLines`], które dokłada drugie sklejanie okna; historia biegu odczytana z dysku
 * (`../past/rows.ts`) pyta o JEDEN wiersz na JEDNĄ linię i sklejać go drugi raz nie ma prawa —
 * te linie skleił już kurator po stronie Rusta, w tym samym biegu, w którym powstały
 * (niezmiennik 15: kuracja mieszka w jednym miejscu). Druga funkcja składająca wiersz obok tej
 * pokazywałaby przy tej samej linii inną etykietę i inną metrykę, a nic na ekranie nie mówiłoby,
 * który z dwóch obrazów jest prawdziwy.
 */
export function rowFor(line: FeedLine): HistoryRow {
  const broke = failed(line);
  /* ILE CZYNNOŚCI STOI ZA TĄ LINIĄ — pytamy LINIĘ, a nie zakładamy jednej.
   *
   * 2026-08-23, zmierzone na `src/ipc/line-wire.golden.json`. Kurator po stronie Rusta skleja
   * sąsiednie odczyty w oknie 2 s i wysyła JEDEN wiersz z `count: 3` i tekstem `Read 3 files`.
   * Ten plik składał z niego wiersz `labelFor(line, 1)`, czyli `Read 1 file` — liczbę, której
   * nie ma w żadnym pliku i której nikt nie zmierzył (niezmiennik 17), na wierszu mówiącym
   * o trzech odczytach. Sklejanie okna zostaje bez zmian: linia z `count: 1` daje dokładnie tę
   * samą etykietę, co przed tą poprawką, więc `coalesce.test.ts` mierzy dalej to samo.
   */
  const behind = 'count' in line ? line.count : 1;
  return {
    id: line.id,
    /* PRZEWÓZ STEMPLA, nie odczyt zegara: czas zdarzenia jest tym, co powiedziała granica, a nie
     * tym, co pokazuje zegar w chwili rysowania. */
    at: line.at,
    kind: line.kind,
    agent: line.agent,
    label: labelFor(line, behind),
    count: behind,
    ids: [line.id],
    metric: metricOf(line),
    /* Niepowodzenie rozwija SIEBIE i nic poza sobą. Rozwinięcie całego strumienia po
     * pierwszym błędzie („tryb paniki") jest dokładnie tą ścianą tekstu, przed którą stoi
     * reguła 2 — i wygląda jak troska. */
    expanded: broke || REGISTRY[line.kind].expanded,
    /* OSTATNIE dwadzieścia linii, nie pierwsze: `slice(0, 20)` pokazuje początek logu, czyli
     * tę jego połowę, która nigdy nie zawiera powodu, i przechodzi każde sprawdzenie liczące
     * same wiersze. */
    output: broke && line.kind === 'ran' ? line.detail.slice(-OUTPUT_LINES) : [],
    /* PRZEWÓZ, nie decyzja: „czy ta proza ma ciało" rozstrzygnął Rust (`engine::line`,
     * `headline_and_body`), bo tam mieszka kuracja (niezmiennik 15). Okno, które liczyłoby to
     * samo po swojej stronie, byłoby drugim miejscem, w którym ta reguła żyje — i rozjechałoby
     * się z pierwszym przy pierwszej zmianie sufitu. */
    body: line.kind === 'note' ? line.body : [],
    detailId: detailOf(line),
    command: commandOf(line),
    /* Klucz jedzie TYLKO z linii, która go niesie: dopisanie `ended: undefined` do każdego
     * wiersza dałoby pole, którego znaczenie jest „nie wiem", tam gdzie nie ma o czym mówić. */
    ...(line.kind === 'done' ? { ended: line.ended } : {}),
  };
}

/** Ten sam wiersz, o jedną linię większy. Nowy obiekt: wiersz w historii jest niezmienny. */
function grown(row: HistoryRow, line: FeedLine): HistoryRow {
  const count = row.count + 1;
  return {
    ...row,
    count,
    /* Identyfikatory, nie sama liczba. Sklejanie, które nie umie pokazać, co skleiło,
     * jest po prostu gubieniem — a wygląda identycznie. */
    ids: [...row.ids, line.id],
    label: labelFor(line, count),
    /* Metryka sklejonego wiersza jest pusta, i to nie jest przeoczenie: `+42 −8` z ostatniej
     * z sześciu zmian opisywałoby jedną z nich w wierszu, który mówi o wszystkich. Liczba,
     * której nie umiemy zsumować uczciwie, nie ma prawa stać obok liczby, którą umiemy. */
    metric: '',
  };
}

/** Nowy, pusty model widoku pracy. */
export function createFeed(scroller: Scroller): Feed {
  /** Historia. Nowa tablica dokładnie wtedy, kiedy coś do niej weszło — i ani razu więcej. */
  let history: readonly HistoryRow[] = [];

  /**
   * Agent → co robi teraz. JEST strefą TERAZ, więc trzyma wyłącznie tych, którzy pracują.
   *
   * `Map`, bo kolejność wstawienia JEST kolejnością pojawienia się w biegu, a strefa TERAZ ma
   * mieć jeden wiersz na agenta. Wycinek historii (`lines.slice(-4)`) daje na zrzucie ekranu
   * to samo i pełznie o wiersz na każde zdarzenie.
   *
   * Rośnie na liniach, za którymi stoi praca ([`windowWrote`]), i schodzi CAŁA w chwili, w której
   * schodzi bieg (`runEnded`). Do 2026-08-20 była tylko dopisywana i nie czyszczona nigdy, więc
   * strefa TERAZ opisywała pracę, której nikt nie wykonywał, do końca pracy człowieka.
   */
  const doing = new Map<string, string>();

  /* Żywe kroki każdego agenta, po to i tylko po to, żeby wiedzieć, KIEDY skończył (T-162).
   *
   * Zbiór, nie licznik: ten sam `stepId` potrafi przyjść dwa razy (`running` po `ready`),
   * a licznik urósłby wtedy o dwa i agent nigdy by ze strefy nie wyszedł. Klucz jest parą
   * agent→kroki, bo jeden agent biegnie w kilku kopiach naraz i zakończenie PIERWSZEJ nie
   * znaczy, że przestał pracować — to jest ta różnica, którą sprawdza
   * `now-holds-only-live-work.test.ts`. */
  const liveSteps = new Map<string, Set<string>>();

  /** Nazwa agenta, którego slot `Thinking…` jest żywy. JEDNO pole, nigdy tablica. */
  let thinking: string | null = null;

  /**
   * Otwarta grupa per agent — klucz sklejania to para (agent, rodzaj), stąd mapa po agencie.
   *
   * Opisuje ŻYWY bieg, więc schodzi CAŁA razem z nim (`runEnded`) — dokładnie jak `doing`.
   * Grupa otwarta przez bieg, który zszedł, nie ma czego sklejać: doliczyłaby linię następnego
   * biegu do wiersza poprzedniego.
   */
  const groups = new Map<string, Group>();

  /**
   * Pytania bez odpowiedzi, najstarsze pierwsze. Przypięte jest zawsze to spod zera.
   *
   * Opisuje ŻYWY bieg, więc schodzi CAŁA razem z nim (`runEnded`) — dokładnie jak `doing`.
   * Pytanie bez biegu, który na nie czeka, nie jest pytaniem, tylko kartą z przyciskami
   * prowadzącymi donikąd.
   */
  let waiting: readonly Question[] = [];

  /**
   * Czy bieg stoi na punkcie kontrolnym — patrz `FeedView.parked`.
   *
   * Osobne od `waiting`, bo odpowiedź człowieka opróżnia kolejkę pytań i NIE puszcza biegu:
   * to `continue_run` go puszcza. Jedna zmienna na oba fakty jest dokładnie tym defektem,
   * który to pole zamyka — bieg parkował na zawsze, bo kontrolka „dalej" znikała razem
   * z odpowiedzią.
   */
  let parked = false;

  /** Zdanie czekające na przewiezienie do agenta — patrz `FeedView.toCarry`. */
  let toCarry = '';

  let answers: readonly Answer[] = [];

  /**
   * Migawka widoku.
   *
   * Świeży obiekt, ale `history` wchodzi do niego PRZEZ REFERENCJĘ — paczka samych `thinking`
   * ma zmienić strefę TERAZ i zostawić historię tą samą tablicą, co przed nią.
   */
  function snapshot(): FeedView {
    const rows: NowRow[] = [];
    for (const [agent, text] of doing) rows.push({ agent, text });
    const pinned = waiting[0] ?? null;
    return {
      history,
      now: { rows, thinking },
      pinned,
      parked,
      toCarry,
      /* Jeden fakt, jedno miejsce: „czyja kolej" wynika z przypięcia, więc nie da się ustawić
       * go osobno i rozjechać z nim (niezmiennik 13). */
      attention: pinned === null ? 'agents' : 'you',
      answers,
    };
  }

  let current: FeedView = snapshot();

  /** Kto czeka na wieść o zmianie. Pusty zbiór na serwerze i w każdym teście modelu. */
  const listeners = new Set<() => void>();

  /** Nowa migawka i jedno powiadomienie. Nigdy jedno bez drugiego. */
  function publish(): void {
    current = snapshot();
    for (const listener of listeners) listener();
  }

  function appendLines(batch: readonly Incoming[]): readonly HistoryRow[] {
    /* Kopia historii powstaje dopiero wtedy, kiedy naprawdę coś do niej wchodzi. Paczka
     * bez ani jednej linii historii ma zostawić tę samą tablicę. */
    let next: HistoryRow[] | null = null;
    const touched = new Set<number>();
    let changed = false;

    for (const incoming of batch) {
      if (!known(incoming)) continue;
      const line = incoming;
      changed = true;

      /* Czy za tym wierszem stoi czyjaś praca — patrz [`windowWrote`]. Rozstrzyga to o strefie
       * TERAZ i o niczym więcej: historia bierze wszystkie wiersze, także te z okna. */
      const atWork = !windowWrote(line);

      if (atWork && !doing.has(line.agent)) doing.set(line.agent, '');

      if (REGISTRY[line.kind].route === 'now') {
        /* Dwa rodzaje jadą do strefy TERAZ i odpowiadają na DWA różne pytania, więc nie wolno
         * ich obsłużyć jedną linią. `thinking` mówi „ktoś myśli" i ma tu swój slot. `stepState`
         * mówi, na czym stoi KROK — przestawia pasek loadoutu i chip na kafelku agenta,
         * i robi to w magazynie biegu (`src/state/run.ts`, `withStepStates`), bo tam mieszka
         * plan. Wersja, która zapaliłaby tym wierszem slot `Thinking…`, pokazywałaby myślącego
         * agenta za każdym razem, gdy krok się kończy. */
        if (line.kind === 'thinking') thinking = line.agent;
        if (line.kind === 'stepState') {
          /* Koniec kroku zdejmuje agenta ze strefy TERAZ dopiero wtedy, gdy nie została mu
           * ani jedna żywa kopia. Start kroku niczego nie zdejmuje — implementacja, która
           * reagowałaby na każdy `stepState`, trzymałaby strefę pustą przez cały bieg. */
          const mine = liveSteps.get(line.agent) ?? new Set<string>();
          if (stepIsOver(line.state)) {
            mine.delete(line.stepId);
            if (mine.size === 0) {
              doing.delete(line.agent);
              liveSteps.delete(line.agent);
            } else {
              liveSteps.set(line.agent, mine);
            }
          } else {
            mine.add(line.stepId);
            liveSteps.set(line.agent, mine);
          }
        }
        continue;
      }

      if (atWork) {
        /* Prawdziwa linia gasi slot [T2 §7.2 wiersz 4] — dowolna, nie tylko od tego agenta:
         * slot jest jeden, więc pytanie „czyj jest" ma dokładnie jedną odpowiedź. Echo własnego
         * Entera prawdziwą linią NIE jest: zgaszony tutaj slot mówiłby, że agent przestał myśleć,
         * bo człowiek wpisał ukośnik. Gasi go zdanie od agenta i nic poza nim. */
        thinking = null;
        doing.set(line.agent, line.kind === 'asked' ? WAITING_ON_YOU : sentence(line));
      }

      const rows = (next ??= [...history]);
      const group = groups.get(line.agent);
      const open =
        group !== undefined &&
        group.kind === line.kind &&
        FOLDED[line.kind] !== undefined &&
        line.at - group.startedAt <= WINDOW_MS;

      if (open && group !== undefined) {
        const row = rows[group.index];
        if (row !== undefined) {
          rows[group.index] = grown(row, line);
          touched.add(group.index);
          continue;
        }
      }

      rows.push(rowFor(line));
      const index = rows.length - 1;
      groups.set(line.agent, { kind: line.kind, index, startedAt: line.at });
      touched.add(index);

      if (line.kind === 'asked') {
        /* Kolejka, nie „ostatnie pytanie": bieg stoi na NAJSTARSZYM nieodpowiedzianym,
         * a odpowiedź na młodsze nie ma prawa go zdjąć. */
        waiting = [
          ...waiting,
          { id: line.id, text: line.text, options: [...line.options], agent: line.agent },
        ];
        /* Pytanie agenta zatrzymuje CAŁY bieg, nie sam krok (`commands::run::wait_for_a_person`),
         * więc ta linia jest zarazem jedyną wiadomością „stoimy", jaką okno dostaje. */
        parked = true;
      }
    }

    let shift = 0;
    if (next !== null) {
      /* Ten sam sufit, co na linie w magazynie (`LINE_LIMIT`): wiersz stoi za co najmniej
       * jedną linią, więc okno historii nie może być szersze niż okno, z którego powstaje.
       * Pamięć jest oknem, prawdą są pliki (niezmiennik 4) — ile wypadło, wie magazyn. */
      shift = Math.max(0, next.length - LINE_LIMIT);
      if (shift > 0) {
        next.splice(0, shift);
        for (const [agent, group] of groups) {
          const index = group.index - shift;
          /* Grupa, której wiersz wypadł z okna, jest zamknięta: nie ma już czego doliczyć. */
          if (index < 0) groups.delete(agent);
          else groups.set(agent, { ...group, index });
        }
      }
      history = next;
    }

    if (changed) publish();

    const entered: HistoryRow[] = [];
    for (const index of [...touched].sort((a, b) => a - b)) {
      const row = history[index - shift];
      if (row !== undefined) entered.push(row);
    }
    return entered;
  }

  function jumpToNewest(): void {
    /* Zero, nie `scrollHeight`: historia rysuje się w `column-reverse`, więc najnowsza linia
     * siedzi pod `scrollTop === 0`. To jedyne wywołanie portu w całym modelu i ma swój
     * przycisk — bez przycisku byłoby zwykłym samoprzewijaniem z lepszą nazwą. */
    scroller.scrollTo(0);
  }

  function answer(questionId: number, option: string): void {
    waiting = waiting.filter((question) => question.id !== questionId);
    /* `who: 'you'` — trzy autorytety w całej aplikacji, nie osiem [FOUNDATIONS §2.2]. */
    answers = [...answers, { questionId, option, who: 'you' }];
    /* NADPISUJE, nie dokleja: agent stoi na JEDNYM pytaniu i dostanie JEDNO zdanie. Kolejka
     * zbierająca odpowiedzi wysłałaby przy drugim punkcie kontrolnym wszystkie poprzednie
     * jeszcze raz — a to jest ta klasa błędu, która wygląda jak agent, który nie słucha. */
    toCarry = option;
    publish();
  }

  /**
   * Bieg został puszczony dalej i IDZIE: gasi `parked` i kolejkę wysyłkową.
   *
   * 2026-08-20 — DLACZEGO TO NIE JEST JEDNO CIAŁO Z `runEnded`. Do dziś obie nazwy z interfejsu
   * wskazywały jedną funkcję (`unpark`), bo obie chwile gaszą to samo jedno pole. Ten kształt
   * przemilczał różnicę, która jest dla strefy TERAZ całą treścią: bieg puszczony PRACUJE dalej,
   * więc strefy dotknąć nie wolno, a bieg, którego nie ma, nie ma nikogo pracującego. Alias
   * dziedziczy zachowanie w obie strony i tak właśnie ta wada przeżyła — poprawka dopisana do
   * wspólnego ciała opróżniałaby strefę TERAZ w środku biegu stojącego na punkcie kontrolnym.
   */
  function carriedOn(): void {
    /* Warunek liczy OBA pola: bieg puszczony bez odpowiedzi ma wyczyścić kolejkę wysyłkową tak
     * samo jak bieg puszczony z odpowiedzią, a `if (!parked) return` zostawiłoby zdanie, które
     * pojechałoby do NASTĘPNEGO pytania. */
    if (!parked && toCarry === '') return;
    parked = false;
    toCarry = '';
    publish();
  }

  /**
   * Bieg zszedł — koniec, odmowa albo zatrzymanie. Gasi KAŻDE pole, które opisywało żywy bieg.
   *
   * 2026-08-20 — ZMIERZONA WADA, KTÓRĄ TA FUNKCJA ZAMYKA. Mapa `doing` była tylko dopisywana,
   * więc po zejściu biegu ostatnie zdanie każdego agenta stało w strefie „co się dzieje teraz"
   * do końca pracy — cztery wiersze o pracy, której nikt nie wykonuje, w jednym z dwóch regionów,
   * którym ARCHITECTURE §7 pozwala się ruszać (niezmiennik 17). Człowiek patrzy w to miejsce
   * właśnie po to, żeby wiedzieć, czy cokolwiek żyje.
   *
   * Bez wyłączania strefy: opróżnia ją TA JEDNA chwila, w której bieg schodzi. Wersja czyszcząca
   * `doing` przy każdej paczce daje pustą strefę równie skutecznie i zostawia w niej to, co
   * przyszło ostatnią paczką — czyli odpowiada na pytanie „kto powiedział coś ostatni" zamiast
   * „kto pracuje", a przy czterech agentach naraz to są dwa różne zdania w każdej chwili biegu.
   *
   * HISTORII NIE TYKA, i zostaje ona TĄ SAMĄ tablicą (`snapshot` bierze ją przez referencję):
   * koniec biegu kasuje strefę STANU, nigdy zapisu tego, co się stało. Świeża tablica prosiłaby
   * Reacta o przerysowanie całego transkryptu za coś, co do niego nie weszło. Z tego samego
   * powodu nie wolno naprawiać tej rodziny wad przez zbudowanie modelu od nowa: `createFeed()`
   * opróżnia całą listę jedną linią i zabiera transkrypt razem z nią.
   */
  function runEnded(): void {
    doing.clear();
    /* Otwarte grupy sklejania też opisują ŻYWY bieg, a `FeedView` ich nie pokazuje — więc lista
     * pól wypisana w kryterium ich nie widzi i mapa przeżywała bieg razem z całą sesją folderu
     * (`feedFor` oddaje jedną `Feed` na workspace na zawsze). Pierwsza linia następnego biegu
     * mieszcząca się w oknie sklejania doliczała się wtedy do wiersza POPRZEDNIEGO biegu:
     * dwa biegi w jednym wierszu historii, czyli relacja, której w danych nie ma
     * (niezmiennik 17). Zamknięcie CAŁEJ mapy, nie wybranych wpisów: dokładnie jak `doing`. */
    groups.clear();
    /* Slot gaśnie razem z mapą: „Thinking…" po biegu jest zdaniem o procesie, który nie istnieje,
     * i jest ostatnią rzeczą na tym ekranie, którą człowiek by podważył. */
    thinking = null;
    /* 2026-08-20 — CZWARTY RAZ TEN SAM KSZTAŁT, I DLATEGO KOLEJKA PYTAŃ STOI TERAZ W TEJ LIŚCIE.
     * Kolejka przeżywała bieg, który ją napełnił, więc pytanie, na które człowiek nie zdążył
     * odpowiedzieć przed Stopem albo przed błędem, zostawało przypięte: `pinned` pełne,
     * `attention` na `you`, a karta „Needs your answer" wisiała z kompletem kontrolek wołających
     * `answer()` dla agenta, który nie pracuje — kontrolka bez roboty (niezmiennik 16) przypięta
     * do relacji, której w danych już nie ma (niezmiennik 17).
     *
     * GAŚNIE TUTAJ, A NIE WARUNKIEM W `./feed.tsx`. Karta wisi na samym `pinned`, więc drugi
     * warunek („rysuj, jeśli przypięte ORAZ bieg żyje") byłby drugim miejscem, w którym mieszka
     * odpowiedź na pytanie „czy cokolwiek żyje", i rozjechałby się z tym pierwszym po cichu
     * (niezmiennik 13). Kuracja mieszka w modelu, nie w widoku (niezmiennik 15).
     *
     * PYTANIA ZNIKAJĄ, NIE SĄ ODPOWIADANE. Domknięcie ich przez `answer()` dopisałoby do
     * `answers` zdanie, którego człowiek nie powiedział, a `answers` jest jego zapisem i zostaje
     * na zawsze. Że agent zapytał, się wydarzyło — i to zostaje: wiersz `asked` stoi w historii. */
    waiting = [];
    /* Dwie rzeczy, które ta chwila gasiła zawsze — powód stoi przy `carriedOn`. Opróżnienie
     * strefy TERAZ nie ma prawa ich kosztować. */
    parked = false;
    toCarry = '';
    publish();
  }

  function toggle(rowId: number): void {
    const index = history.findIndex((row) => row.id === rowId);
    const row = index < 0 ? undefined : history[index];
    if (row === undefined) return;
    const rows = [...history];
    rows[index] = { ...row, expanded: !row.expanded };
    history = rows;
    publish();
  }

  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  return {
    get view(): FeedView {
      return current;
    },
    appendLines,
    jumpToNewest,
    answer,
    carriedOn,
    runEnded,
    toggle,
    subscribe,
  };
}
