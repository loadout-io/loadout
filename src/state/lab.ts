import { create } from 'zustand';
import type { StoreApi, UseBoundStore } from 'zustand';

import { why } from '../ipc/why';
import { list as listAgents } from '../sections/agents/io';
import { labIo } from '../sections/lab/io';
import type {
  EvalBoard,
  EvalCase,
  EvalFix,
  EvalSet,
  EvalSubject,
  EvalVariant,
  LabIo,
} from '../sections/lab/io';
import { runEvalSet } from '../sections/run/io';
import { atOnce as chosenAtOnce } from '../sections/run/limits/chosen';
import { activeWorkspace } from '../state/workspaces';

/* Ile przebiegów czytamy przy otwarciu zestawu.
 *
 * Osiem, bo tyle mieści się w linii trendu bez zamieniania jej w wykres, a każdy kosztuje
 * odczyt `run.json` i katalogu przekazań. Czytanie wszystkich biegów projektu przy każdym
 * otwarciu ekranu jest kosztem, który rośnie z wiekiem projektu i którego nikt nie zamawia. */
const HOW_MANY_RUNS = 8;

/** Co ekran robi w tej chwili. Jedno pole, bo dwie prace naraz w tej sekcji nie istnieją. */
export type LabBusy = 'idle' | 'loading' | 'proposing' | 'running' | 'saving';

/** Agent z biblioteki, sprowadzony do tego, czego ta sekcja potrzebuje. */
export interface LabAgent {
  readonly id: string;
  readonly name: string;
}

export interface LabState {
  /** Zestawy tego projektu, w kolejności nazw plików. */
  readonly sets: readonly EvalSet[];
  /**
   * Agenci biblioteki — do wyboru, kto pisze kandydatki i czyja praca jest mierzona.
   *
   * Lista, nie identyfikator: człowiek wybiera przy każdej z tych dwóch czynności osobno, bo
   * agent piszący przypadki i agent mierzony to dwie różne role. Ten sam wybór w obu byłby
   * zestawem, w którym mierzony pisze sobie sprawdziany.
   */
  readonly agents: readonly LabAgent[];
  /** Który zestaw jest otwarty, albo `null`. */
  readonly openId: string | null;
  /** Wszystko, co ekran rysuje dla otwartego zestawu. */
  readonly board: EvalBoard | null;
  readonly busy: LabBusy;
  /**
   * Zdanie dla człowieka — odmowa albo wynik, który warto powiedzieć.
   *
   * Jedno pole na oba, bo oba stoją w tym samym miejscu na ekranie i oba znikają przy
   * następnej czynności. Dwa pola dawałyby ekran, na którym stara odmowa stoi pod nowym
   * wynikiem i nie wiadomo, które z nich jest o tym, co człowiek właśnie zrobił.
   */
  readonly said: string | null;
  /**
   * Poprawka czekająca na człowieka, albo `null`.
   *
   * Nie stosuje się sama i nie ma trzeciego stanu: instrukcja agenta jest tym, co on robi
   * w KAŻDYM biegu, także poza Labem, więc pętla przepisująca ją bez człowieka zmieniałaby
   * jego zachowanie w nocy.
   */
  readonly fix: EvalFix | null;
  load(): Promise<void>;
  open(id: string): Promise<void>;
  create(name: string, subject: EvalSubject, agent: string): Promise<void>;
  remove(id: string): Promise<void>;
  propose(agent: string): Promise<void>;
  stopProposing(): Promise<void>;
  decide(caseId: string, keep: boolean): Promise<void>;
  putCase(one: EvalCase): Promise<void>;
  putVariant(variant: EvalVariant): Promise<void>;
  dropVariant(variantId: string): Promise<void>;
  run(): Promise<void>;
  askForAFix(agent: string): Promise<void>;
  applyFix(): Promise<void>;
  dropFix(): void;
  forget(): void;
}

/** Agenci biblioteki, sprowadzeni do dwóch pól. Definicji tu nie trzymamy: sekcja Agents jest
 * ich właścicielem, a druga kopia rozjechałaby się z nią po pierwszym zapisie. */
async function people(): Promise<LabAgent[]> {
  const all = await listAgents();
  return all.map((one) => ({ id: one.id, name: one.name }));
}

