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

/** Rodzaj materiału. `unknown` przychodzi z nowszego pliku i ma być minięty, nie wywrócony. */
export type SourceKind = 'text' | 'image' | 'pdf' | 'document' | 'unknown';

/** Plik zapisany w bibliotece — KOPIA, nigdy dowiązanie do miejsca, z którego przyszedł. */
export interface StoredFile {
  /** Ścieżka względem katalogu zestawu. Okno jej nie używa do niczego poza pokazaniem. */
  path: string;
  revision: string;
  mime: string;
  bytes: number;
  fingerprint: string;
  /** Ile ważą pochodne tego źródła. Osobny budżet od oryginałów. */
  derived: number;
  /** Ile stron ma dokument — `null`, dopóki nikt go nie otworzył. */
  pages: number | null;
}

/**
 * Gdzie stoi przygotowanie pliku.
 *
 * Unia po `state`, tak jak po stronie Rusta. Wariant `unknown` jest tu, bo nowszy Loadout może
 * dopisać stan, którego ten build nie zna — i ma go MINĄĆ, nie przewrócić na nim ekranu.
 */
export type Preparation =
  | { state: 'notNeeded' }
  | { state: 'needs'; pagesDone: number }
  | { state: 'ready' }
  | { state: 'failed'; said: string }
  | { state: 'unknown' };

export interface ContextSource {
  id: string;
  kind: SourceKind;
  /** Nazwa, którą człowiek widzi na liście źródeł. */
  name: string;
  /** Po co ten materiał tu leży — jedno zdanie człowieka. */
  description: string;
  /** Materiał wpisany w polu, słowo w słowo. Przy źródle plikowym pusty. */
  text: string;
  /* Cztery pola niżej są OPCJONALNE po stronie okna, choć Rust wysyła je zawsze: `draft.json`
   * zapisany przed CT-02 ich nie niesie, a szkic składany w teście nie ma powodu ich wypisywać. */
  file?: StoredFile | null;
  preparation?: Preparation;
  /** Tekst, przy którym stanął ten obraz — szew jednego mieszanego wklejenia. */
  companionOf?: string | null;
  /** Widoczne ograniczenia tego źródła, gotowymi zdaniami. */
  notes?: string[];
}

/** Bajty ze schowka. Nazwy pliku w tym kształcie nie ma i mieć nie ma. */
export interface PastedBytes {
  mime: string;
  base64: string;
}

/** Jedna pozycja żądania importu: plik z dysku ALBO to, co niesie schowek. */
export interface ImportItem {
  name: string;
  path?: string | null;
  text?: string | null;
  image?: PastedBytes | null;
}

/** Wynik JEDNEJ pozycji. Pięć wybranych plików daje pięć takich wierszy. */
export interface ImportResult {
  name: string;
  added: string[];
  /** Zdanie odmowy z Rusta — `null`, kiedy pozycja weszła. */
  refused: string | null;
  notes: string[];
}

export interface ImportReport {
  operationId: string;
  results: ImportResult[];
  read: ContextSetRead;
}

/** Jedna strona przygotowana przez lokalny worker, ze stemplami, po których się ją rozlicza. */
export interface PreparedPage {
  operationId: string;
  fingerprint: string;
  pagesTotal: number;
  number: number;
  text: string;
  image: PastedBytes | null;
  /**
   * Czym plik zawinił, kiedy nie da się go otworzyć w ogóle — wtedy pola wyżej są puste.
   *
   * Tą samą drogą, co strona, bo rozlicza się tak samo: porażka od okna, które pracowało nad
   * poprzednim importem, nie ma prawa oznaczyć pliku wybranego przed chwilą.
   */
  failed?: Unopenable | null;
}

/** Dlaczego dokumentu nie da się otworzyć. Rozpoznaje to sterownik, ZDANIE pisze Rust. */
export type Unopenable = 'locked' | 'damaged';

export interface PreviewImage {
  mime: string;
  base64: string;
}

/** Kawałek zatwierdzonego źródła. Adresem jest ID źródła, nigdy ścieżka od okna. */
export type SourcePart =
  | { kind: 'image'; image: PreviewImage }
  | { kind: 'page'; number: number; pagesTotal: number; text: string; image: PreviewImage | null }
  | { kind: 'text'; text: string; more: boolean }
  | { kind: 'whole'; mime: string; base64: string; operationId: string; fingerprint: string };

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

export interface DeleteContextSet {
  setId: string;
  title: string;
  uses: string[];
  deleted: boolean;
  said: string;
}

