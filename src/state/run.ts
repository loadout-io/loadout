/* Magazyn biegu: bufor linii i nic poza tym.
 *
 * Ten plik ma jedną robotę — DOPISAĆ i UCIĄĆ. Żadnego sklejania, żadnego zwijania, żadnych
 * etykiet: to wszystko mieszka w `src/sections/run/feed/model.ts`, bo tam da się je
 * przetestować bez okna, a tutaj rosłoby jako drugie miejsce prawdy o tym samym (niezmiennik 23).
 *
 * Limit 2000 linii wolno mieć TYLKO dlatego, że reszta biegu leży w `logs/agent-<id>.jsonl`
 * i w SQLite — pamięć jest oknem, pliki są prawdą (niezmiennik 4). Okno bez licznika tego,
 * co z niego wypadło, cicho kłamie: „Load earlier" nie ma o co poprosić, więc albo pobiera
 * od zera, albo nic. Stąd DWA pola obok siebie, i każde odpowiada na inne pytanie:
 *   `droppedBefore`    ile linii już wypadło — czy „Load earlier" ma w ogóle po co istnieć
 *                      (niezmiennik 16: kontrolka bez roboty nie wchodzi do repo),
 *   `earliestKnownId`  identyfikator najstarszej linii, którą jeszcze mamy — czyli granica,
 *                      od której ta kontrolka prosi o stronę wstecz.
 *
 * Typy `FeedLine` i `ForeignLine` stoją TUTAJ, a nie w sekcji, żeby zależność szła w jedną
 * stronę: sekcja zna magazyn, magazyn nie zna sekcji.
 */
import { create } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';
import type { Line } from '../ipc/types';

/** Dwa pola, które granica dokłada wierszowi z drutu. */
export interface Stamped {
  /** Ściśle rosnący numer nadawany po stronie Rusta [T2 §6.3]. */
  readonly id: number;
  /** Kiedy zdarzenie napłynęło, w milisekundach. Okno sklejania liczy się z tego. */
  readonly at: number;
}

/** Wiersz, który to repo umie nazwać: jeden z czternastu rodzajów, ostemplowany. */
export type FeedLine = Line & Stamped;

/**
 * Wiersz, którego rodzaju to repo NIE zna.
 *
 * Nie jest to hipoteza: vendorzy dokładają typy zdarzeń co tydzień i po cichu, a lustro
 * `src/ipc/types.ts` jest pisane ręcznie. Kształt jest w typie, żeby model musiał się
 * z takim wierszem zmierzyć w czasie kompilacji, a nie dopiero na ekranie użytkownika
 * (niezmiennik 5 w duchu, po stronie frontu).
 */
export interface ForeignLine extends Stamped {
  readonly kind: string;
  readonly agent: string;
}

/** Cokolwiek, co może wjechać kanałem. */
export type Incoming = FeedLine | ForeignLine;

/** Siedem stanów kroku [ARCHITECTURE §5]. `paused` jest stanem BIEGU, nigdy kroku. */
export type StepState =
  'pending' | 'ready' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'skipped';

/** Krok biegu w kolejności grafu — jeden do jednego z blokiem paska loadoutu. */
export interface Step {
  readonly id: string;
  readonly name: string;
  readonly state: StepState;
}

/** Kto to powiedział — trzy wartości, nie osiem [00-SYNTHESIS §2.2]. */
export type Who = 'you' | 'agent' | 'loadout';

/** Odpowiedź człowieka na pytanie agenta. */
export interface Answer {
  readonly questionId: number;
  readonly option: string;
  readonly who: Who;
}

/** Ile linii biegu trzymamy w pamięci naraz [T2 §6.3, obrona 5]. */
export const LINE_LIMIT = 2000;

