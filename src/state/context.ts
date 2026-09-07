/* Magazyn sekcji Context — nazwane zestawy materiałów, które człowiek wybiera do pracy.
 *
 * Ten plik NIE importuje `@tauri-apps/api`. Nazwy komend zna jedno miejsce w sekcji —
 * `src/sections/context/io.ts` — i to ono wstrzykuje tu [`ContextIo`] (niezmiennik 23: polityka
 * w jednym rdzeniu, adaptery po pięć linii). Test wstrzykuje atrapę zamiast podmieniać transport.
 *
 * Typy niżej są lustrem `src-tauri/src/context/mod.rs`. Rozjazd łapie granica: Tauri
 * deserializuje ładunek PRZED wejściem w ciało komendy, więc klucz, którego Rust nie ma, daje
 * wywołanie odrzucone, a nie mniejsze — pilnuje tego `src/sections/commands-wired.test.ts`.
 *
 * `Knowledge` zostaje osobną szufladą i ten magazyn nic z niej nie czyta: notatka wchodzi do
 * KAŻDEGO promptu, a zestaw jest materiałem WYBIERANYM do konkretnej pracy.
 */
import { create } from 'zustand';

import { why } from '../ipc/why';

/** Rodzaj materiału. Ten etap zapisuje wyłącznie `text`; `unknown` przychodzi z nowszego pliku. */
export type SourceKind = 'text' | 'unknown';

export interface ContextSource {
  id: string;
  kind: SourceKind;
  /** Nazwa, którą człowiek widzi na liście źródeł. */
  name: string;
  /** Po co ten materiał tu leży — jedno zdanie człowieka. */
  description: string;
  /** Materiał wpisany w polu, słowo w słowo. */
  text: string;
}

export interface ContextDraft {
  schema: number;
  /** Kolejność źródeł JEST kolejnością tej listy. */
  sources: ContextSource[];
  /** Identyfikatory źródeł wyłączonych z opracowania. */
  excluded: string[];
  /** Odpowiedź na pytanie `How should this context be prepared?`. */
  howToPrepare: string;
  /** Własne wymagania człowieka, zachowane co do brzmienia. */
  requirements: string[];
}

export interface ContextSet {
  schema: number;
  id: string;
  title: string;
  description: string;
  archived: boolean;
  draftRevision: number;
  /** Wersja gotowego opracowania. Zawsze `null`, dopóki nikt nie zbudował ani jednej. */
  latestReadyRevision: string | null;
  createdAt: string;
  changedAt: string;
}

/** Zestaw odczytany w całości — i rewizja, którą okno odda przy następnym zapisie. */
export interface ContextSetRead {
  set: ContextSet;
  draft: ContextDraft;
  revision: string;
}

/** To, co okno wysyła przy zapisie. Tytuł i szkic jadą razem: to jedna decyzja człowieka. */
export interface DraftEdit {
  id: string;
  title: string;
  description: string;
  draft: ContextDraft;
  /** Rewizja `draft.json`, którą to okno przeczytało. */
  expectedRevision: string | null;
}

/** Wszystko, co magazyn robi poza swoją głową. Jedna atrapa w teście zastępuje całość. */
export interface ContextIo {
  list(): Promise<ContextSet[]>;
  read(id: string): Promise<ContextSetRead>;
  create(title: string): Promise<ContextSetRead>;
  saveDraft(edit: DraftEdit): Promise<ContextSetRead>;
}

/**
 * Co ten magazyn wie o KATALOGU zestawów — trzy stany, nie dwa.
 *
 * Pusta biblioteka i biblioteka, do której jeszcze nikt nie zajrzał, są w zustandzie tą samą
 * tablicą; ekran, który tego nie rozróżnia, mówi „nic tu nie ma" o folderze, którego nikt nie
 * otworzył. Ta sama nazwa i te same trzy wartości, co w `src/state/agents.ts`.
 */
export type Library = 'reading' | 'read' | 'unreadable';

