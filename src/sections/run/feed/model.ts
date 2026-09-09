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
/* WYŁĄCZNIE TYP, i to jest warunek, pod którym ten import w ogóle wolno tu postawić: `../io`
 * niesie transport Tauri, a ten plik jest czystym modułem, który montują kryteria bez atrapy
 * granicy. `verbatimModuleSyntax` gwarantuje, że `import type` znika przy budowaniu. */
import type { Interrupted } from '../io';
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
  readonly source?: import('../../../ipc/types').RunSource;
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
  readonly question?: import('../../../ipc/types').QuestionAddress;
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
  /**
   * Zdanie o wiadomości, która CZEKA za komendą idącą w tej chwili — albo `null`.
   *
   * 2026-09 (Z-36) — PO CO TO POLE, ZMIERZONE. Lider siedział siedem minut w jednym wywołaniu
   * Basha; człowiek wysłał w tym czasie wiadomość, plik tury zapisał ją jako dostarczoną, a ekran
   * nie powiedział o tym ani słowa. Zgłoszenie brzmiało „lider się zawiesza i nie odpisuje".
   * Wiadomość dostarczona i nieczytana wygląda dokładnie tak samo jak wiadomość zgubiona —
   * jedyną różnicą jest zdanie, którego nie było.
   *
   * JEDNO POLE, NIE LISTA: żywy region na ten fakt jest jeden (niezmiennik 13), a odpowiedź
   * „na co czekamy" ma sens dokładnie wtedy, gdy stoi za nią JEDNA komenda w toku. Gaśnie
   * w chwili, w której ta komenda się domyka — czyli wtedy, kiedy tura naprawdę może ruszyć —
   * i razem z całą strefą żywą, kiedy bieg schodzi.
   */
  readonly queued: string | null;
  /**
   * Kontrolka „Interrupt" nad turą, która ciągnie się za długo — albo `null`.
   *
   * 2026-09 (Z-40) — PO CO TO POLE. Po Z-36 człowiek WIDZI, że lider siedzi siódmą minutę
   * w jednym wywołaniu Basha, i nadal może tylko czekać albo zamknąć rozmowę razem z całym jej
   * kontekstem. Droga przerwania po stronie Rusta istniała od pierwszego dnia i nie miała
   * wołającego z okna.
   *
   * PO PROGACH, NIE ZAWSZE, i to jest cała treść typu `null`: przycisk stojący nad każdą turą
   * proponuje zatrzymanie pracy, która idzie normalnie — a kontrolka bez roboty nie zostaje na
   * ekranie (niezmiennik 16). Progi są dwa, bo długo czekać da się na dwa różne sposoby:
   * [`AFTER_TOOL`] nad jedną komendą i [`AFTER_TURN`] nad turą, która nie zapowiedziała żadnej.
   *
   * Gaśnie razem z całą strefą żywą, kiedy bieg schodzi (`runEnded`).
   */
  readonly interrupt: InterruptOffer | null;
  readonly attention: Attention;
  readonly answers: readonly Answer[];
}

