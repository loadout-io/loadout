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
 *
 * DLACZEGO TEN PLIK IMPORTUJE `src/ipc/why.ts`, a zdanie wyżej mówi „NIE importuje `@/ipc`"
 * — dopisane 2026-08-18. Zakaz dotyczy NAZW KOMEND: gdyby magazyn wiedział, że zapis to
 * `save_agent`, byłaby druga droga do Rusta i asercja „edycja kroku nie zapisuje agenta"
 * pilnowałaby jednej z dwóch. `why()` nie zna ani jednej nazwy komendy — to czysta funkcja
 * tekstowa, która wyjmuje zdanie z tego, CZYM Tauri odrzuciło wywołanie. Musi stać tutaj,
 * bo zdanie zapasowe zależy od CZYNNOŚCI (zapis, usunięcie, wczytanie), a czynność zna
 * magazyn, nie ekran.
 *
 * DLACZEGO MAGAZYN MA POLE `refusal` — zmierzone na maszynie właściciela 2026-08-18:
 * `~/.loadout/agents` nie istniał, bo nieudany zapis był ABSOLUTNIE cichy. `index.tsx` robił
 * `void (async () => { … })()` bez `catch`, a stan nie miał gdzie postawić zdania dla
 * człowieka. Klikasz `Save`, nic się nie dzieje, drugie kliknięcie identycznie — i to jest
 * jedyna i wystarczająca przyczyna, dla której każdy bieg kończył się odmową „no agent
 * saved here has the id …". Kontrolka, której handler nie ma widocznego skutku, jest gorsza
 * niż jej brak (niezmiennik 16).
 */
import { create } from 'zustand';

import { why } from '../ipc/why';

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
  /** Czy ten agent może sięgnąć do internetu.
   *
   * OSOBNE POLE, nie pozycja na liście narzędzi, i to jest jedyny kształt, którym umieją mówić
   * OBAJ vendorzy: u Claude'a sieć to dwa czasowniki (`WebFetch`, `WebSearch`), a Codex nie ma
   * listy narzędzi wcale — u niego jest to ustawienie piaskownicy. Powód w całości stoi przy
   * `Agent::reaches_the_web` w `library/agents.rs`. */
  reachesTheWeb: boolean;
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
  /**
   * Zdanie dla człowieka po odmowie z dysku — `null`, kiedy nie ma o czym mówić.
   *
   * JEDNO pole na całą sekcję, nie po jednym na czynność (niezmiennik 13). Powód jest
   * praktyczny: człowiek robi jedną rzecz naraz, a cztery pola znaczą cztery miejsca, w których
   * ktoś zapomni skasować stare zdanie, i ekran zaczyna pokazywać odmowę sprzed dwóch minut
   * jako odpowiedź na klik, który się właśnie udał. Każda czynność czyści je na wejściu.
   */
  refusal: string | null;
  load: () => Promise<void>;
  /**
   * Zapisuje agenta na dysk i wstawia go do listy. `true`, kiedy naprawdę się zapisał.
   *
   * Wartość zwracana nie jest ozdobą: ekran ma po niej zamknąć panel albo go ZOSTAWIĆ
   * otwartym z tym, co człowiek wpisał. Panel zamykany bezwarunkowo gubił całą definicję
   * agenta przy pierwszej odmowie dysku.
   *
   * Puste `id` znaczy „nowy agent" — identyfikator wybija mennica po stronie Rusta [T4 §5.1].
   */
  save: (agent: Agent) => Promise<boolean>;
  duplicate: (id: string) => Promise<void>;
  delete: (id: string) => Promise<void>;
  /** Kasuje zdanie odmowy. Kontrolka „×" obok zdania i nic więcej. */
  dismiss: () => void;
}

/**
 * Czego brakuje, żeby tego agenta dało się zapisać — zdanie dla człowieka albo `null`.
 *
 * PO CO TO ISTNIEJE (zmierzone 2026-08-18). `Save` w formularzu był wygaszony i NIC nie mówiło
 * dlaczego: `grep 'required|aria-required'` dawał zero trafień w kontrolkach. Wygaszony przycisk
 * bez powodu jest kontrolką, która kłamie przez milczenie — człowiek widzi Save, klika i nie
 * dostaje ani zapisu, ani zdania. To jest druga połowa przyczyny, dla której `~/.loadout/agents`
 * nie istniał na maszynie właściciela (pierwsza to cichy `catch`, patrz nagłówek).
 *
 * DLACZEGO TA REGUŁA MIESZKA W MAGAZYNIE, A NIE W FORMULARZU. Dwóch wołających: formularz
 * (wygasza przycisk i podpisuje powód) i `save` (odmawia, bo jest JEDYNĄ krawędzią do dysku).
 * Przy dwóch kopiach warunku dopisanie trzeciego pola wymaganego budzi przycisk i nie zmienia
 * odmowy, albo odwrotnie — a wtedy „czy da się zapisać" ma dwie odpowiedzi (niezmiennik 13).
 * Jest to fakt o AGENCIE, nie o kontrolce, więc mieszka tam, gdzie mieszka agent.
 *
 * Zdanie NAZYWA POLA dokładnie tymi etykietami, które stoją nad kontrolkami. „Fill in the
 * required fields" kazałoby szukać, które to są — a formularz ma ich dziewięć.
 */
