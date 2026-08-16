/* Magazyn LISTY workflow: spis plików w katalogu, a nie jeden otwarty plik.
 *
 * To jest inny magazyn niż `src/state/workflows.ts` (T-13) i celowo o tym nie wie. Tamten
 * trzyma JEDEN otwarty dokument — kroki, cofnij/ponów, autosave. Ten trzyma KATALOG:
 * co leży na dysku, pod jakimi nazwami plików, i nic ponadto. Zlanie ich w jeden magazyn
 * daje stan, w którym „lista" i „otwarty plik" mówią co innego o tej samej nazwie.
 *
 * DLACZEGO TYPY SĄ TUTAJ, A NIE IMPORTOWANE. `src/state/workflows.ts` należy do T-13 i na
 * dzień pisania tego pliku jeszcze nie istnieje. Import nieistniejącego modułu daje
 * „Cannot find module", czyli czerwień, której bramka nie liczy (AGENTS.md §2a) — kryterium
 * nie sprawdziłoby wtedy niczego. Poniżej jest więc WĘŻSZE lustro schematu z T3 §3.1:
 * dokładnie te pola, które lista czyta. Kiedy T-13 wyląduje, te dwa opisy schematu mają
 * zostać zredukowane do jednego — to jest zadanie dla człowieka, nie cicha decyzja tego pliku.
 *
 * Wszystko, co ten magazyn robi poza swoją głową, idzie przez wstrzyknięte `WorkflowListIo`
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii). Test wstrzykuje atrapę
 * jako jawny argument, więc nie ma tu żadnej warstwy transportu do zaślepienia.
 */
import { create } from 'zustand';

/** Strzałka „po tym kroku". Bez portów, bez danych, bez warunku [T3 §3.1]. */
export interface Link {
  from: string;
  to: string;
}

/**
 * Krok tak, jak widzi go LISTA — tyle, ile trzeba, żeby policzyć kroki i różnych agentów.
 * Pełny schemat kroku (instrukcje, `copies`, `at`, folder, handover) mieszka na płótnie [T3 §3.1].
 */
export interface Step {
  kind: 'agent' | 'checkpoint';
  id: string;
  name: string;
  /** Identyfikator zapisanego agenta. Krok rodzaju `checkpoint` go nie ma. */
  agent?: string;
}

/** Plik workflow. `format` rośnie tylko przy zmianie łamiącej [T3 §3.1, §8.4]. */
export interface WorkflowFile {
  format: 1;
  /** Stabilny i NIGDY nie zmienia się przy zmianie nazwy [T3 §3.1]. */
  id: string;
  /** To, co wpisał człowiek. Nazwa pliku powstaje z niej raz i potem żyje osobno. */
  name: string;
  description?: string;
  steps: Step[];
  links: Link[];
}

/** Jedna pozycja listy: plik i jego nazwa na dysku. */
export interface WorkflowEntry {
  /**
   * Nazwa pliku w katalogu workflow, np. `ship-a-feature.json`. Bez katalogu i bez `~`:
   * ścieżkę rozwiązuje JEDNA funkcja po stronie Rusta (`directories`, nie sklejanie `$HOME`
   * — T3 §8.3), a frontend, który dokleja katalog sam, jest drugim miejscem, w którym
   * mieszka odpowiedź na pytanie „gdzie to leży".
   *
   * Powstaje raz, przy tworzeniu, i nigdy się nie zmienia — zmiana nazwy workflow zmienia
   * pole `name` i zostawia plik tam, gdzie był. Przemianowywanie plików potrafi zgubić dane
   * i nic za to nie kupuje.
   */
  path: string;
  workflow: WorkflowFile;
}

