/* Budowniczowie linii dla scen testowych.
 *
 * Po co w ogóle: każdy z czternastu rodzajów ma inny komplet pól (`src/ipc/types.ts`), więc
 * scena pisana w teście ręcznie to trzydzieści linii literałów, w których jedna literówka
 * w nazwie pola daje wiersz „prawie dobry" i cichy fałsz. Tutaj kształt jest w JEDNYM miejscu,
 * a scena w teście czyta się jak zdanie.
 *
 * Zwracany typ jest zadeklarowany jako `FeedLine`, nie wywnioskowany, i to jest cała obrona:
 * pole dodane albo usunięte po stronie Rusta rozjeżdża lustro `src/ipc/types.ts`, a wtedy
 * te funkcje przestają się kompilować. Scena testowa nie ma prawa być jedynym miejscem
 * w repo, które wie, jak wygląda wiersz z drutu.
 *
 * Teksty są po angielsku i bez żargonu (decyzja D5, niezmiennik 14) — to są zdania, które
 * w prawdziwym biegu pisze mapper po stronie Rusta i które czyta użytkownik.
 */
import type { FeedLine, ForeignLine } from '../../../../state/run';

export const line = {
  /** Nagłówek całego biegu. */
  run(id: number, at: number, agent: string, text: string): FeedLine {
    return { kind: 'run', agent, text, id, at };
  },

  /** Kreska z etykietą; kotwica bloku na pasku loadoutu. */
  step(id: number, at: number, agent: string, text: string): FeedLine {
    return { kind: 'step', agent, text, id, at };
  },

  /** Agent wszedł do biegu. Tylko na wejściu i wyjściu, nigdy gadanina. */
  agent(id: number, at: number, agent: string): FeedLine {
    return { kind: 'agent', agent, text: agent + ' joined', id, at };
  },

  /** Status, nie linia. Nie ma nawet pola tekstowego [T2 §7.3 reguła 5]. */
  thinking(id: number, at: number, agent: string): FeedLine {
    return { kind: 'thinking', agent, id, at };
  },

  /** Na czym stoi KROK. Jedyny wiersz, z którego widać, że czyjaś praca się skończyła. */
  stepState(id: number, at: number, agent: string, stepId: string, state: string): FeedLine {
    return { kind: 'stepState', agent, stepId, state, id, at };
  },

  read(id: number, at: number, agent: string, path: string): FeedLine {
    return {
      kind: 'read',
      agent,
      text: 'Read ' + path,
      count: 1,
      paths: [path],
      detailId: null,
      id,
      at,
    };
  },

  search(id: number, at: number, agent: string, what: string, matches: number): FeedLine {
    return {
      kind: 'search',
      agent,
      text: 'Searched for ' + what + ' — ' + String(matches) + ' matches',
      count: matches,
      paths: [],
      detailId: null,
      id,
      at,
    };
  },

  edit(
    id: number,
    at: number,
    agent: string,
    path: string,
    added: number,
    removed: number,
  ): FeedLine {
    return {
      kind: 'edit',
      agent,
      text: 'Edited ' + path,
      count: 1,
      paths: [path],
      added,
      removed,
      detailId: null,
      id,
      at,
    };
  },

  /** `output` to PEŁNE wyjście; ile z niego widać, rozstrzyga model, nie ten plik. */
  ran(
    id: number,
    at: number,
    agent: string,
    text: string,
    ok: boolean,
    output: readonly string[],
  ): FeedLine {
    return {
      kind: 'ran',
      agent,
      text,
      ok,
      preview: output[0] ?? '',
      detail: [...output],
      detailId: null,
      id,
      at,
    };
  },

  /** Jedyna proza w widoku. */
  note(id: number, at: number, agent: string, text: string): FeedLine {
    return { kind: 'note', agent, text, id, at };
  },

  /** Pytanie do człowieka. Przyklejone, dopóki nie ma odpowiedzi. */
  asked(id: number, at: number, agent: string, text: string, options: readonly string[]): FeedLine {
    return { kind: 'asked', agent, text, options: [...options], id, at };
  },

  handoff(id: number, at: number, agent: string, text: string): FeedLine {
    return { kind: 'handoff', agent, text, id, at };
  },

  memory(id: number, at: number, agent: string, path: string): FeedLine {
    return { kind: 'memory', agent, text: 'Saved a note — ' + path, path, id, at };
  },

  problem(id: number, at: number, agent: string, text: string): FeedLine {
    return { kind: 'problem', agent, text, resetsAt: null, id, at };
  },

  done(
    id: number,
    at: number,
    agent: string,
    text: string,
    ended: 'well' | 'badly' | 'stopped' = 'well',
  ): FeedLine {
    return {
      kind: 'done',
      agent,
      text,
      turns: 2,
      durationMs: 252_000,
      costUsd: 0.31,
      inputTokens: 4,
      outputTokens: 336,
      cachedTokens: 65_403,
      ended,
      id,
      at,
    };
  },

  /**
   * Wiersz rodzaju, którego to repo nie zna — czyli enum prosto z drutu.
   *
   * Nie jest to hipoteza ani złośliwość testu: dokładnie tak wygląda pierwszy nowy typ
   * zdarzenia, który vendor doda w przyszłym tygodniu i o którym nikt nas nie uprzedzi.
   */
  foreign(id: number, at: number, agent: string, kind: string): ForeignLine {
    return { kind, agent, id, at };
  },
};
