/* Otwarty dokument workflow: `WorkflowFile` w pamięci plus akcje, które go zmieniają.
 *
 * SZKIELET. Każda akcja rzuca `not implemented` — to jest odpowiednik `todo!()` z Rusta
 * (AGENTS.md §2a). Warstwa `before` ma paść na braku ZACHOWANIA, więc moduł musi się wczytać
 * i wystawić prawdziwe sygnatury; implementacja podmienia ciała, nie nagłówki.
 *
 * Ten plik NIE importuje `@/ipc`. Nazwy komend zna jedno miejsce w sekcji —
 * `src/sections/workflows/canvas/io.ts` — i to ono wstrzykuje tu `WorkflowIo` (niezmiennik 23),
 * dokładnie tak jak `AgentsIo` w `src/state/agents.ts`. Test wstrzykuje atrapę zamiast mockować
 * transport, którego magazyn i tak nie widzi.
 *
 * Dlaczego `saveAgent` siedzi w `WorkflowIo`, choć to sekcja workflow: panel kroku ma w liście
 * „Who does this" pozycję `＋ Create a new agent…` (`docs/mockup/index.html:603`), więc ta sekcja
 * NAPRAWDĘ umie zapisać plik agenta. To jest jedyny powód, dla którego zdanie „edycja kroku nie
 * dotyka agenta" da się w ogóle udowodnić: `expect(io.saveAgent).not.toHaveBeenCalled()` na
 * funkcji, której w interfejsie nie ma, nie dowodzi niczego.
 *
 * Typy niżej są lustrem `src-tauri/src/workflow/mod.rs` — tak samo jak typy w
 * `src/state/agents.ts` są lustrem `src-tauri/src/library/agents.rs`. Dopóki nie ma generatora
 * (`ts-rs` albo `specta`, T3 §3.2), obie kopie stoją obok siebie i rozjazd łapie recenzja.
 *
 * Czego tu świadomie NIE MA: cofnij/ponów. PLAN §7 stawia je w v1.1, a TASK.md mówi „magazyn ma
 * zostawić na to miejsce, nie implementować" — tym miejscem jest `commit`, jedyna droga, którą
 * nowy dokument wchodzi do stanu.
 */
import { create } from 'zustand';
import type { Agent } from './agents';

/** Waga uwagi z walidatora Rusta. `Problem` blokuje Run, `Warning` nie blokuje niczego. */
export type Level = 'problem' | 'warning';

/** Jedna uwaga o jednym defekcie — lustro `workflow::check::Note`.
 *
 * `message` idzie WPROST na ekran: to jest gotowe angielskie zdanie, nie klucz i nie kod. */
export interface Note {
  level: Level;
  /** Krok, na którym ląduje kropka. `null`, kiedy uwaga dotyczy całego pliku. */
  stepId: string | null;
  message: string;
}

/** Pozycja kafelka. Zapisywana zawsze jako całkowita wielokrotność [`GRID`]. */
export interface Point {
  x: number;
  y: number;
}

/** Skok siatki w pikselach [T3 §8.2 reguła 1]. Ta sama liczba co `workflow::GRID`. */
export const GRID = 24;

/** Strzałka. Bez portów, bez danych, bez warunku — znaczy „po" (T3 §3.1). */
export interface Link {
  from: string;
  to: string;
}

export type Folder = { use: 'project' } | { use: 'fresh-copy' } | { use: 'pick'; path: string };

/** `'all'` albo lista nazw. Lustro `workflow::Skills`. */
export type Skills = 'all' | string[];

/** Co kontrolka Skills umie zapisać. `'none'` istnieje tylko w trybie `all-or-none`. */
export type SkillChoice = 'all' | 'none' | { only: string[] };

export interface HandoverField {
  name: string;
  describe: string;
  required?: boolean;
}

export type Handover = 'notes' | { fields: HandoverField[] };

/** Dziewięć pól, które krok może zmienić wobec agenta — lustro `OVERRIDABLE` z T-11.
 *
 * Czego tu nie ma: `id`, `name` i `runsWith`. Krok, który przestawia vendora, unieważnia
 * połowę reszty [T4 §6.4]. */
export type OverridableField =
  | 'instructions'
  | 'model'
  | 'thinking'
  | 'fileAccess'
  | 'giveUpAfterMinutes'
  | 'tools'
  | 'skills'
  | 'connections'
  | 'writeResultsTo';