export function missingForSave(agent: Agent): string | null {
  const hasName = agent.name.trim().length > 0;
  const hasInstructions = agent.instructions.trim().length > 0;

  if (!hasName && !hasInstructions) return 'Fill in Name and Instructions to save this agent.';
  if (!hasName) return 'Fill in Name to save this agent.';
  if (!hasInstructions) return 'Fill in Instructions to save this agent.';

  /* WIERSZ PRZELOTKI Z NAZWĄ I BEZ WARTOŚCI JEDZIE TĄ SAMĄ DROGĄ, i to jest cały powód, dla
   * którego stoi tutaj, a nie w kontrolce. Warunek ma już dwóch wołających — formularz wygasza
   * Save i podpisuje powód, magazyn odmawia, bo jest jedyną krawędzią do dysku — więc trzecia
   * kopia znaczyłaby przycisk, który budzi się przy wierszu bez wartości, i zapis, który go
   * dalej nie przyjmuje (niezmiennik 13).
   *
   * DLACZEGO TO W OGÓLE JEST ODMOWĄ. Flaga oddana bez wartości połyka następny argument jako
   * swój, więc komenda znaczy co innego, niż wygląda — a nic na ekranie by tego nie powiedziało.
   *
   * Czytamy OBIE nazwy aplikacji, nie tylko tę z `Runs with`: wpisy drugiej zostają w pliku
   * (formularz je chowa, nie kasuje), a plik jest tym, co pojedzie do argv. */
  const half = halfAPair(agent);
  if (half !== null) {
    return `Give ${half} a value in Extra options, or delete that line, to save this agent.`;
  }
  return null;
}

/** Nazwa wpisu przelotki, za którą nic nie stoi — albo `null`, kiedy każdy ma swoją wartość. */
function halfAPair(agent: Agent): string | null {
  for (const options of Object.values(agent.vendorOptions ?? {})) {
    for (const [name, value] of Object.entries(options)) {
      if (value.trim().length === 0) return name;
    }
  }
  return null;
}

/** Lista z podmienionym agentem o tym `id`, albo z dopisanym na końcu, gdy go tam nie było. */
function upsert(agents: readonly Agent[], saved: Agent): Agent[] {
  const known = agents.some((agent) => agent.id === saved.id);
  return known
    ? agents.map((agent) => (agent.id === saved.id ? saved : agent))
    : [...agents, saved];
}

export function createAgentsStore(io: AgentsIo) {
  return create<AgentsState>()((set, get) => ({
    agents: [],
    refusal: null,

    load: async () => {
      set({ refusal: null });
      try {
        set({ agents: await io.list() });
      } catch (error) {
        /* Pusta biblioteka i biblioteka NIEOSIĄGALNA czytają się na ekranie identycznie —
         * dokładnie ta pomyłka trzymała sekcję pustą przez kilkanaście godzin (patrz nagłówek
         * `src/sections/agents/index.tsx`). Dlatego odmowa ma zdanie, a lista zostaje taka,
         * jaka była: kasowanie jej tutaj mówiłoby „nic tam nie leży", czego nie wiemy. */
        set({ refusal: why(error, 'Loadout could not read the agents you have saved.') });
      }
    },

    save: async (agent: Agent) => {
      set({ refusal: null });

      /* Odmowa PRZED dotknięciem dysku. Agent bez instrukcji przechodzi walidator biblioteki
       * i pada dopiero na kroku, w biegu — czyli po tym, jak człowiek zbudował wokół niego
       * workflow. Mennica `newId` też się nie odpala: wybity identyfikator, którego nikt nie
       * zapisał, jest dziurą w sortowalnym po czasie ciągu [T4 §5.1]. */
      const missing = missingForSave(agent);
      if (missing !== null) {
        set({ refusal: missing });
        return false;
      }

      try {
        const id = agent.id === '' ? await io.newId() : agent.id;
        const complete: Agent = { ...agent, id };
        /* Dysk PIERWSZY, lista druga. W odwrotnej kolejności agent, którego zapis odmówił,
         * siedzi na ekranie do najbliższego uruchomienia i wygląda na zapisanego — a krok
         * workflow, który go nazwie, odmawia dopiero w biegu (niezmiennik 4). */
        await io.save(complete);
        set({ agents: upsert(get().agents, complete) });
        return true;
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not save that agent.') });
        return false;
      }
    },

    duplicate: async (id: string) => {
      set({ refusal: null });
      const original = get().agents.find((agent) => agent.id === id);
      if (original === undefined) return;

      /* `structuredClone`, nie `{ ...original }`. Płytka kopia współdzieli `skills`,
       * `connections` i `vendorOptions` z oryginałem, więc pierwsza edycja kopii po cichu
       * przepisuje agenta, którego użytkownik nie tknął — i dowiaduje się o tym po biegu,
       * nie po ekranie. Kopiowanie przez wyliczenie pól ma tę samą wadę o dzień później:
       * pierwsze dopisane pole-lista jest znowu współdzielone i nikt tego nie zauważa. */
      try {
        const copy: Agent = {
          ...structuredClone(original),
          id: await io.newId(),
          name: `${original.name} (copy)`,
        };

        /* Duplikat to nowy PLIK. Kopia, która żyje tylko na ekranie, znika przy następnym
         * uruchomieniu — a agent, który zniknął, wygląda jak awaria zapisu (T4 §5.3).
         * Zapis idzie PRZED wstawieniem do listy, z tego samego powodu, co w `save`. */
        await io.save(copy);
        set({ agents: [...get().agents, copy] });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not make a copy of that agent.') });
      }
    },

    delete: async (id: string) => {
      set({ refusal: null });
      try {
        /* Plik pierwszy, ekran drugi. W odwrotnej kolejności agent zniknięty z listy przy
         * nieudanym usunięciu WRACA po restarcie, a człowiek dowiaduje się o tym wtedy, kiedy
         * już zapomniał, że go usuwał. */
        await io.remove(id);
        set({ agents: get().agents.filter((agent) => agent.id !== id) });
      } catch (error) {
        set({ refusal: why(error, 'Loadout could not delete that agent.') });
      }
    },

    dismiss: () => {
      set({ refusal: null });
    },
  }));
}
