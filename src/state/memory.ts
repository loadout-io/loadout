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
 */
import { create } from 'zustand';
import { putToUse, stopUsing as stopUsingOnDisk } from '../sections/memory/io';

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

/* Zdania odmowy na wypadek, gdyby Rust nie przysłał własnego. Odmowa w ciszy wygląda dokładnie
 * jak zepsuty przycisk, a człowiek, który nie wie, czego się od niego chce, klika drugi raz. */
const COULD_NOT_USE = 'Loadout could not put that note to use.';
const COULD_NOT_STOP = 'Loadout could not stop using that note.';

/**
 * Czy ta odmowa jest „zakres jest pełny".
 *
 * Po KSZTAŁCIE, nie po klasie błędu: przez krawędź IPC jedzie zwykły obiekt, więc `instanceof`
 * odpowiedziałby „nie" na każdą odmowę z Rusta i wymuszony wybór nigdy by się nie otworzył.
 * Pytamy o dwa pola, których zwykły błąd nie ma, i nie sprawdzamy typu każdego elementu listy:
 * nieznany kształt ma się zdegradować do zwykłej odmowy, a nie wywalić sekcji (niezmiennik 5).
 */
function isMemoryFull(refusal: unknown): refusal is MemoryFull {
  if (typeof refusal !== 'object' || refusal === null) return false;
  const maybe = refusal as { overBy?: unknown; retire?: unknown };
  return typeof maybe.overBy === 'number' && Array.isArray(maybe.retire);
}

/** Zdanie od Rusta, kiedy jakieś jest — jego odmowy są już napisane po ludzku. */
function why(refusal: unknown, fallback: string): string {
  const said = refusal instanceof Error ? refusal.message.trim() : '';
  return said.length > 0 ? said : fallback;
}

/**
 * Notatka odczytana z pliku po zapisie zastępuje tę, którą sekcja trzymała.
 *
 * Podmiana CAŁEGO obiektu, nie samego `status`: wraz ze statusem zmienia się `modified`, a przy
 * drugim zgłoszeniu także `occurrences`. Przepisanie jednego pola zostawiłoby wiersz, który
 * o jednej rzeczy mówi prawdę z dysku, a o reszcie to, co pamiętał sprzed zapisu.
 */
function replace(notes: Note[], fresh: Note): Note[] {
  return notes.map((one) => (one.id === fresh.id ? fresh : one));
}

export const useMemory = create<MemoryState>()((set, get) => ({
  notes: [],
  message: null,
  choice: null,

  use: async (id: string) => {
    try {
      /* Komenda, odpowiedź, DOPIERO POTEM stan. Wiersz przestawiony przed odpowiedzią pokazuje
       * „In use" dla notatki, której plik dalej mówi `suggested` — czyli kłamie dokładnie o tym
       * jednym, o czym ta sekcja mówi: co wejdzie do promptu następnego agenta. */
      const fresh = await putToUse({ id });
      set({ notes: replace(get().notes, fresh), message: null, choice: null });
    } catch (refusal) {
      if (isMemoryFull(refusal)) {
        /* Lista do wymuszonego wyboru przychodzi Z ODMOWY i tylko stamtąd. Złożona tutaj
         * z tego, co sekcja akurat trzyma, byłaby drugą odpowiedzią na pytanie „co odstawić",
         * liczoną bez połowy plików i bez `last_used_at` (niezmiennik 13). */
        set({ choice: { id, overBy: refusal.overBy, retire: refusal.retire }, message: null });
        return;
      }
      /* Zwykła odmowa NIE otwiera okna: pytanie „które notatki odstawić" postawione komuś,
       * kto właśnie usłyszał „ta notatka nie ma uzasadnienia", każe naprawiać nie to, co jest
       * zepsute. Jedna odmowa, jedno miejsce, w którym o niej piszemy. */
      set({ message: why(refusal, COULD_NOT_USE), choice: null });
    }
  },

  stopUsing: async (id: string) => {
    try {
      const fresh = await stopUsingOnDisk({ id });
      set({ notes: replace(get().notes, fresh), message: null, choice: null });
    } catch (refusal) {
      set({ message: why(refusal, COULD_NOT_STOP), choice: null });
    }
  },

  cancel: () => {
    /* Zamknięcie okna nie jest zgodą na nic: żaden status się nie rusza i nic nie jedzie do
     * Rusta. Magazyn, który „przy okazji" odstawia pierwszą pozycję z listy, jest tym samym
     * cichym przycięciem, przed którym stoi cały ten podsystem [T6 §5.3]. */
    set({ choice: null });
  },
}));
