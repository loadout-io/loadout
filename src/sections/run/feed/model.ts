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
import type { Answer, Incoming } from '../../../state/run';
import type { Kind } from './kinds';

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
  /** Jeden wiersz na agenta biegu. Nigdy wycinek historii — wycinek pełznie. */
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
  readonly kind: Kind;
  readonly agent: string;
  /** Tekst po angielsku z zamkniętej tabeli; licznik jest zawsze w środku [T2 ryzyko 3]. */
  readonly label: string;
  readonly count: number;
  /** Identyfikatory sklejonych linii w kolejności napłynięcia — rozwinięcie oddaje je. */
  readonly ids: readonly number[];
  readonly expanded: boolean;
  /** Ostatnie 20 linii wyjścia; niepuste tylko dla `ran`, które padło [T2 §7.3 reguła 3]. */
  readonly output: readonly string[];
  /** Numer, o który poprosi panel szczegółów. Sam panel jest poza tym zadaniem. */
  readonly detailId: number | null;
}

/** Pytanie do człowieka. Przyklejone, dopóki nie ma odpowiedzi [T2 §7.2 wiersz 10]. */
export interface Question {
  readonly id: number;
  readonly text: string;
  readonly options: readonly string[];
}

/** Czyja jest teraz kolej. `you` maluje się kolorem `--attend` [DESIGN §3]. */
export type Attention = 'agents' | 'you';

export interface FeedView {
  readonly history: readonly HistoryRow[];
  readonly now: NowZone;
  readonly pinned: Question | null;
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
  /** Odpowiedź człowieka: zdejmuje przypięcie tego pytania i zapisuje ją z `who: 'you'`. */
  answer(questionId: number, option: string): void;
}

/** Nowy, pusty model widoku pracy. */
export function createFeed(scroller: Scroller): Feed {
  /* Zaślepka fazy kontraktu; implementacja zastępuje całe ciało. `void` jest tu tylko po to,
   * żeby sygnatura — która JEST kontraktem — nie musiała chodzić z podkreśleniem w nazwie. */
  void scroller;
  throw new Error('not implemented');
}