/** Co ekran ma postawić nad wierszem wejścia, kiedy tura ciągnie się za długo. */
export interface InterruptOffer {
  /**
   * Komenda, w której lider stoi — albo `null`, kiedy tura idzie bez ani jednej.
   *
   * Nie jest tekstem przycisku: nazwa kontrolki brzmi „Interrupt" i tyle. Jest tym, co ta
   * kontrolka mówi o sobie czytnikowi ekranu, żeby „Interrupt" nad strumieniem sześciu agentów
   * nie było pytaniem „co dokładnie".
   */
  readonly subject: string | null;
  /**
   * Zdanie, które stoi ZAMIAST przycisku, kiedy przerwać się nie da — albo `null`.
   *
   * Powstaje z odpowiedzi Rusta ([`Feed.interruptAnswered`]), nigdy z sondowania granicy przed
   * naciśnięciem: zdolności `interrupt_receipt_v1` nie da się poznać przed startem sesji, bo
   * lista przychodzi dopiero w `system/init`. Odpowiedź staje więc w miejscu przycisku i gasi
   * go na tę rozmowę — to jest „mówi wprost", a nie kolejne pytanie o granicę.
   */
  readonly refusal: string | null;
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
   * Bije zegar okna: przelicza `view.interrupt` bez ani jednego zdarzenia z drutu.
   *
   * 2026-09 (Z-40) — PO CO MODEL POTRZEBUJE ZEGARA Z ZEWNĄTRZ. Próg [`AFTER_TOOL`] liczy się
   * z czasu, który przysyła Rust w wierszu komendy, więc rośnie sam przy każdym biciu serca.
   * Próg [`AFTER_TURN`] mierzy ciszę — a cisza z definicji nie przysyła zdarzeń. Bez tego
   * wywołania tura, która myśli czwartą minutę i nie zapowiedziała ani jednej komendy, nie
   * dostałaby kontrolki nigdy.
   *
   * ZEGAR JEST ARGUMENTEM, nie ścianą: model z własnym `Date.now()` nie da się sprawdzić bez
   * czekania, a test ze `sleep` mierzy planistę przeglądarki. Ta sama reguła stoi po drugiej
   * stronie granicy, w `engine::line::Seen`.
   *
   * BUDZI EKRAN TYLKO PRZY ZMIANIE. Wołane raz na sekundę, a publikacja przy każdym tyknięciu
   * kazałaby Reactowi przerysować strumień co sekundę przez cały bieg.
   */
  tick(now: number): void;
  /**
   * Rust odpowiedział na naciśnięty „Interrupt".
   *
   * Odpowiedź staje w miejscu przycisku i zostaje tam do końca rozmowy: „to CLI tego nie umie"
   * jest faktem o sesji, a nie o tej jednej komendzie, więc przycisk odrastający przy następnej
   * obiecywałby to samo drugi raz.
   */
  interruptAnswered(said: Interrupted): void;
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
  /* `=== false`, nie `!ok`: od 2026-09 (Z-36) `null` znaczy „komenda właśnie idzie", a `!null`
   * jest prawdą — wiersz w toku malowałby się wtedy jak porażka i rozwijał sam, pokazując
   * pustkę, której nikt jeszcze nie wypisał. */
  return line.kind === 'ran' && line.ok === false;
}

/**
 * Wywołanie, którego wiersz ta linia PRZEPISUJE — albo `null`, kiedy nie przepisuje żadnego.
 *
 * 2026-09 (Z-36) — komenda dostaje wiersz w chwili, w której rusza, a każde bicie serca i wynik
 * są tą samą linią z nowym czasem (`engine::line`, `Line::Ran::call_id`). Bez tego klucza każda
 * aktualizacja byłaby wierszem OBOK, bo numer linii bije okno przy odbiorze paczki (`../io.ts`)
 * — czyli siedmiominutowa komenda zostawiałaby w strumieniu piętnaście wierszy o sobie.
 *
 * Pusty napis nie jest kluczem: znaczy „vendor nie nazwał tego wywołania", a wspólny pusty klucz
 * skleiłby w jeden wiersz dwie różne komendy.
 */
function carrierOf(line: FeedLine): string | null {
  if (line.kind !== 'ran' || line.callId === '') return null;
  return line.callId;
}

/** Komenda, która w tej chwili idzie — tyle, ile trzeba, żeby powiedzieć, na co się czeka. */
interface InFlight {
  readonly agent: string;
  readonly subject: string;
  readonly elapsed: number;
}