/** Dokładna wersja zestawu i tematy wybrane do workflow albo jednego kroku. */
export type ContextTopics = 'all' | string[];

export interface ContextPin {
  id: string;
  revision: string;
  topics: ContextTopics;
}

export interface WorkflowContext {
  schema: 1;
  sets: ContextPin[];
}

export interface StepContext {
  schema: 1;
  inheritWorkflow?: boolean | undefined;
  exclude?: string[] | undefined;
  sets?: ContextPin[] | undefined;
}

export interface ContextChoice {
  id: string;
  title: string;
  description: string;
  revision: string | null;
  topics: ContextTopic[];
  said: string | null;
}

export interface SelectedContext {
  id: string;
  title: string;
  revision: string;
  selectedTopics: ContextTopics;
  topics: ContextTopic[];
  source: 'workflow' | 'step';
  update: 'Update available' | null;
  said: string | null;
}

export interface OmittedContext {
  id: string;
  title: string;
  said: string;
}

export interface StepContextView {
  stepId: string;
  sets: SelectedContext[];
  omitted: OmittedContext[];
  inheritsWorkflow: boolean;
  protectedScope: boolean;
  said: string | null;
}

export interface WorkflowContextView {
  catalog: ContextChoice[];
  workflow: SelectedContext[];
  steps: StepContextView[];
  warnings: string[];
}

/** Zestaw odczytany w całości — i rewizja, którą okno odda przy następnym zapisie. */
export interface ContextSetRead {
  set: ContextSet;
  draft: ContextDraft;
  revision: string;
}

export type ContextApp = 'claude-code' | 'codex';
export type SourceOutcome = 'processed' | 'excluded' | 'failed' | 'unknown';
export type BuildStage =
  | 'freezing'
  | 'splitting'
  | 'extracting'
  | 'grouping'
  | 'publishing'
  | 'ready'
  | 'interrupted'
  | 'unknown';
export type BuildEnd =
  'running' | 'ready' | 'cancelled' | 'failed' | 'stillRunning' | 'interrupted' | 'unknown';
export type FindingKind =
  'requirement' | 'fact' | 'visual-reference' | 'assumption' | 'question' | 'conflict' | 'unknown';

export interface SourceReference {
  sourceId: string;
  part: string;
}

export interface ContextFinding {
  id: string;
  kind: FindingKind;
  text: string;
  condition: string;
  sources: SourceReference[];
  topic: string;
  conflictsWith: string[];
  origin: 'human' | 'generated' | 'unknown';
}

export interface ContextTopic {
  id: string;
  title: string;
}

export interface SourceProgress {
  sourceId: string;
  part: string;
  outcome: SourceOutcome;
  said: string;
}

export interface ContextBuild {
  operationId: string;
  setId: string;
  generation: number;
  draftRevision: number;
  stage: BuildStage;
  end: BuildEnd;
  app: ContextApp;
  requestedModel: string | null;
  model: string | null;
  batchesDone: number;
  batchesTotal: number;
  sources: SourceProgress[];
  said: string;
  revisionId: string | null;
  startedAt: string;
  changedAt: string;
}

export interface ContextRevision {
  id: string;
  setId: string;
  draftRevision: number;
  app: ContextApp;
  requestedModel: string | null;
  model: string | null;
  createdAt: string;
  origin: 'human' | 'generated' | 'unknown';
  topics: ContextTopic[];
  findings: ContextFinding[];
  questions: string[];
  conflicts: string[];
  sources: SourceProgress[];
}

export interface ContextBuildRead {
  build: ContextBuild | null;
  revision: ContextRevision | null;
  buildWith: ContextApp;
}

export interface RevisionEdit {
  correction: string;
  findingId: string | null;
  text: string | null;
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
  list(archived: boolean): Promise<ContextSet[]>;
  read(id: string): Promise<ContextSetRead>;
  create(title: string): Promise<ContextSetRead>;
  saveDraft(edit: DraftEdit): Promise<ContextSetRead>;
  /** Kładzie w zestawie wszystko, co człowiek wybrał albo wkleił — jeden wynik na pozycję. */
  importSources(
    setId: string,
    operationId: string,
    items: ImportItem[],
    expectedRevision: string | null,
  ): Promise<ImportReport>;
  completePreparation(
    setId: string,
    sourceId: string,
    page: PreparedPage,
    expectedRevision: string | null,
  ): Promise<ContextSetRead>;
  readSource(setId: string, sourceId: string, page: number | null): Promise<SourcePart>;
  removeSource(
    setId: string,
    sourceId: string,
    expectedRevision: string | null,
  ): Promise<ContextSetRead>;
  buildContext(
    setId: string,
    operationId: string,
    app: ContextApp,
    model: string | null,
  ): Promise<ContextBuildRead>;
  readBuild(setId: string): Promise<ContextBuildRead>;
  stopBuild(setId: string, operationId: string): Promise<ContextBuild>;
  saveRevision(setId: string, edit: RevisionEdit): Promise<ContextBuildRead>;
  archive(setId: string, archived: boolean): Promise<ContextSet>;
  deleteSet(setId: string, confirmed: boolean): Promise<DeleteContextSet>;
}

