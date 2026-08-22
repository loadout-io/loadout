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
import {
  listHandoffs,
  listNotes,
  putToUse,
  stopUsing as stopUsingOnDisk,
} from '../sections/memory/io';
import { why } from '../ipc/why';

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
  /**
   * Czyja to wiedza — nazwa agenta z pliku notatki.
   *
   * Nieobecne znaczy „niczyja", i to jest jedyna poprawna odpowiedź dla notatki o zakresie
   * `everywhere` albo `this-project`. Wiersz, który dla braku właściciela pisze myślnik albo
   * „unassigned", odpowiada na pytanie, którego nikt nie zadał — a człowiek czyta to jako
   * fakt o notatce.
   *
   * `null` stoi tu obok braku, bo Rust przysyła KLUCZ z pustą wartością (`NoteWire::agent`):
   * zbiór kluczy drutu jest porównywany z tym interfejsem co do jednego, więc klucz pomijany
   * dla części notatek znaczyłby, że lustro zgadza się tylko czasem. Wiersz sprawdza wartość,
   * nie obecność klucza — i dlatego oba zapisy „nikt" prowadzą do tego samego pustego miejsca.
   */
  agent?: string | null;
  /**
   * Z jakiego projektu ta notatka przyszła. Puste znaczy „stąd" — notatka napisana tutaj
   * nie ma pochodzenia do pokazania. `null` z tego samego powodu, co wyżej.
   */
  from?: string | null;
}

/**
 * Jeden plik, który jeden agent zostawił drugiemu. Lustro `HandoffWire`
 * (`src-tauri/src/commands/memory.rs`, camelCase na drucie).
 *
 * Pola są tu WSZYSTKIE, także te, których trzecia strefa nie pokazuje (`run`, `kind`,
 * `created`, `id`, `title`): lustro, które przepisuje połowę drutu, przy pierwszej zmianie
 * kształtu milczy dokładnie o tej połowie, której nie zna. Co z tego dojeżdża na ekran,
 * rozstrzyga `src/sections/memory/passed-row.tsx` i tylko on.
 */
export interface Handoff {
  id: string;
  /** Bieg, w którym ten plik powstał. Dziś nie ma go na ekranie — patrz `listHandoffs`. */
  run: string;
  /** Kto to zostawił. */
  from: string;
  /** Dla kogo. Pusta lista znaczy „dla nikogo w szczególności", nie „dla wszystkich". */
  to: string[];
  /** `brief`, `findings`, `plan`, `patch-summary`, `question`, `answer`, `review` — albo coś,
   * czego ta wersja nie zna (niezmiennik 5: nieznana wartość jest niesiona, nie odrzucana). */
  kind: string;
  title: string;
  /** `current` albo `superseded`. Przekazania są niezmienne — korekta to nowy plik [T6 §9]. */
  status: string;
  created: string;
  /** Gdzie ten plik leży. To jest jedyna rzecz, która czyni z niego „plik na dysku". */
  path: string;
  bytes: number;
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
  /**
   * Co agenci przekazali sobie po drodze. Trzecia strefa ekranu, do 2026-08-18 nieodczytywana
   * przez nic.
   */
  passed: Handoff[];
  /** Zdanie po angielsku mówiące, co się stało. `null`, kiedy nie ma nic do powiedzenia. */
  message: string | null;
  /**
   * Odmowa odczytu PRZEKAZAŃ, osobno od `message`.
   *
   * To nie jest drugie miejsce na ten sam fakt (niezmiennik 13), a dwa różne fakty: „nie umiem
   * przeczytać notatek" i „nie umiem przeczytać tego, co agenci sobie przekazali" mówią
   * o dwóch różnych katalogach i wymagają dwóch różnych rzeczy do zrobienia. Zlane w jedno
   * pole, drugie nadpisuje pierwsze i jedna z dwóch odmów ginie w każdym wejściu w sekcję.
   */
  passedProblem: string | null;
  choice: Choice | null;
  /**
   * Wejście w sekcję: przeczytaj, co leży na dysku, i pokaż to.
   *
   * Do 2026-08-18 tej ścieżki nie było wcale i to jest cały powód, dla którego pole istnieje.
   */
  load: () => Promise<void>;
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
const COULD_NOT_READ = 'Loadout could not read the notes on this machine.';
const COULD_NOT_READ_PASSED = 'Loadout could not read what agents passed to each other.';

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
  passed: [],
  message: null,
  passedProblem: null,
  choice: null,

  load: async () => {
    /* DWA ODCZYTY, DWIE OSOBNE ODMOWY, i to nie jest ostrożność na zapas: notatki leżą
     * w `~/.loadout/memory/notes/`, a przekazania w katalogach biegów
     * (`<repo>/.loadout/runs/<…>/handoffs/`). Jeden `try` na oba znaczy, że katalog notatek,
     * którego nie da się przeczytać, zabiera z ekranu także przekazania — czyli awaria jednej
     * ścieżki pustoszy strefę, która ma swoje pliki w porządku. Awaria każdej z nich ma
     * kosztować dokładnie tyle, ile mówi (niezmiennik 5). */
    try {
      /* PODMIANA CAŁEJ LISTY, nigdy dopisanie. Wejście w sekcję drugi raz dokładałoby wtedy
       * te same notatki jeszcze raz, a człowiek zobaczyłby każdą podwójnie i licznik nad
       * sekcją policzyłby pliki dwa razy. Lista ma być odpowiedzią dysku, a nie sumą
       * wszystkich odpowiedzi, jakich dysk kiedykolwiek udzielił. */
      set({ notes: await listNotes(), message: null });
    } catch (refusal) {
      /* Odmowa NIE leci w górę: wywołujący to wejście w sekcję, a wyjątek stamtąd wywraca
       * ekran zamiast pokazać zdanie. Lista pustoszeje z rozmysłem — notatki sprzed odmowy są
       * tym, co sekcja PAMIĘTA, a nie tym, co leży w plikach, i pokazanie ich byłoby dokładnie
       * tym kłamstwem, przed którym stoi niezmiennik 4. */
      set({ notes: [], message: why(refusal, COULD_NOT_READ) });
    }

    try {
      /* Ta sama reguła podmiany całej listy i ten sam powód. */
      set({ passed: await listHandoffs(), passedProblem: null });
    } catch (refusal) {
      set({ passed: [], passedProblem: why(refusal, COULD_NOT_READ_PASSED) });
    }
  },

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
