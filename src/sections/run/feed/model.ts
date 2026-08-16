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
import { LINE_LIMIT } from '../../../state/run';
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
  /**
   * Przełącza rozwinięcie JEDNEGO wiersza — to, co robi `+` przy zwiniętej linii.
   *
   * Jest w modelu, a nie w komponencie, z tego samego powodu, co reszta: stan rozwinięcia
   * jest polem wiersza, więc przycisk, który trzymałby go u siebie, byłby drugim miejscem
   * prawdy o tym samym (niezmiennik 13). Wiersz, którego nie ma, nie robi nic — kliknięcie
   * w wiersz wypchnięty z okna nie ma prawa przewrócić widoku.
   */
  toggle(rowId: number): void;
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

/** Numer dla panelu szczegółów; większość rodzajów nie ma czego pokazać pod kliknięciem. */
function detailOf(line: FeedLine): number | null {
  return 'detailId' in line ? line.detailId : null;
}

/** Czy ta linia jest niepowodzeniem, które rozwija się samo [T2 §7.3 reguła 3]. */
function failed(line: FeedLine): boolean {
  return line.kind === 'ran' && !line.ok;
}

/** Etykieta wiersza stojącego za `count` liniami tego rodzaju. */
function labelFor(line: FeedLine, count: number): string {
  const folded = FOLDED[line.kind];
  if (folded === undefined) return sentence(line);
  if (count > 1 || COUNTS_FROM_ONE.has(line.kind)) return folded(count);
  return sentence(line);
}

/** Świeży wiersz historii dla tej linii. */
function rowFor(line: FeedLine): HistoryRow {
  const broke = failed(line);
  return {
    id: line.id,
    kind: line.kind,
    agent: line.agent,
    label: labelFor(line, 1),
    count: 1,
    ids: [line.id],
    /* Niepowodzenie rozwija SIEBIE i nic poza sobą. Rozwinięcie całego strumienia po
     * pierwszym błędzie („tryb paniki") jest dokładnie tą ścianą tekstu, przed którą stoi
     * reguła 2 — i wygląda jak troska. */
    expanded: broke || REGISTRY[line.kind].expanded,
    /* OSTATNIE dwadzieścia linii, nie pierwsze: `slice(0, 20)` pokazuje początek logu, czyli
     * tę jego połowę, która nigdy nie zawiera powodu, i przechodzi każde sprawdzenie liczące
     * same wiersze. */
    output: broke && line.kind === 'ran' ? line.detail.slice(-OUTPUT_LINES) : [],
    detailId: detailOf(line),
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
  };
}

/** Nowy, pusty model widoku pracy. */
export function createFeed(scroller: Scroller): Feed {
  /** Historia. Nowa tablica dokładnie wtedy, kiedy coś do niej weszło — i ani razu więcej. */
  let history: readonly HistoryRow[] = [];

  /**
   * Agent → co robi teraz.
   *
   * `Map`, bo kolejność wstawienia JEST kolejnością pojawienia się w biegu, a strefa TERAZ ma
   * mieć jeden wiersz na agenta. Wycinek historii (`lines.slice(-4)`) daje na zrzucie ekranu
   * to samo i pełznie o wiersz na każde zdarzenie.
   */
  const doing = new Map<string, string>();

  /** Nazwa agenta, którego slot `Thinking…` jest żywy. JEDNO pole, nigdy tablica. */
  let thinking: string | null = null;

  /** Otwarta grupa per agent — klucz sklejania to para (agent, rodzaj), stąd mapa po agencie. */
  const groups = new Map<string, Group>();

  /** Pytania bez odpowiedzi, najstarsze pierwsze. Przypięte jest zawsze to spod zera. */
  let waiting: readonly Question[] = [];

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
      /* Jeden fakt, jedno miejsce: „czyja kolej" wynika z przypięcia, więc nie da się ustawić
       * go osobno i rozjechać z nim (niezmiennik 13). */
      attention: pinned === null ? 'agents' : 'you',
      answers,
    };
  }

  let current: FeedView = snapshot();

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

      if (!doing.has(line.agent)) doing.set(line.agent, '');

      if (REGISTRY[line.kind].route === 'now') {
        /* Status, nie linia: nie wchodzi do historii, nie zamyka grupy i nie przepisuje tego,
         * co agent ostatnio zrobił. Sam slot jest jeden na cały widok. */
        thinking = line.agent;
        continue;
      }

      /* Prawdziwa linia gasi slot [T2 §7.2 wiersz 4] — dowolna, nie tylko od tego agenta:
       * slot jest jeden, więc pytanie „czyj jest" ma dokładnie jedną odpowiedź. */
      thinking = null;
      doing.set(line.agent, sentence(line));

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
        waiting = [...waiting, { id: line.id, text: line.text, options: [...line.options] }];
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

    if (changed) current = snapshot();

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
    /* `who: 'you'` — trzy autorytety w całej aplikacji, nie osiem [00-SYNTHESIS §2.2]. */
    answers = [...answers, { questionId, option, who: 'you' }];
    current = snapshot();
  }

  function toggle(rowId: number): void {
    const index = history.findIndex((row) => row.id === rowId);
    const row = index < 0 ? undefined : history[index];
    if (row === undefined) return;
    const rows = [...history];
    rows[index] = { ...row, expanded: !row.expanded };
    history = rows;
    current = snapshot();
  }

  return {
    get view(): FeedView {
      return current;
    },
    appendLines,
    jumpToNewest,
    answer,
    toggle,
  };
}