/** Cały styk z dyskiem. Jedna atrapa w teście zastępuje całość. */
export interface WorkflowListIo {
  /** Wszystko, co leży w katalogu workflow, każdy plik ze swoją nazwą. */
  list(): Promise<WorkflowEntry[]>;
  /** uuid v7, mennica stoi po stronie Rusta [T4 §5.1] — `crypto.randomUUID()` daje v4. */
  newId(): Promise<string>;
  write(path: string, workflow: WorkflowFile): Promise<void>;
  remove(path: string): Promise<void>;
}

/**
 * Wszystko, co ekran listy umie zrobić. JEDEN obiekt: przycisk w pustym stanie i przycisk
 * w nagłówku dostają ten sam `create`, bo drugi przepływ tworzenia to drugie miejsce,
 * w którym powstaje plik, i pierwsza okazja do rozjazdu (niezmiennik 16).
 */
export interface WorkflowListActions {
  create: (name: string) => Promise<void>;
  duplicate: (id: string) => Promise<void>;
  requestDelete: (id: string) => void;
  cancelDelete: () => void;
  confirmDelete: () => Promise<void>;
}

export interface WorkflowListState extends WorkflowListActions {
  /** Posortowane po nazwie, bez uwzględnienia wielkości liter. Licznik w nagłówku to `.length`. */
  workflows: WorkflowEntry[];
  /** O co pytamy. `null` znaczy, że o nic — pytanie ma jedno miejsce (niezmiennik 13). */
  pendingDeleteId: string | null;
  load: () => Promise<void>;
}

/* ── Nazwa pliku ───────────────────────────────────────────────────────────────────────────
 *
 * Powstaje z nazwy workflow RAZ, przy tworzeniu, i potem żyje osobno. Cała pułapka tego
 * ekranu mieszka w tych trzydziestu liniach: `Ship a feature` i `Ship a Feature` dają ten sam
 * slug, a zapis pliku po prostu nadpisuje. Drugi zapis kończy się wtedy sukcesem, lista
 * pokazuje dwie pozycje, na dysku jest jedna — i użytkownik traci workflow, którego nigdy nie
 * usuwał, a dowiaduje się o tym dopiero po restarcie.
 */

/**
 * Kiedy z nazwy nie zostaje ani jeden znak, który da się zapisać w nazwie pliku.
 * `"???"` daje pusty slug, a samo `.json` to plik ukryty w Uniksie i nazwa, której nie da się
 * ani pokazać, ani powiązać z workflow.
 */
const FALLBACK_SLUG = 'workflow';

/**
 * Sufit długości sluga. Nazwa workflow jest zdaniem po ludzku, a nie identyfikatorem — wklejony
 * akapit daje nazwę pliku dłuższą niż 255 bajtów, czyli ENAMETOOLONG przy zapisie: błąd na
 * dysku zamiast brzydkiej nazwy na ekranie. Sześćdziesiąt znaków mieści każde sensowne zdanie
 * i zostawia miejsce na `-2.json`.
 */
const MAX_SLUG = 60;

/** `Ship a Feature` → `ship-a-feature`. Wielkość liter ginie i to jest cały problem. */
function slugOf(name: string): string {
  /* NFD plus skasowanie znaków łączących: `Wdrożenie` daje `wdrozenie`, nie `wdro-enie`.
   * Nazwę wpisuje człowiek, więc bywa w dowolnym języku; nazwa pliku ma zostać w ASCII, bo
   * jedzie przez FFI, przez `git`, przez archiwum i przez cudzy system plików. */
  const ascii = name.normalize('NFD').replace(/\p{M}+/gu, '');
  const hyphenated = ascii
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .slice(0, MAX_SLUG);
  /* Obcięcie do sufitu potrafi skończyć się na łączniku, więc przycinamy PO nim. */
  const trimmed = hyphenated.replace(/^-+|-+$/g, '');
  return trimmed === '' ? FALLBACK_SLUG : trimmed;
}

/**
 * Pierwsza wolna nazwa pliku dla tej nazwy workflow, szukana wobec TEGO, CO LEŻY W KATALOGU.
 *
 * Sufiks jest liczbą od 2, więc drugi `Ship a feature` ląduje obok pierwszego jako
 * `ship-a-feature-2.json`, a nie na nim.
 */
