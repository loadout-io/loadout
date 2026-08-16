/* Magazyn sekcji Pamięć: dwa stany notatki i jedno miejsce, w którym stan się zmienia.
 *
 * DLACZEGO BEZ OPTYMISTYCZNEGO PRZESTAWIENIA. Wiersz, który pokazuje „In use", zanim Rust
 * potwierdził zapis, jest kłamstwem o tym, co wejdzie do promptu — a to jest jedyna rzecz,
 * o której ta sekcja mówi. Zwykły optymistyczny magazyn kłamie przez 30 ms i nikt tego nie
 * zauważa; ten kłamałby także wtedy, gdy zapis się nie udał, bo zakres był pełny. Stąd
 * kolejność: komenda, odpowiedź, dopiero potem stan.
 *
 * DLACZEGO WYMUSZONY WYBÓR MIESZKA W MAGAZYNIE, A NIE W KOMPONENCIE. „Zakres jest pełny"
 * przychodzi z Rusta jako odmowa promocji [T6 §5.3]. Okno, które otwiera się samo w wierszu,
 * dostaje tę informację przez propsy z dwóch różnych miejsc i pierwsza ścieżka, która o nim
 * zapomni, cicho przywróci ciche przycięcie.
 *
 * Typy niżej są lustrem `src-tauri/src/memory/notes.rs`. Dopóki nie ma generatora, rozjazd
 * łapią kryteria po stronie Rusta: tam te same pola są zamrożone testem.
 *
 * Nazw komend nie zna ten plik: zna je `sections/memory/io.ts` i to jest JEDYNA krawędź,
 * przez którą cokolwiek jedzie do Rusta (niezmiennik 23). Zdanie „ani jednego wywołania
 * więcej" ma sens tylko wtedy, kiedy jest jedna droga do policzenia.
 *
 * Ciała akcji są jeszcze puste. Szkielet ma się WCZYTAĆ i paść w czasie wykonania — moduł,
 * którego nie ma, daje „Cannot find module", czyli czerwień, której bramka nie liczy
 * (AGENTS.md §2a).
 */
import { create } from 'zustand';

/** Dwa stany i ani jeden trzeci [ARCHITECTURE §2 pyt. 5]. Słowo jest to samo, co w pliku. */
export type NoteStatus = 'suggested' | 'in-use';

/** Trzy zakresy, które mają własny budżet [T6 §6]. */
export type NoteScope = 'everywhere' | 'this-project' | 'this-agent';

/** Notatka tak, jak ją widzi sekcja. Lustro `notes::Note`, bez pól, których UI nie pokazuje. */
export interface Note {
  id: string;
  title: string;
  /** Jedna linia — i jedyna część notatki, która trafia do promptu. */
  rule: string;
  /** Dlaczego to jest prawda. Bez tego notatka nie powstaje [T6 §10.3]. */
  because: string;
  status: NoteStatus;
  scope: NoteScope;
  /**
   * Ile ta notatka zabiera z budżetu zakresu.
   *
   * Pole nazywa się `length`, bo tak brzmi to słowo w interfejsie [DESIGN.md §8]. Nazwa
   * z drutu wjeżdża na ekran przez pole o tej nazwie, nie przez tłumaczenie w komponencie.
   */
  length: number;
  /** W ilu osobnych zgłoszeniach ta kandydatka się pojawiła. Sygnał, nigdy decyzja. */
  occurrences: number;
  modified: string;
}

/**
 * Odmowa „zakres jest pełny", tak jak przyjeżdża z Rusta [T6 §5.3].
 *
 * Kształt jest tu wypisany, bo magazyn musi go rozpoznać, żeby otworzyć wymuszony wybór
 * zamiast pokazać kolejne zdanie o błędzie.
 */
export interface MemoryFull {
  /** O ile jednostek długości ta promocja przekroczyłaby limit zakresu. */
  overBy: number;
  /** Identyfikatory notatek do odstawienia, najdawniej użyte pierwsze. */
  retire: string[];
}

/** Wymuszony wybór czekający na człowieka. `null` znaczy, że nic nie czeka (niezmiennik 13). */
export interface Choice {
  /** Notatka, którą człowiek chciał wziąć do użytku. */
  id: string;
  overBy: number;
  retire: string[];
}

export interface MemoryState {
  notes: Note[];
  /** Zdanie po angielsku mówiące, co się stało. `null`, kiedy nie ma nic do powiedzenia. */
  message: string | null;
  choice: Choice | null;
  /** „Use this" — od tej chwili notatka wchodzi do promptu. Decyduje odpowiedź z Rusta. */
  use: (id: string) => Promise<void>;
  /** „Stop using" — notatka zostaje na liście i przestaje wchodzić do promptu. */
  stopUsing: (id: string) => Promise<void>;
  /** Zamyka wymuszony wybór, niczego nie zmieniając. */
  cancel: () => void;
}

export const useMemory = create<MemoryState>()(() => ({
  notes: [],
  message: null,
  choice: null,

  use: async (_id: string) => {
    /* T-17: komenda, odpowiedź, dopiero potem stan. */
  },

  stopUsing: async (_id: string) => {
    /* T-17: to samo w drugą stronę. */
  },

  cancel: () => {
    /* T-17: zamknij okno, nie ruszaj statusów. */
  },
}));