export interface ContextState {
  sets: ContextSet[];
  library: Library;
  /** Zestaw otwarty w edytorze — `null`, kiedy człowiek stoi na liście. */
  open: ContextSetRead | null;
  /** Czego człowiek szuka na liście. Puste znaczy „wszystko". */
  search: string;
  /** Zdanie po odmowie z Rusta — `null`, kiedy nie ma o czym mówić. Jedno na całą sekcję. */
  refusal: string | null;
  load: () => Promise<void>;
  create: (title: string) => Promise<void>;
  openSet: (id: string) => Promise<void>;
  close: () => void;
  /** Zapisuje i oddaje `true`, kiedy naprawdę się zapisało. Ekran zostaje otwarty po odmowie. */
  save: (edit: DraftEdit) => Promise<boolean>;
  narrow: (search: string) => void;
  dismiss: () => void;
}

/** Zestawy, które pasują do tego, czego człowiek szuka. Po nazwie — tak mówi PLAN §12. */
export function matching(sets: readonly ContextSet[], search: string): ContextSet[] {
  const wanted = search.trim().toLowerCase();
  if (wanted === '') return [...sets];
  return sets.filter((set) => set.title.toLowerCase().includes(wanted));
}

/** Pusty szkic — kształt, który dostaje zestaw, zanim człowiek cokolwiek w nim napisze. */
export function emptyDraft(): ContextDraft {
  return { schema: 1, sources: [], excluded: [], howToPrepare: '', requirements: [] };
}

/** Lista z podmienionym zestawem o tym `id`, albo z dopisanym, gdy go tam nie było. */
function upsert(sets: readonly ContextSet[], saved: ContextSet): ContextSet[] {
  const known = sets.some((set) => set.id === saved.id);
  return known ? sets.map((set) => (set.id === saved.id ? saved : set)) : [...sets, saved];
}

export function createContextStore(io: ContextIo) {
  return create<ContextState>()((set, get) => ({
    sets: [],
    library: 'reading',
    open: null,
    search: '',
    refusal: null,

    load: async () => {
      set({ refusal: null, library: 'reading' });
      try {
        set({ sets: await io.list(), library: 'read' });
      } catch (error) {
        /* Lista zostaje taka, jaka była: skasowanie jej tutaj mówiłoby „nic tam nie leży",
         * czego nie wiemy — a pusta biblioteka i nieczytelna wyglądają na ekranie tak samo. */
        set({
          refusal: why(error, 'Loadout could not read the context sets you have saved.'),
          library: 'unreadable',
        });
      }
    },

    create: async (title: string) => {
      set({ refusal: null });
      try {
        /* Dysk PIERWSZY, ekran drugi. W odwrotnej kolejności zestaw, którego zapis odmówił,
         * siedzi na liście do najbliższego uruchomienia i wygląda na zapisany (niezmiennik 4). */
        const made = await io.create(title.trim());
        set({ sets: upsert(get().sets, made.set), open: made });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not make that context set.') });
      }
    },

    openSet: async (id: string) => {
      set({ refusal: null });
      try {
        set({ open: await io.read(id) });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not open that context set.') });
      }
    },

    close: () => {
      set({ open: null, refusal: null });
    },

    save: async (edit: DraftEdit) => {
      set({ refusal: null });
      try {
        const saved = await io.saveDraft(edit);
        /* Rewizja świeżych bajtów wraca RAZEM z zestawem, więc następny zapis pyta o to, co
         * naprawdę leży na dysku. Bez tego drugi Save z tego samego okna niósłby rewizję sprzed
         * pierwszego i odbijałby się od pracy, którą sam przed chwilą zapisał. */
        set({ open: saved, sets: upsert(get().sets, saved.set) });
        return true;
      } catch (error) {
        /* Odmowa spóźnionego zapisu przyjeżdża GOTOWYM zdaniem z Rusta i tak jest pokazywana:
         * front nie wyciąga sensu z surowego błędu (D5, niezmiennik 14). Zdanie zapasowe stoi
         * tu wyłącznie na wypadek odmowy, która nie ma własnego. */
        set({ refusal: why(error, 'Loadout could not save that context set.') });
        return false;
      }
    },

    narrow: (search: string) => {
      set({ search });
    },

    dismiss: () => {
      set({ refusal: null });
    },
  }));
}