function freeFileName(name: string, taken: readonly WorkflowEntry[]): string {
  /* Porównanie po małych literach, bo APFS jest domyślnie NIEwrażliwy na wielkość liter:
   * `Ship-A-Feature.json` i `ship-a-feature.json` to na tym dysku jeden plik. Slug jest już
   * mały, więc wystarczy sprowadzić do małych to, co przyszło z katalogu. */
  const used = new Set(taken.map((entry) => entry.path.toLowerCase()));
  const base = slugOf(name);

  let candidate = `${base}.json`;
  let ordinal = 1;
  while (used.has(candidate)) {
    ordinal += 1;
    candidate = `${base}-${ordinal}.json`;
  }
  return candidate;
}

/**
 * `apple` stoi przed `Banana`.
 *
 * Domyślne `Array.sort()` porównuje kody znaków, a wielkie litery mają niższe — daje
 * `['Banana', 'apple']`, czyli listę, która czyta się jak nieposortowana. `sensitivity: 'accent'`
 * znaczy: wielkość liter nie ma znaczenia, znaki diakrytyczne mają (`resume` ≠ `résumé`).
 */
const byName = new Intl.Collator(undefined, { sensitivity: 'accent' });

/** Sortowanie należy do LISTY, nie do ścieżki tworzenia. Katalog wydaje pliki w swojej kolejności. */
function sortedByName(entries: readonly WorkflowEntry[]): WorkflowEntry[] {
  return [...entries].sort((a, b) => byName.compare(a.workflow.name, b.workflow.name));
}

/** Nazwa, którą dostaje świeżo utworzony workflow. */
const NEW_WORKFLOW_NAME = 'New workflow';

/**
 * Nazwa dla workflow, który właśnie powstaje: `New workflow`, potem `New workflow 2`.
 *
 * Ekran nie pyta o nazwę — makieta prowadzi z `＋ Create` prosto na puste płótno
 * (`docs/mockup/index.html:651-652`), a nazwa jest edytowalna tam [T-13]. Numer jest
 * uprzejmością, nie niezmiennikiem: dwa workflow wolno nazwać tak samo, bo tożsamością jest
 * `id`, a nie nazwa (i nie nazwa pliku). Bez numeru trzy kliknięcia dają trzy wiersze,
 * których nie da się od siebie odróżnić.
 */
export function newWorkflowName(entries: readonly WorkflowEntry[]): string {
  const taken = new Set(entries.map((entry) => entry.workflow.name));
  if (!taken.has(NEW_WORKFLOW_NAME)) {
    return NEW_WORKFLOW_NAME;
  }

  let ordinal = 2;
  while (taken.has(`${NEW_WORKFLOW_NAME} ${ordinal}`)) {
    ordinal += 1;
  }
  return `${NEW_WORKFLOW_NAME} ${ordinal}`;
}