/**
 * `0s`, `42s`, `4m`, `7m 30s` — ta sama drabinka, którą wiersz dostaje z Rusta.
 *
 * BLIŹNIAK `engine::line::for_how_long`, i to jest świadomy koszt, nie przeoczenie. Zdanie
 * o czekaniu powstaje TUTAJ, bo mówi o stanie okna („wiadomość stoi w kolejce"), którego Rust
 * nie zna — a dwa różne zapisy jednej liczby na jednym ekranie („7m 30s" w wierszu i „450s"
 * pod nim) są dokładnie tym rozjazdem, który każe człowiekowi sprawdzać, który z nich jest
 * prawdziwy. Kształt jest więc przepisany co do znaku i tak ma zostać.
 *
 * WYEKSPORTOWANY, żeby sceny testowe (`./fixtures/lines.ts`) nie zapisywały tego samego czasu
 * trzecim sposobem: wiersz w scenie ma czytać się dokładnie tak, jak czyta się w produkcie.
 */
export function forHowLong(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return String(seconds) + 's';
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (rest === 0) return String(minutes) + 'm';
  return String(minutes) + 'm ' + String(rest) + 's';
}

/**
 * Zdanie o wiadomości, która czeka za tą komendą.
 *
 * NAZYWA KOMENDĘ I CZAS, bo to są dwie rzeczy, których człowiek w tej chwili nie wie: czy jego
 * zdanie w ogóle doszło i dlaczego nikt na nie nie odpowiada. „Please wait" bez nich jest
 * kółkiem kręcącym się nad ciszą.
 */
function queuedSays(one: InFlight): string {
  return 'Queued — the lead is still running ' + one.subject + ' (' + forHowLong(one.elapsed) + ')';
}

/**
 * Ile JEDNA komenda ma iść, zanim „Interrupt" stanie nad wierszem wejścia.
 *
 * Minuta, i to jest wybór z zapisaną ceną. W dół: kontrolka nad każdym `npm test` proponuje
 * zatrzymanie pracy, która idzie normalnie, a przycisk, który zwykle jest pomyłką, przestaje być
 * czytany. W górę: zgłoszenie, z którego wzięło się to zadanie, mówi o siedmiu minutach — a każda
 * minuta czekania nad komendą, o której już wiadomo, że wisi, jest minutą płaconą u dostawcy.
 */
const AFTER_TOOL = 60_000;

/**
 * Ile ma iść CAŁA tura bez ani jednej komendy, zanim stanie ta sama kontrolka.
 *
 * Dwa razy tyle, bo cisza mówi mniej: nad komendą widać, na co się czeka, a nad myśleniem widać
 * wyłącznie to, że nic nie widać. Przy tym progu tura, która po prostu jest długa, zdąży odpisać.
 */
const AFTER_TURN = 120_000;

/**
 * Podpis, którym LIDER stoi w strumieniu — `commands::chat::LEAD` po tamtej stronie granicy.
 *
 * 2026-09 (Z-40) — DRUGA KOPIA TEGO NAPISU I JEST TO ZAPISANY DŁUG, nie przeoczenie. Kontrolka
 * „Interrupt" prowadzi do rozmowy z liderem (`interrupt_the_lead`), a `Line::Told` wystawiają
 * DWIE strony: rozmowa (`commands/chat.rs`, podpis `Lead`) i zdanie zaadresowane do pracującego
 * kroku (`commands/run.rs`, podpis nazwą kroku). Bez tego warunku ta sama kontrolka stanęłaby nad
 * krokiem i przerwałaby cudzą turę — czyli robiłaby coś innego, niż mówi (niezmiennik 17).
 *
 * Napisu nie da się dziś przywieźć drutem: wiersz niesie podpis, a nie odpowiedź na pytanie „czy
 * to lider". Ten sam dług nazywa `../entry/echo.ts` przy `LOADOUT` i z tego samego powodu.
 */
const LEAD = 'Lead';

/**
 * Jak nazywa się aplikacja agenta, o której mówi odmowa.
 *
 * Tabela, a nie napis z drutu, bo po tamtej stronie stoi klucz vendora (`claude`, `codex`), a na
 * ekranie ma stać nazwa produktu (decyzja D5: zdanie mieszka w oknie). Klucz, którego ta tabela
 * nie zna, idzie na ekran taki, jaki przyjechał — zgadnięta nazwa byłaby gorsza od surowej.
 */