/**
 * Co ten magazyn wie o KATALOGU zestawów — trzy stany, nie dwa.
 *
 * Pusta biblioteka i biblioteka, do której jeszcze nikt nie zajrzał, są w zustandzie tą samą
 * tablicą; ekran, który tego nie rozróżnia, mówi „nic tu nie ma" o folderze, którego nikt nie
 * otworzył. Ta sama nazwa i te same trzy wartości, co w `src/state/agents.ts`.
 */
export type Library = 'reading' | 'read' | 'unreadable';

/**
 * Otwarty dokument w rękach lokalnego workera.
 *
 * Interfejs, a nie import `pdfjs-dist`: magazyn nie ma prawa znać biblioteki, która rysuje
 * strony — tak samo, jak nie zna nazw komend. Prawdziwy sterownik stoi
 * w `src/sections/context/pdf-preparation.ts`, a test podaje własny.
 */
export interface OpenDocument {
  /** Ile stron ma ten plik. */
  readonly pages: number;
  /** Jedna strona: jej tekst i jej wygląd. Pamięć strony zwalnia sterownik zaraz po oddaniu. */
  page(number: number): Promise<Made>;
  /** Zwalnia dokument. Wołane także wtedy, gdy przygotowanie padło w połowie. */
  close(): void;
}

/**
 * Co wyszło z JEDNEJ strony.
 *
 * Ta sama unia, co przy otwieraniu, i z tego samego powodu: plik potrafi się otworzyć,
 * a rozsypać dopiero na trzeciej stronie — przy odczycie jej treści albo przy rysowaniu.
 * Wyjątek z tego miejsca kończył się zdaniem na ekranie, które znika przy wyjściu z sekcji,
 * a plik wracał jako „czeka na przygotowanie". Porażka strony jest porażką TEGO PLIKU.
 */
export type Made =
  { readonly made: { text: string; image: PastedBytes | null } } | { readonly failed: Unopenable };

/**
 * Co wyszło z próby otwarcia pliku.
 *
 * Wartość, nie wyjątek (niezmiennik 7): „tego pliku nie da się otworzyć" jest ODPOWIEDZIĄ,
 * którą trzeba zapisać na dysku, a nie awarią do zalogowania. Rzucony wyjątek kończyłby się
 * zdaniem na ekranie, które znika przy wyjściu z sekcji — a plik zostawałby na zawsze
 * „czekający na przygotowanie".
 */
export type Opened = { readonly opened: OpenDocument } | { readonly failed: Unopenable };

/** Kto umie otworzyć plik, którego bajty właśnie wróciły z biblioteki. */
export type PageMaker = (file: { mime: string; base64: string }) => Promise<Opened>;