export function createWorkflowListStore(io: WorkflowListIo) {
  return create<WorkflowListState>()((set, get) => ({
    workflows: [],
    pendingDeleteId: null,

    load: async () => {
      set({ workflows: sortedByName(await io.list()) });
    },

    create: async (name) => {
      /* Katalog czytamy TERAZ, tuż przed wyborem nazwy pliku, zamiast ufać temu, co ekran
       * pokazuje. Lista w głowie ekranu jest z chwili, w której ktoś ją wczytał; plik mógł
       * w międzyczasie powstać w drugim oknie albo w Finderze, a wolna nazwa wybrana wobec
       * nieaktualnego spisu to dokładnie ten cichy zapis na cudzym pliku, przed którym stoi
       * całe to sprawdzanie unikalności. */
      const onDisk = await io.list();
      const path = freeFileName(name, onDisk);
      const workflow: WorkflowFile = {
        format: 1,
        id: await io.newId(),
        /* Dokładnie to, co wpisał człowiek — nazwa pliku jest z niej wyprowadzona raz,
         * a sama nazwa nigdy nie jest spłaszczana pod nazwę pliku. */
        name,
        steps: [],
        links: [],
      };

      await io.write(path, workflow);
      set({ workflows: sortedByName([...onDisk, { path, workflow }]) });
    },

    duplicate: async (id) => {
      /* Ten sam odczyt katalogu i z tego samego powodu, co w `create`: duplikat jest nowym
       * plikiem, więc też musi trafić na wolną nazwę. */
      const onDisk = await io.list();
      const source = onDisk.find((entry) => entry.workflow.id === id);
      if (source === undefined) {
        /* Plik zniknął spod ekranu — usunięty w drugim oknie albo w Finderze. Duplikowanie
         * wiersza, którego nie ma na dysku, wskrzesiłoby workflow, który ktoś przed chwilą
         * usunął; ekran ma się zamiast tego zgodzić z katalogiem (niezmiennik 4). */
        set({ workflows: sortedByName(onDisk) });
        return;
      }

      const name = `${source.workflow.name} (copy)`;
      const copy: WorkflowFile = {
        /* `structuredClone`, nie `{ ...wf }`. Rozłożenie obiektu kopiuje ODWOŁANIA do tablic
         * `steps` i `links`, więc pierwsza edycja duplikatu po cichu przepisuje oryginał,
         * na którym użytkownik pracuje od miesiąca — i widać to dopiero po biegu, który zrobił
         * co innego, niż mówi ekran. */
        ...structuredClone(source.workflow),
        /* Własne `id`: dwie pozycje z jednym `id` to jeden workflow wypisany dwa razy,
         * a wygrywa ta, którą zapisano później. `id` oryginału zostaje nietknięte [T3 §3.1]. */
        id: await io.newId(),
        name,
      };
      /* Identyfikatory kroków zostają takie, jakie były. Są lokalne dla pliku, a `links`
       * wskazują na nie po tych identyfikatorach — świeżo wybite `id` kroków bez przepisania
       * strzałek dałyby kopię podłączoną do niczego. */

      const path = freeFileName(name, onDisk);
      await io.write(path, copy);
      set({ workflows: sortedByName([...onDisk, { path, workflow: copy }]) });
    },

    requestDelete: (id) => {
      /* Pytanie i nic poza pytaniem. Usunięcie pliku PRZED jego pokazaniem robi z pytania
       * ozdobę, a z `Cancel` kłamstwo — i przechodzi każde sprawdzenie, które pyta tylko o to,
       * czy słowo `Delete` jest na ekranie (niezmiennik 20). */
      set({ pendingDeleteId: id });
    },

    cancelDelete: () => {
      set({ pendingDeleteId: null });
    },

    confirmDelete: async () => {
      const id = get().pendingDeleteId;
      if (id === null) {
        return;
      }

      /* Ścieżkę bierzemy z listy, a nie ze świeżego odczytu katalogu — inaczej niż w `create`
       * i `duplicate`. Tam katalog rozstrzygał, która nazwa jest WOLNA, więc musiał być
       * aktualny. Tu pytanie brzmi „na który wiersz wskazał człowiek", a na to odpowiada
       * ekran, na którym ten wiersz stał. */
      const target = get().workflows.find((entry) => entry.workflow.id === id);
      if (target === undefined) {
        set({ pendingDeleteId: null });
        return;
      }

      /* Najpierw plik, potem lista. Skreślenie pozycji z listy bez usunięcia pliku daje stan,
       * który wraca po restarcie (niezmiennik 4); a kiedy `io.remove` się nie uda, pozycja ma
       * zostać na ekranie, bo plik został na dysku. */
      await io.remove(target.path);
      set({
        workflows: get().workflows.filter((entry) => entry.workflow.id !== id),
        pendingDeleteId: null,
      });
    },
  }));
}
