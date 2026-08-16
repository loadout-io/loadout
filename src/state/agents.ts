/* Magazyn sekcji Agents.
 *
 * Ten plik NIE importuje `@/ipc`. Nazwy komend zna jedno miejsce w sekcji —
 * `src/sections/agents/io.ts` — i to ono wstrzykuje tu `AgentsIo` (niezmiennik 23: polityka
 * w jednym rdzeniu, adaptery po pięć linii). Test wstrzykuje atrapę zamiast mockować
 * transport, więc nie ma jak sprawdzić magazynu przez zaślepienie warstwy, której magazyn
 * i tak nie widzi.
 *
 * Dlaczego identyfikator przychodzi z `AgentsIo`, a nie z `crypto.randomUUID()`: to ma być
 * uuid v7, czyli sortowalny po czasie [T4 §5.1], a `randomUUID` daje v4. Mennica stoi po
 * stronie Rusta, gdzie uuid v7 już jest.
 *
 * Typy niżej są lustrem `src-tauri/src/library/agents.rs`. Dopóki nie ma generatora
 * (`ts-rs` albo `specta` — T4 §7.2), rozjazd łapie kryterium 1 po stronie Rusta: ono zamraża
 * te same piętnaście kluczy.
 */
import { create } from 'zustand';

export type Vendor = 'claude-code' | 'codex';
export type Thinking = 'quick' | 'balanced' | 'deep' | 'deepest';
export type FileAccess = 'look-only' | 'ask-first' | 'work-freely';
export type Tools = 'everything' | { only: string[] };

/** Pięć przygaszonych tokenów tożsamości, `--id-1`…`--id-5` (DESIGN §3). */
export type Color = 'slate' | 'plum' | 'clay' | 'moss' | 'rose';

export interface Agent {
  schema: 1;
  id: string;
  name: string;
  summary: string;
  color: Color;
  instructions: string;
  runsWith: Vendor;
  model: string;
  thinking: Thinking;
  /** Etykieta: `Can it change files`. */
  fileAccess: FileAccess;
  /** `0` znaczy „bez limitu". Nigdy `null` — w RFC 7396 `null` kasuje klucz. */
  giveUpAfterMinutes: number;
  tools: Tools;
  skills: string[];
  /** Etykieta: `Connections`. */
  connections: string[];
  writeResultsTo: string;
  /** Przelotka D6. Nieobecna, kiedy pusta — pusta mapa nie ma prawa dokładać klucza. */
  vendorOptions?: Record<string, Record<string, string>>;
}

/** Wszystko, co magazyn robi poza swoją głową. Jedna atrapa w teście zastępuje całość. */
export interface AgentsIo {
  list(): Promise<Agent[]>;
  /** uuid v7, minted po stronie Rusta. */
  newId(): Promise<string>;
  save(agent: Agent): Promise<void>;
  remove(id: string): Promise<void>;
}

export interface AgentsState {
  agents: Agent[];
  load: () => Promise<void>;
  duplicate: (id: string) => Promise<void>;
  delete: (id: string) => Promise<void>;
}

export function createAgentsStore(io: AgentsIo) {
  return create<AgentsState>()((set, get) => ({
    agents: [],

    load: async () => {
      set({ agents: await io.list() });
    },

    duplicate: async (id: string) => {
      const original = get().agents.find((agent) => agent.id === id);
      if (original === undefined) return;

      /* `structuredClone`, nie `{ ...original }`. Płytka kopia współdzieli `skills`,
       * `connections` i `vendorOptions` z oryginałem, więc pierwsza edycja kopii po cichu
       * przepisuje agenta, którego użytkownik nie tknął — i dowiaduje się o tym po biegu,
       * nie po ekranie. Kopiowanie przez wyliczenie pól ma tę samą wadę o dzień później:
       * pierwsze dopisane pole-lista jest znowu współdzielone i nikt tego nie zauważa. */
      const copy: Agent = {
        ...structuredClone(original),
        id: await io.newId(),
        name: `${original.name} (copy)`,
      };

      set({ agents: [...get().agents, copy] });
      /* Duplikat to nowy PLIK. Kopia, która żyje tylko na ekranie, znika przy następnym
       * uruchomieniu — a agent, który zniknął, wygląda jak awaria zapisu (T4 §5.3). */
      await io.save(copy);
    },

    delete: async (id: string) => {
      set({ agents: get().agents.filter((agent) => agent.id !== id) });
      await io.remove(id);
    },
  }));
}