export interface ContextState {
  sets: ContextSet[];
  library: Library;
  /** Zestaw otwarty w edytorze — `null`, kiedy człowiek stoi na liście. */
  open: ContextSetRead | null;
  /** Czego człowiek szuka na liście. Puste znaczy „wszystko". */
  search: string;
  archived: boolean;
  deleting: DeleteContextSet | null;
  /** Zdanie po odmowie z Rusta — `null`, kiedy nie ma o czym mówić. Jedno na całą sekcję. */
  refusal: string | null;
  /**
   * Wynik ostatniego importu, jeden wiersz na każdą wybraną pozycję.
   *
   * Lista, nie jedno zdanie: pięć plików, z których dwa odmówiły, jest pięcioma różnymi
   * odpowiedziami — a jedno zdanie „część plików nie weszła" każe zgadywać, które.
   */
  imported: ImportResult[];
  /** Otwarty podgląd: źródło i kawałek, który wrócił z granicy. */
  preview: { sourceId: string; part: SourcePart } | null;
  /** Źródło, które właśnie się przygotowuje — `null`, kiedy nic nie trwa. */
  preparing: string | null;
  /** Postęp zawsze pochodzi z zapisanego `state.json`, także po powrocie do sekcji. */
  build: ContextBuild | null;
  version: ContextRevision | null;
  buildWith: ContextApp;
  buildModel: string;
  load: () => Promise<void>;
  create: (title: string) => Promise<void>;
  openSet: (id: string) => Promise<void>;
  close: () => void;
  /** Zapisuje i oddaje `true`, kiedy naprawdę się zapisało. Ekran zostaje otwarty po odmowie. */
  save: (edit: DraftEdit) => Promise<boolean>;
  narrow: (search: string) => void;
  showArchived: (archived: boolean) => Promise<void>;
  archive: (setId: string, archived: boolean) => Promise<void>;
  previewDelete: (setId: string) => Promise<void>;
  cancelDelete: () => void;
  confirmDelete: () => Promise<void>;
  dismiss: () => void;
  /** Kładzie w otwartym zestawie wszystko, co człowiek wybrał albo wkleił. */
  addSources: (items: ImportItem[]) => Promise<void>;
  /** Przygotowuje dokument od pierwszej brakującej strony, jedną stroną naraz. */
  prepareSource: (sourceId: string, open: PageMaker) => Promise<void>;
  showSource: (sourceId: string, page: number | null) => Promise<void>;
  hidePreview: () => void;
  dropSource: (sourceId: string) => Promise<void>;
  chooseBuildWith: (app: ContextApp) => void;
  chooseBuildModel: (model: string) => void;
  startBuild: () => Promise<void>;
  readBuild: () => Promise<void>;
  stopBuild: () => Promise<void>;
  saveRevision: (edit: RevisionEdit) => Promise<void>;
}

/** Ile stron tego źródła jest już gotowych. Nieznany stan liczy się jak zero. */
export function pagesDone(source: ContextSource | undefined): number {
  const preparation = source?.preparation;
  return preparation?.state === 'needs' ? preparation.pagesDone : 0;
}

/** Zestawy, które pasują do tego, czego człowiek szuka. Po nazwie — tak mówi PLAN §12. */
export function matching<T extends Pick<ContextSet, 'title'>>(
  sets: readonly T[],
  search: string,
): T[] {
  const wanted = search.trim().toLowerCase();
  if (wanted === '') return [...sets];
  return sets.filter((set) => set.title.toLowerCase().includes(wanted));
}

/** Zdanie, którym panel kroku odmawia wybrania niezbudowanego zestawu.
 *
 * 2026-09-09 (UX-3) — TO SAMO ZDANIE MIESZKA W RUŚCIE, w `catalog_choice`
 * (`src-tauri/src/commands/workflow_context.rs`). Nie ma jak podać go tu drutem, bo sekcja
 * Context nie pyta panelu kroku o nic — więc jest kopią i **jedynym**, co ją trzyma, jest test
 * porównujący oba pliki bajt w bajt (`build-controls.test.tsx`). Bez tego testu dwa napisy
 * rozjeżdżają się przy pierwszej korekcie brzmienia i człowiek czyta dwa różne zdania o jednym
 * fakcie (niezmiennik 13). Nazwana stała, żeby test miał co porównać. */
export const BUILD_BEFORE_ADDING = 'Build this context before adding it to a workflow.';