export interface RunState {
  /** Okno ostatnich `LINE_LIMIT` linii, najstarsza pierwsza. */
  readonly lines: readonly FeedLine[];
  /** Ile linii wypadło z głowy okna od początku biegu. */
  readonly droppedBefore: number;
  /** Identyfikator najstarszej linii, którą jeszcze mamy; `null`, dopóki nie ma żadnej. */
  readonly earliestKnownId: number | null;
  /** Agenci, którzy w tym biegu wystąpili, w kolejności pojawienia się. */
  readonly agents: readonly string[];
  /**
   * Kroki biegu w kolejności grafu.
   *
   * Przychodzą z grafu razem ze startem biegu, nie z linii — linia `step` jest kreską
   * w strumieniu, a nie stanem kroku, więc odtwarzanie z niej listy kroków dałoby pasek,
   * który rośnie w trakcie biegu zamiast pokazywać plan od pierwszej sekundy (niezmiennik 17).
   * Wypełnia je komenda startu biegu; to zadanie ich nie pisze, tylko czyta.
   */
  readonly steps: readonly Step[];
  /**
   * Nazwa workflow, który ten bieg wykonuje — pierwszy człon podpisu paska loadoutu.
   *
   * Stoi obok `steps`, bo przychodzi tą samą drogą i w tej samej chwili: pasek bez nazwy
   * i pasek bez kroków to ten sam brak. Puste, dopóki nie ma biegu.
   */
  readonly workflow: string;
  readonly answers: readonly Answer[];

  /**
   * Dokłada paczkę i przycina okno do `LINE_LIMIT`. Oddaje linie, które weszły —
   * dokładnie te obiekty, które przyszły, nigdy ich kopie.
   */
  appendLines: (batch: readonly FeedLine[]) => readonly FeedLine[];

  /** Zapisuje odpowiedź człowieka. */
  answer: (questionId: number, option: string) => void;
}

export type RunStore = UseBoundStore<StoreApi<RunState>>;

/**
 * Agenci po dołożeniu paczki — TA SAMA tablica, kiedy nikt nowy się w niej nie pojawił.
 *
 * Tożsamość jest tu całą robotą: świeża tablica przy każdej paczce mówi Reactowi, że szyna
 * agentów się zmieniła, dwadzieścia razy na sekundę i przez cały bieg, w którym skład agentów
 * ustala się w pierwszych trzech zdarzeniach.
 */
function withAgents(agents: readonly string[], batch: readonly FeedLine[]): readonly string[] {
  let next: string[] | null = null;
  const seen = new Set(agents);
  for (const line of batch) {
    if (seen.has(line.agent)) continue;
    seen.add(line.agent);
    (next ??= [...agents]).push(line.agent);
  }
  return next ?? agents;
}

/**
 * Nowy magazyn. Fabryka, nie singleton na poziomie modułu: dwa testy w jednym pliku dzieliłyby
 * stan i drugi z nich czytałby linie pierwszego.
 */
export function createRunStore(): RunStore {
  return create<RunState>()((set) => ({
    lines: [],
    droppedBefore: 0,
    earliestKnownId: null,
    agents: [],
    steps: [],
    workflow: '',
    answers: [],

    appendLines(batch: readonly FeedLine[]): readonly FeedLine[] {
      /* Paczka bez ani jednej linii nie rusza stanu. To nie jest mikrooptymalizacja: kanał
       * woła sink także wtedy, gdy z paczki nie przeżył żaden wiersz (src/ipc/run.ts), a `set`
       * ze świeżą tablicą `lines` jest dla Reacta zmianą i przerysowuje całą historię. */
      if (batch.length === 0) return batch;

      set((state) => {
        const lines = [...state.lines, ...batch];
        /* Obcinamy GŁOWĘ, nigdy ogon. Implementacja obcinająca ogon trzyma dwa tysiące
         * NAJSTARSZYCH linii: długość się zgadza, strumień zamiera w połowie biegu i wygląda
         * to dokładnie jak bieg, który się zatrzymał. */
        const dropped = Math.max(0, lines.length - LINE_LIMIT);
        if (dropped > 0) lines.splice(0, dropped);

        return {
          /* `lines` niesie TE SAME obiekty, które przyszły — `[...]` i `splice` przepisują
           * tablicę, nie wiersze. Kopia wiersza jest poprawna co do wartości i katastrofalna
           * dla Reacta: każdy widoczny wiersz dostaje nową tożsamość na każdą paczkę. */
          lines,
          droppedBefore: state.droppedBefore + dropped,
          /* Dwa pola, dwa różne pytania: ile wypadło (czy „Load earlier" ma po co istnieć)
           * i od czego zacząć prośbę wstecz. */
          earliestKnownId: lines[0]?.id ?? null,
          agents: withAgents(state.agents, batch),
        };
      });

      return batch;
    },

    answer(questionId: number, option: string): void {
      /* `who: 'you'` — trzy autorytety w całej aplikacji, nie osiem [00-SYNTHESIS §2.2]. */
      set((state) => ({ answers: [...state.answers, { questionId, option, who: 'you' }] }));
    },
  }));
}
