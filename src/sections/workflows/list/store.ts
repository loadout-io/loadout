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

/* Faza kontraktu: sygnatury stoją, zachowania jeszcze nie ma. To jest odpowiednik `todo!()`
 * z Rusta i istnieje po to, żeby import się rozwiązał, a kryterium padło NA ASERCJI, a nie
 * przy wczytywaniu modułu — „Cannot find module" jest na liście `NOT_A_REAL_RED` i nie
 * dowodzi niczego (AGENTS.md §2a). Faza implementacji kasuje każde z tych wywołań. */
function notImplemented(): never {
  throw new Error('not implemented');
}

export function createWorkflowListStore(io: WorkflowListIo) {
  /* `io` jest kontraktem tej funkcji i wraca w fazie implementacji. `void` zamiast nazwania
   * go `_io`: podkreślenie przemianowałoby publiczny parametr, a `noUnusedParameters` daje
   * TS6133, które checks/quick-types.sh klasyfikuje jako NASZĄ złą konfigurację (exit 2),
   * czyli czerwień trafiającą nie tam, gdzie trzeba. */
  void io;

  return create<WorkflowListState>()(() => ({
    workflows: [],
    pendingDeleteId: null,

    load: notImplemented,
    create: notImplemented,
    duplicate: notImplemented,
    requestDelete: notImplemented,
    cancelDelete: notImplemented,
    confirmDelete: notImplemented,
  }));
}