/** Jedno zdanie prowadzące od materiału do wersji, którą można wybrać w workflow. */
export function whatIsNextForTheSet(set: ContextSet, draft: ContextDraft): string {
  if (draft.sources.length === 0) return 'Add material to this set before it can be built.';
  if (set.latestReadyRevision === null) return BUILD_BEFORE_ADDING;
  return 'This context is built, so a workflow step can add it.';
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
    archived: false,
    deleting: null,
    refusal: null,
    imported: [],
    preview: null,
    preparing: null,
    build: null,
    version: null,
    buildWith: 'claude-code',
    buildModel: '',

    load: async () => {
      set({ refusal: null, library: 'reading' });
      try {
        set({ sets: await io.list(get().archived), library: 'read' });
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
        set({
          sets: upsert(get().sets, made.set),
          open: made,
          build: null,
          version: null,
        });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not make that context set.') });
      }
    },

    openSet: async (id: string) => {
      set({ refusal: null });
      try {
        const open = await io.read(id);
        set({ open });
        try {
          const view = await io.readBuild(id);
          set({ build: view.build, version: view.revision, buildWith: view.buildWith });
        } catch (error) {
          set({ refusal: why(error, 'Loadout could not read the saved build for this set.') });
        }
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not open that context set.') });
      }
    },

    close: () => {
      /* Wyniki importu i podgląd należą do OTWARTEGO zestawu, więc wychodzą razem z nim.
       * Zostawione, opowiadałyby o plikach następnego zestawu, do którego ktoś wejdzie. */
      set({
        open: null,
        refusal: null,
        imported: [],
        preview: null,
        build: null,
        version: null,
      });
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

    showArchived: async (archived: boolean) => {
      set({ archived, deleting: null });
      await get().load();
    },

    archive: async (setId: string, archived: boolean) => {
      set({ refusal: null, deleting: null });
      try {
        await io.archive(setId, archived);
        set({ sets: get().sets.filter((one) => one.id !== setId) });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not move that context set.') });
      }
    },

    previewDelete: async (setId: string) => {
      set({ refusal: null, deleting: null });
      try {
        set({ deleting: await io.deleteSet(setId, false) });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not check where that context set is used.') });
      }
    },

    cancelDelete: () => {
      set({ deleting: null });
    },

    confirmDelete: async () => {
      const deleting = get().deleting;
      if (deleting === null) return;
      set({ refusal: null });
      try {
        const deleted = await io.deleteSet(deleting.setId, true);
        if (!deleted.deleted) {
          throw new Error('Loadout did not confirm that the context set was deleted.');
        }
        set({
          sets: get().sets.filter((one) => one.id !== deleting.setId),
          deleting: null,
        });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not delete that context set.') });
      }
    },

    dismiss: () => {
      set({ refusal: null });
    },

    addSources: async (items: ImportItem[]) => {
      const open = get().open;
      if (open === null || items.length === 0) return;
      set({ refusal: null, imported: [] });
      try {
        /* Numer operacji wybija OKNO i wysyła go razem z żądaniem. Po nim rozlicza się wynik,
         * także spóźniony: odpowiedź na poprzednie kliknięcie nie ma prawa wylądować w tym,
         * co człowiek zlecił przed chwilą. */
        const report = await io.importSources(
          open.set.id,
          crypto.randomUUID(),
          items,
          open.revision,
        );
        set({
          open: report.read,
          imported: report.results,
          sets: upsert(get().sets, report.read.set),
        });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not add those files to this set.') });
      }
    },

    prepareSource: async (sourceId: string, openDocument: PageMaker) => {
      const open = get().open;
      if (open === null) return;
      set({ refusal: null, preparing: sourceId });
      try {
        const whole = await io.readSource(open.set.id, sourceId, null);
        if (whole.kind !== 'whole') {
          throw new Error('Loadout could not read that file to prepare it.');
        }
        /* PLIK, KTÓRY SIĘ NIE PRZYGOTOWAŁ, ZOSTAJE ZAPISANY JAKO TAKI — obojętnie, czy odbił
           się przy otwarciu, czy dopiero na trzeciej stronie. Samo zdanie na ekranie znika
           przy wyjściu z sekcji, a źródło wracałoby jako „czeka na przygotowanie", więc
           człowiek klikałby Prepare bez końca i nigdy nie dowiedziałby się, czemu. Zdanie pisze
           Rust, bo to on je pokaże następnym razem (PLAN §5). */
        const givingUp = async (failed: Unopenable): Promise<void> => {
          set({
            open: await io.completePreparation(
              open.set.id,
              sourceId,
              {
                operationId: whole.operationId,
                fingerprint: whole.fingerprint,
                pagesTotal: 0,
                number: 0,
                text: '',
                image: null,
                failed,
              },
              get().open?.revision ?? null,
            ),
          });
        };

        const answer = await openDocument({ mime: whole.mime, base64: whole.base64 });
        if ('failed' in answer) {
          await givingUp(answer.failed);
          return;
        }
        const document = answer.opened;
        try {
          /* OD PIERWSZEJ BRAKUJĄCEJ, nie od początku. Strony gotowe przed zamknięciem okna
           * zostały na dysku, a przygotowanie ich drugi raz jest czekaniem bez powodu. */
          const from = pagesDone(open.draft.sources.find((source) => source.id === sourceId)) + 1;
          for (let number = from; number <= document.pages; number += 1) {
            const page = await document.page(number);
            if ('failed' in page) {
              /* Strony gotowe przed tą ZOSTAJĄ na dysku: są prawdziwe i przygotowane raz.
                 Stan mówi o pliku, a nie o nich. */
              await givingUp(page.failed);
              return;
            }
            const saved = await io.completePreparation(
              open.set.id,
              sourceId,
              {
                operationId: whole.operationId,
                fingerprint: whole.fingerprint,
                pagesTotal: document.pages,
                number,
                text: page.made.text,
                image: page.made.image,
              },
              get().open?.revision ?? null,
            );
            set({ open: saved });
          }
        } finally {
          document.close();
        }
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not prepare that file.') });
      } finally {
        set({ preparing: null });
      }
    },

    showSource: async (sourceId: string, page: number | null) => {
      const open = get().open;
      if (open === null) return;
      set({ refusal: null });
      try {
        set({ preview: { sourceId, part: await io.readSource(open.set.id, sourceId, page) } });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not open that file.') });
      }
    },

    hidePreview: () => {
      set({ preview: null });
    },

    dropSource: async (sourceId: string) => {
      const open = get().open;
      if (open === null) return;
      set({ refusal: null });
      try {
        const saved = await io.removeSource(open.set.id, sourceId, open.revision);
        /* Podgląd znika RAZEM ze źródłem: obrazek stojący nad wierszem, którego już nie ma,
         * jest pokazywaniem czegoś, czego zestaw nie zawiera (niezmiennik 13). */
        set({
          open: saved,
          preview: get().preview?.sourceId === sourceId ? null : get().preview,
          imported: [],
        });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not take that file out of this set.') });
      }
    },

    chooseBuildWith: (app: ContextApp) => {
      set({ buildWith: app });
    },

    chooseBuildModel: (model: string) => {
      set({ buildModel: model });
    },

    startBuild: async () => {
      const open = get().open;
      if (open === null) return;
      const operationId = crypto.randomUUID();
      const app = get().buildWith;
      const model = get().buildModel.trim();
      set({
        refusal: null,
        build: {
          operationId,
          setId: open.set.id,
          generation: 0,
          draftRevision: open.set.draftRevision,
          stage: 'freezing',
          end: 'running',
          app,
          requestedModel: model === '' ? null : model,
          model: null,
          batchesDone: 0,
          batchesTotal: 0,
          sources: [],
          said: 'Loadout is freezing the material for this build.',
          revisionId: null,
          startedAt: '',
          changedAt: '',
        },
      });

      /* 2026-09-08 (CT-04) — wywołanie budowania odpowiada dopiero po końcu, więc pasek czyta
       * zapisany stan równolegle. Bez tego żywa operacja istnieje, ale ekran stoi na pierwszym
       * zdaniu aż do publikacji. */
      let done = false;
      const refresh = (): void => {
        void io
          .readBuild(open.set.id)
          .then((view) => {
            if (get().build?.operationId === operationId) {
              set({ build: view.build, version: view.revision, buildWith: view.buildWith });
            }
          })
          .catch(() => undefined);
      };
      const timer = globalThis.setInterval(() => {
        if (!done) refresh();
      }, 250);
      try {
        const view = await io.buildContext(
          open.set.id,
          operationId,
          app,
          model === '' ? null : model,
        );
        const reopened = await io.read(open.set.id);
        set({
          build: view.build,
          version: view.revision,
          buildWith: view.buildWith,
          open: reopened,
          sets: upsert(get().sets, reopened.set),
        });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not build this context.') });
        refresh();
      } finally {
        done = true;
        globalThis.clearInterval(timer);
      }
    },

    readBuild: async () => {
      const open = get().open;
      if (open === null) return;
      try {
        const view = await io.readBuild(open.set.id);
        set({ build: view.build, version: view.revision, buildWith: view.buildWith });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not read the saved build for this set.') });
      }
    },

    stopBuild: async () => {
      const open = get().open;
      const build = get().build;
      if (
        open === null ||
        build === null ||
        (build.end !== 'running' && build.end !== 'stillRunning')
      )
        return;
      set({ refusal: null });
      try {
        set({ build: await io.stopBuild(open.set.id, build.operationId) });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not stop this context build.') });
      }
    },

    saveRevision: async (edit: RevisionEdit) => {
      const open = get().open;
      if (open === null) return;
      set({ refusal: null });
      try {
        const view = await io.saveRevision(open.set.id, edit);
        const reopened = await io.read(open.set.id);
        set({
          build: view.build,
          version: view.revision,
          buildWith: view.buildWith,
          open: reopened,
          sets: upsert(get().sets, reopened.set),
        });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not save that context version.') });
      }
    },
  }));
}