/** Patch RFC 7396 nad definicją agenta: brak klucza znaczy „weź z agenta" [T4 §5.1].
 *
 * `{}` dla kroku nietkniętego — i to `{}` niesie informację, więc nie znika przy zapisie. */
export type Overrides = Partial<Pick<Agent, OverridableField>>;

/** Krok, który uruchamia agenta.
 *
 * Vendora ani modelu tu nie ma: krok nazywa AGENTA, a vendor, model i narzędzia mieszkają
 * w jego definicji (T3 §3.1). Zmiana modelu dzieje się raz, nie w sześciu kafelkach. */
export interface AgentStep {
  kind: 'agent';
  id: string;
  name: string;
  /** Id zapisanego agenta (`src/state/agents.ts`). */
  agent: string;
  overrides: Overrides;
  /** Przelotka na opcje vendora. Loadout nie interpretuje jej zawartości. */
  vendorOptions?: Record<string, Record<string, string>>;
  /** Ile identycznych kopii naraz, 1–8 [T3 §4.4]. */
  copies: number;
  /** Prompt kroku, zwykły tekst. To NIE jest `Overrides.instructions`, które dotyczy agenta. */
  instructions: string;
  skills: Skills;
  folder: Folder;
  handover: Handover;
  at: Point;
}

/** Krok, który zatrzymuje bieg i pyta człowieka [T3 §6.1 punkt 5]. */
export interface CheckpointStep {
  kind: 'checkpoint';
  id: string;
  name: string;
  question?: string;
  at: Point;
}

/** Dwa rodzaje kafelka. To jest cała lista i ma taka zostać (D6, TASK.md rozstrzygnięcie 1). */
export type Step = AgentStep | CheckpointStep;

export interface WorkflowFile {
  format: 1;
  id: string;
  name: string;
  description?: string;
  /** Kolejność WSTAWIANIA, nigdy przesortowana [T3 §8.2 reguła 2]. */
  steps: Step[];
  links: Link[];
}

/** Wszystko, co magazyn robi poza swoją głową. Jedna atrapa w teście zastępuje całość. */
export interface WorkflowIo {
  /** Zapis pliku workflow. Odmowa przy problemie żyje po stronie Rusta (`workflow::file::save`). */
  save(file: WorkflowFile): Promise<void>;
  /** Uwagi z walidatora Rusta (T-12). Frontend ich nie liczy i nie tłumaczy. */
  check(file: WorkflowFile): Promise<Note[]>;
  /** Zapis pliku AGENTA — patrz nagłówek pliku. Edycja kroku nie ma prawa tego zawołać. */
  saveAgent(agent: Agent): Promise<void>;
}

export interface WorkflowState {
  /** Otwarty dokument. Magazyn bez dokumentu nie ma sensu, więc nie ma tu `null`. */
  document: WorkflowFile;
  /** Ostatnie uwagi z Rusta. Frontend ich nie wymyśla. */
  notes: Note[];
  /** Jedyna droga, którą nowy dokument wchodzi do stanu — i miejsce na stos cofnij/ponów. */
  commit: (next: WorkflowFile) => void;
  /** Odświeża uwagi. Wołane po zapisie i przed Run. */
  recheck: () => Promise<void>;
  /** Zmiana wiersza panelu, wyrażona wartościami EFEKTYWNYMI. Różnicę liczy `applyPanelEdit`. */
  editStep: (stepId: string, agent: Agent, edit: Overrides) => void;
  /** `Reset` przy jednym wierszu: kasuje jeden klucz patcha i tylko jeden. */
  resetRow: (stepId: string, field: OverridableField) => void;
  chooseSkills: (stepId: string, choice: SkillChoice) => void;
}

/** Magazyn otwartego dokumentu.
 *
 * Drugi argument jest wymagany, bo „otwarty dokument" bez dokumentu to stan, którego ten ekran
 * nie ma — listę plików workflow i ich otwieranie posiada T-14. */
export function createWorkflowStore(_io: WorkflowIo, open: WorkflowFile) {
  return create<WorkflowState>()(() => ({
    document: open,
    notes: [],

    commit: () => {
      throw new Error('not implemented');
    },

    recheck: () => {
      throw new Error('not implemented');
    },

    editStep: () => {
      throw new Error('not implemented');
    },

    resetRow: () => {
      throw new Error('not implemented');
    },

    chooseSkills: () => {
      throw new Error('not implemented');
    },
  }));
}