/** Folder, w którym ta sekcja pracuje. `null` znaczy „ten, pod którym wstała aplikacja". */
function folder(): string | null {
  return activeWorkspace()?.folder ?? null;
}

/**
 * Magazyn sekcji Lab. `io` wchodzi argumentem, żeby kryteria mogły podać własną krawędź —
 * ta sama zasada, co w magazynie triggerów.
 */
export function createLabStore(
  io: LabIo = labIo,
  launch: typeof runEvalSet = runEvalSet,
): UseBoundStore<StoreApi<LabState>> {
  return create<LabState>((set, get) => ({
    sets: [],
    agents: [],
    openId: null,
    board: null,
    busy: 'idle',
    said: null,
    fix: null,

    async load(): Promise<void> {
      set({ busy: 'loading' });
      try {
        /* Oba odczyty razem i oba przed zdjęciem `busy`: ekran bez listy agentów rysuje pole
         * wyboru, w którym nie ma czego wybrać — czyli kontrolkę, która na kliknięcie nie ma
         * odpowiedzi (niezmiennik 16). */
        const [sets, agents] = await Promise.all([io.list(folder()), people()]);
        set({ sets, agents, busy: 'idle' });
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not read the sets here.') });
      }
    },

    async open(id: string): Promise<void> {
      set({ busy: 'loading', openId: id, said: null });
      try {
        set({ board: await io.board(folder(), id, HOW_MANY_RUNS), busy: 'idle' });
      } catch (error: unknown) {
        set({
          busy: 'idle',
          board: null,
          said: why(error, 'Loadout could not open that set.'),
        });
      }
    },

    async create(name: string, subject: EvalSubject, agent: string): Promise<void> {
      set({ busy: 'saving', said: null });
      try {
        const made = await io.create(folder(), name, subject, agent);
        set({ busy: 'idle' });
        await get().load();
        await get().open(made.set.id);
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not start that set.') });
      }
    },

    async remove(id: string): Promise<void> {
      set({ busy: 'saving', said: null });
      try {
        await io.remove(folder(), id);
        /* Otwarty zestaw znika razem z plikiem: ekran pokazujący tabelę czegoś, czego już nie
         * ma, jest ekranem, na którym każdy przycisk odmawia. */
        const closing = get().openId === id;
        set({
          busy: 'idle',
          openId: closing ? null : get().openId,
          board: closing ? null : get().board,
        });
        await get().load();
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not remove that set.') });
      }
    },

    async propose(agent: string): Promise<void> {
      const id = get().openId;
      if (id === null) return;
      set({ busy: 'proposing', said: null });
      try {
        const proposed = await io.propose(folder(), id, agent);
        /* ODŚWIEŻENIE PRZED ZDANIEM, i to nie jest kolejność do gustu. `open` zaczyna od
         * `said: null`, bo otwarcie INNEGO zestawu nie ma prawa nieść odmowy poprzedniego —
         * więc zdanie postawione przed nim znika w tej samej turze, w której powstało.
         * Zmierzone kryterium `controls-have-an-effect`: licznik „2 cases are waiting for you"
         * był liczony, zapisywany i kasowany, a człowiek po turze, za którą zapłacił, widział
         * pusty ekran. */
        await get().open(id);
        set({ busy: 'idle', said: saidAboutCases(proposed.written, proposed.withoutAReason) });
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not write cases here.') });
      }
    },

    async stopProposing(): Promise<void> {
      try {
        await io.stopProposing();
      } catch (error: unknown) {
        set({ said: why(error, 'Loadout could not stop that.') });
      }
    },

    async decide(caseId: string, keep: boolean): Promise<void> {
      const id = get().openId;
      const board = get().board;
      if (id === null || board === null) return;
      set({ busy: 'saving', said: null });
      try {
        await io.decide(folder(), id, caseId, keep, board.set.revision);
        set({ busy: 'idle' });
        await get().open(id);
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not save that.') });
      }
    },

    async putCase(one: EvalCase): Promise<void> {
      const id = get().openId;
      const board = get().board;
      if (id === null || board === null) return;
      set({ busy: 'saving', said: null });
      try {
        await io.putCase(folder(), id, one, board.set.revision);
        set({ busy: 'idle' });
        await get().open(id);
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not save that case.') });
      }
    },

    async putVariant(variant: EvalVariant): Promise<void> {
      const id = get().openId;
      const board = get().board;
      if (id === null || board === null) return;
      set({ busy: 'saving', said: null });
      try {
        await io.putVariant(folder(), id, variant, board.set.revision);
        set({ busy: 'idle' });
        await get().open(id);
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not save that column.') });
      }
    },

    async dropVariant(variantId: string): Promise<void> {
      const id = get().openId;
      const board = get().board;
      if (id === null || board === null) return;
      set({ busy: 'saving', said: null });
      try {
        await io.dropVariant(folder(), id, variantId, board.set.revision);
        set({ busy: 'idle' });
        await get().open(id);
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not remove that column.') });
      }
    },

    async run(): Promise<void> {
      const board = get().board;
      const id = get().openId;
      if (id === null || board === null) return;
      /* Odmowa PRZED biegiem, ze zdaniem, które napisał Rust: zestaw bez kolumny albo z samymi
       * kandydatkami nie ma czego uruchomić, a bieg, który by ruszył, byłby biegiem o zerowej
       * liczbie kroków — i odmówiłby po założeniu karty i mignięciu paska. */
      /* ODMOWA BEZ DRUGIEGO ZDANIA. Powod stoi juz na ekranie — rysuje go `cannotRun`
       * nad tabela — a przepisanie go do `said` daje ten sam napis dwa razy pod soba
       * (niezmiennik 13, zmierzone na zywym ekranie 2026-08-31). Przycisk Run jest przy tym
       * stanie wygaszony, wiec ta galaz jest ostatnia zapora, nie glowna droga. */
      if (board.cannotRun !== null) return;
      set({ busy: 'running', said: null });
      try {
        const refused = await launch(id, chosenAtOnce(), folder(), board.set.set.name);
        // Ta sama kolejność i ten sam powód, co przy `propose`: `open` czyści zdanie.
        await get().open(id);
        set({ busy: 'idle', said: refused });
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not start that run.') });
      }
    },

    async askForAFix(agent: string): Promise<void> {
      const id = get().openId;
      if (id === null) return;
      set({ busy: 'proposing', said: null, fix: null });
      try {
        set({ busy: 'idle', fix: await io.proposeFix(folder(), id, agent) });
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not ask for a fix.') });
      }
    },

    async applyFix(): Promise<void> {
      const fix = get().fix;
      const id = get().openId;
      if (fix === null || id === null) return;
      set({ busy: 'saving', said: null });
      try {
        await io.applyFix(fix.agent, fix.instructions, fix.revision);
        /* Poprawka schodzi z ekranu dopiero PO udanym zapisie: karta zdjęta wcześniej zabiera
         * tekst, którego nie ma już gdzie przeczytać, a odmowa dysku zostawia człowieka
         * z niczym. */
        set({
          busy: 'idle',
          fix: null,
          said: 'Saved. Run the set again to see whether it helped.',
        });
      } catch (error: unknown) {
        set({ busy: 'idle', said: why(error, 'Loadout could not save that change.') });
      }
    },

    dropFix(): void {
      set({ fix: null });
    },

    forget(): void {
      set({ said: null });
    },
  }));
}

/**
 * Zdanie o tym, co przyszło z tury pisania kandydatek.
 *
 * MÓWI TAKŻE O ODRZUCONYCH, i to jest cała treść tej funkcji. „Przyszło sześć, zapisano
 * cztery" jest zdaniem, które człowiek umie sprawdzić; ciche odrzucenie uczy go, że model
 * zwraca mniej, niż zwraca — a wtedy przestaje wierzyć licznikowi.
 */
export function saidAboutCases(written: number, withoutAReason: number): string {
  const wrote =
    written === 1
      ? 'One case is waiting for you.'
      : String(written) + ' cases are waiting for you.';
  if (withoutAReason === 0) return wrote;
  const dropped =
    withoutAReason === 1
      ? 'One more was thrown away: it did not say which file it came from.'
      : String(withoutAReason) +
        ' more were thrown away: they did not say which file they came from.';
  return wrote + ' ' + dropped;
}

/** Magazyn produkcyjny. */
export const useLab = createLabStore();