const AGENT_APPS: Readonly<Record<string, string>> = {
  claude: 'Claude',
  codex: 'Codex',
};

/**
 * Zdanie, które staje ZAMIAST przycisku — albo `null`, kiedy prośba pojechała.
 *
 * `sent` nie dostaje zdania z rozmysłem: o tym, co się stało, powie strumień, wierszem
 * `Interrupted — … stopped after 7m`, który składa Rust w miejscu, gdzie mieszka kuracja
 * (niezmiennik 15). Drugie zdanie o tym samym nad wierszem wejścia byłoby drugim żywym regionem
 * na jeden fakt (niezmiennik 13).
 */
function interruptRefusal(said: Interrupted): string | null {
  if (said.answer === 'sent') return null;
  if (said.answer === 'noLongerListening') {
    return 'Nothing to interrupt — this conversation has already ended';
  }
  const app = AGENT_APPS[said.agentApp] ?? said.agentApp;
  return (
    'This ' +
    (app === '' ? 'agent app' : app) +
    " can't be interrupted — stop the conversation instead"
  );
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
  /* `=== false` z tego samego powodu, co w [`failed`]: komenda, która właśnie idzie, nie ma
   * jeszcze wyjścia, a metryka wzięta z pustego podglądu byłaby pustą kolumną w wierszu, który
   * przepisuje się co trzydzieści sekund. */
  if (line.kind === 'ran' && line.ok === false) return line.preview;
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
    body: line.kind === 'note' ? line.body : line.kind === 'messageStored' ? [line.body] : [],
    detailId: detailOf(line),
    command: commandOf(line),
    ...(line.kind === 'runSource'
      ? {
          source: {
            workspace: line.workspace,
            runId: line.runId,
            runFolder: line.runFolder,
            observedAt: line.observedAt,
          },
        }
      : {}),
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
   * Wywołanie → gdzie w historii stoi jego wiersz. Jedna komenda, jeden nośnik (2026-09, Z-36).
   *
   * Opisuje ŻYWY bieg, więc schodzi CAŁA razem z nim (`runEnded`) — dokładnie jak `groups`,
   * i z tego samego powodu: wywołanie o tym samym identyfikatorze w następnym biegu
   * przepisywałoby wiersz biegu poprzedniego.
   */
  const carriers = new Map<string, number>();

  /**
   * Komendy, które w tej chwili idą, po identyfikatorze wywołania.
   *
   * Osobna od `carriers`, bo odpowiada na inne pytanie: tamta wie, GDZIE stoi wiersz, ta wie,
   * CO ta komenda robi i jak długo. Wpis znika w chwili, w której komenda się domyka.
   */
  const inFlight = new Map<string, InFlight>();

  /** Wywołanie, za którym stoi w kolejce zdanie człowieka — patrz `FeedView.queued`. */
  let queuedBehind: string | null = null;

  /**
   * Kiedy ruszyła tura lidera, na którą ktoś czeka — albo `null`, kiedy żadna nie idzie.
   *
   * Stawia ją wiersz `told` podpisany liderem (to jest chwila, w której człowiek nacisnął Enter),
   * gasi ją wiersz `done` tego samego podpisu. Bez tej pary próg [`AFTER_TURN`] nie miałby od
   * czego liczyć, a kontrolka stałaby nad turą, która skończyła się minutę temu.
   */
  let turnStartedAt: number | null = null;

  /** Ostatnia chwila podana przez okno (`Feed.tick`). Zero, dopóki nikt nie zapytał. */
  let clock = 0;

  /** Zdanie odmowy przerwania — patrz `InterruptOffer.refusal`. */
  let refusal: string | null = null;

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
  /**
   * Czy „Interrupt" ma teraz stać nad wierszem wejścia — i co ta kontrolka o sobie mówi.
   *
   * DWA PROGI, JEDNA ODPOWIEDŹ. Komenda w toku ma własny zegar, przysyłany przez Rusta co
   * trzydzieści sekund, więc nad nią próg mija bez pytania okna o czas. Tura bez ani jednej
   * komendy nie przysyła niczego — i to jest jedyna rzecz, do której model potrzebuje `clock`
   * (patrz `Feed.tick`).
   */
  function interruptOffer(): InterruptOffer | null {
    if (turnStartedAt === null) return null;
    for (const one of inFlight.values()) {
      /* TYLKO KOMENDA LIDERA: kontrolka prowadzi do jego rozmowy, a strumień wiezie obok
       * komendy kroków biegu (powód w całości przy [`LEAD`]). */
      if (one.agent === LEAD && one.elapsed >= AFTER_TOOL) {
        return { subject: one.subject, refusal };
      }
    }
    if (clock - turnStartedAt >= AFTER_TURN) {
      /* Bez podmiotu, bo go nie ma: tura, która nie zapowiedziała ani jednej komendy, nie daje
       * się nazwać niczym, czego ktoś by nie wymyślił (niezmiennik 17). */
      return { subject: null, refusal };
    }
    return null;
  }

  function snapshot(): FeedView {
    const rows: NowRow[] = [];
    for (const [agent, text] of doing) rows.push({ agent, text });
    const pinned = waiting[0] ?? null;
    /* Zdanie składamy z KOMENDY, która stoi teraz, a nie z tej, która stała w chwili wysłania:
     * czas w nim rośnie razem z wierszem, bo to jest ten sam fakt widziany dwa razy. */
    const holding = queuedBehind === null ? undefined : inFlight.get(queuedBehind);
    return {
      history,
      now: { rows, thinking },
      pinned,
      parked,
      toCarry,
      queued: holding === undefined ? null : queuedSays(holding),
      interrupt: interruptOffer(),
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

      // `stepCarriedOn` jest pełnym faktem schedulera o zakończonym kroku, nie nową pracą
      // agenta. Sam zdejmuje go ze strefy TERAZ niżej; dodanie tutaj zostawiałoby go jako
      // pracującego już po końcu kroku.
      if (line.kind === 'stepSession' || line.kind === 'runProgress') continue;
      if (line.kind === 'questionAnswered') {
        const question = waiting.find(
          (one) =>
            one.question?.runId === line.runId &&
            one.question.checkpointId === line.checkpointId &&
            one.question.questionId === line.checkpointId,
        );
        if (question !== undefined) recordAnswer(question.id, line.answer);
        continue;
      }
      if (atWork && line.kind !== 'stepCarriedOn' && !doing.has(line.agent)) {
        doing.set(line.agent, '');
      }

      if (REGISTRY[line.kind].route === 'now') {
        /* Trzy rodzaje jadą trasą TERAZ i odpowiadają na trzy różne pytania. `thinking`
         * ma swój slot. `stepState` i `stepCarriedOn` są faktami kroku konsumowanymi przez
         * magazyn biegu; model strumienia nie rysuje ich i nie zamienia w bieżącą pracę. */
        if (line.kind === 'thinking') thinking = line.agent;
        if (line.kind === 'stepState' || line.kind === 'stepCarriedOn') {
          /* Koniec kroku zdejmuje agenta ze strefy TERAZ dopiero wtedy, gdy nie została mu
           * ani jedna żywa kopia. `stepCarriedOn` jest samowystarczalnym terminalnym wynikiem
           * `failed`; nie czeka na osobną linię stanu, bo stratna kolejka mogłaby ją zgubić.
           * Start kroku niczego nie zdejmuje — implementacja, która reagowałaby na każdy
           * `stepState`, trzymałaby strefę pustą przez cały bieg. */
          const mine = liveSteps.get(line.agent) ?? new Set<string>();
          if (line.kind === 'stepCarriedOn' || stepIsOver(line.state)) {
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

      /* KOMENDA, KTÓRA IDZIE, I ZDANIE, KTÓRE ZA NIĄ CZEKA (2026-09, Z-36). Oba fakty czytamy
       * z tej samej linii i przed wierszami, bo paczka bywa całą sceną: wiersz zamykający
       * komendę i zdanie człowieka potrafią przyjść razem. */
      const carrier = carrierOf(line);
      if (carrier !== null && line.kind === 'ran') {
        if (line.ok === null) {
          inFlight.set(carrier, {
            agent: line.agent,
            subject: line.subject,
            elapsed: line.elapsed,
          });
        } else {
          inFlight.delete(carrier);
          /* Komenda się domknęła, więc tura może ruszyć — a zdanie o czekaniu opisywałoby
           * od tej chwili czekanie, którego nie ma (niezmiennik 17). */
          if (queuedBehind === carrier) queuedBehind = null;
        }
      }
      if (line.kind === 'told') {
        /* Wiadomość CZEKA tylko wtedy, gdy ten, do kogo poszła, stoi w komendzie. Zdanie
         * postawione bez tego warunku mówiłoby „queued" nad agentem, który właśnie ją czyta. */
        for (const [call, one] of inFlight) {
          if (one.agent === line.agent) queuedBehind = call;
        }
        /* TURA LIDERA ZACZYNA SIĘ TUTAJ (2026-09, Z-40). Ten wiersz powstaje w chwili, w której
         * zdanie człowieka naprawdę doszło do agenta (`commands::chat`, `say_to_stream`), więc
         * jest jedynym stemplem początku tury, jaki okno dostaje. */
        if (line.agent === LEAD) turnStartedAt = line.at;
      }
      if (line.kind === 'done' && line.agent === LEAD) {
        /* …i kończy się tutaj, na wierszu zamykającym turę. Kontrolka „Interrupt" nad turą,
         * która właśnie odpisała, jest kontrolką bez roboty (niezmiennik 16). */
        turnStartedAt = null;
      }

      const rows = (next ??= [...history]);

      /* TEN SAM NOŚNIK, TEN SAM WIERSZ. Aktualizacja komendy PODMIENIA swój wiersz w miejscu
       * i zachowuje jego numer oraz stempel — numer, bo to jest klucz Reacta i klucz rozwinięcia,
       * a stempel, bo jest chwilą, w której ta komenda RUSZYŁA. Wiersz dopisany obok byłby
       * piętnastoma wierszami o siedmiominutowej komendzie.
       *
       * `groups` zostaje NIETKNIĘTA, i to jest treść: okno sklejania otwarte przed tą komendą
       * (na przykład na odczytach tego agenta) ma rosnąć dalej, a przestawione tutaj kazałoby
       * następnemu odczytowi otworzyć wiersz obok. */
      const standing = carrier === null ? undefined : carriers.get(carrier);
      const before = standing === undefined ? undefined : rows[standing];
      if (standing !== undefined && before !== undefined) {
        rows[standing] = { ...rowFor(line), id: before.id, at: before.at, ids: before.ids };
        touched.add(standing);
        continue;
      }

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
      if (carrier !== null) carriers.set(carrier, rows.length - 1);
      const index = rows.length - 1;
      groups.set(line.agent, { kind: line.kind, index, startedAt: line.at });
      touched.add(index);

      if (line.kind === 'asked') {
        /* Kolejka, nie „ostatnie pytanie": bieg stoi na NAJSTARSZYM nieodpowiedzianym,
         * a odpowiedź na młodsze nie ma prawa go zdjąć. */
        waiting = [
          ...waiting,
          {
            id: line.id,
            text: line.text,
            options: [...line.options],
            agent: line.agent,
            ...(line.question === undefined ? {} : { question: line.question }),
          },
        ];
        /* Pytanie agenta zatrzymuje CAŁY bieg, nie sam krok (`commands::run::wait_for_a_person`),
         * więc ta linia jest zarazem jedyną wiadomością „stoimy", jaką okno dostaje. */
        if (line.question === undefined) parked = true;
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
        /* Ta sama poprawka dla nośników komend, i z tego samego powodu: pozycja niezaktualizowana
         * po przycięciu głowy wskazuje CUDZY wiersz, więc następne bicie serca przepisałoby
         * czyjąś linię swoim zdaniem (2026-09, Z-36). */
        for (const [call, index] of carriers) {
          const moved = index - shift;
          if (moved < 0) carriers.delete(call);
          else carriers.set(call, moved);
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
    recordAnswer(questionId, option);
    publish();
  }

  function recordAnswer(questionId: number, option: string): void {
    const found = waiting.find((question) => question.id === questionId);
    // IPC reply and the run event can arrive in either order. One accepted answer is one row.
    if (found === undefined) return;
    const bound = found.question;
    waiting = waiting.filter((question) => question.id !== questionId);
    /* `who: 'you'` — trzy autorytety w całej aplikacji, nie osiem [FOUNDATIONS §2.2]. */
    answers = [...answers, { questionId, option, who: 'you' }];
    /* 2026-09 (Z-26): odpowiedź starsza niż okno historii nie ma już wiersza pytania, pod
     * którym ekran mógłby ją pokazać. Tniemy głowę, żeby najnowsza odpowiedź zawsze została. */
    answers = answers.slice(-LINE_LIMIT);
    /* NADPISUJE, nie dokleja: agent stoi na JEDNYM pytaniu i dostanie JEDNO zdanie. Kolejka
     * zbierająca odpowiedzi wysłałaby przy drugim punkcie kontrolnym wszystkie poprzednie
     * jeszcze raz — a to jest ta klasa błędu, która wygląda jak agent, który nie słucha. */
    // Host-bound replies are consumed at their exact IPC endpoint. They must never fill the
    // old generic Continue queue (a Stop confirmation is not a checkpoint answer).
    if (bound === undefined) toCarry = option;
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
    /* 2026-09 (Z-36) — TRZY POLA TEJ SAMEJ RODZINY. Nośniki komend opisują żywy bieg tak samo
     * jak okna sklejania: wywołanie o tym samym identyfikatorze w następnym biegu przepisywałoby
     * wiersz poprzedniego. Komenda „w toku" po zejściu biegu jest pracą, której nikt nie wykonuje
     * (niezmiennik 17), a zdanie o czekaniu — wiadomością stojącą w kolejce do agenta, który
     * już nie słucha. */
    carriers.clear();
    inFlight.clear();
    queuedBehind = null;
    /* 2026-09 (Z-40) — TA SAMA RODZINA, dwa pola dalej. Kontrolka „Interrupt" nad biegiem, który
     * zszedł, prowadzi do tury, której nie ma, a zdanie odmowy opisuje rozmowę, która się
     * skończyła. Oba są kontrolką bez roboty (niezmiennik 16). */
    turnStartedAt = null;
    refusal = null;
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

  function tick(now: number): void {
    clock = now;
    const fresh = interruptOffer();
    const before = current.interrupt;
    /* BUDZIMY EKRAN TYLKO PRZY ZMIANIE. Ta funkcja biegnie raz na sekundę przez cały czas, w
     * którym cokolwiek idzie; publikacja przy każdym tyknięciu kazałaby Reactowi przerysować
     * strumień sześćdziesiąt razy na minutę za odpowiedź, która się nie zmieniła. Porównanie
     * jest po WARTOŚCIACH, bo `interruptOffer` składa świeży obiekt przy każdym pytaniu — a
     * pierwszy człon jest tu treścią: `{ subject: null }` i „nie ma czego przerywać" mają te same
     * pola i są dwoma różnymi ekranami. */
    const same =
      (fresh === null) === (before === null) &&
      fresh?.subject === before?.subject &&
      fresh?.refusal === before?.refusal;
    if (same) return;
    publish();
  }

  function interruptAnswered(said: Interrupted): void {
    refusal = interruptRefusal(said);
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
    tick,
    interruptAnswered,
    toggle,
    subscribe,
  };
}
