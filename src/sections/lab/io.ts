import { invoke } from '@tauri-apps/api/core';

/* Krawędź sekcji Lab. Kształty są LUSTREM `src-tauri/src/commands/lab.rs`, pole w pole.
 *
 * Nazwy argumentów w każdym `invoke` muszą co do znaku odpowiadać nazwom parametrów skorup
 * w `ipc.rs`: Tauri dopasowuje je PO NAZWIE i deserializuje PRZED wejściem w ciało komendy,
 * więc literówka w kluczu nie daje mniejszego wywołania, tylko odrzucone — po cichu, bez ani
 * jednego zdania na ekranie. Sądzi to `checks/invoke-args.sh` i lustro `ipc-signature.ts`.
 */

/** Czego zestaw dotyczy: zapisanego agenta albo umiejętności. */
export type EvalSubject =
  | { readonly kind: 'agent'; readonly id: string }
  | { readonly kind: 'skill'; readonly name: string };

/** Oczekiwanie wobec jednego pola odpowiedzi. */
export interface EvalExpect {
  readonly field: string;
  /** Czego w nim szukamy. Pusty znaczy „wystarczy, że pole jest". */
  readonly contains: string;
  /** Zdanie, którym prompt prosi o to pole. */
  readonly describe: string;
}

/** Dwa stany przypadku. Awansuje wyłącznie człowiek. */
export type EvalCaseStatus = 'suggested' | 'in-use';

/** Jeden wiersz macierzy. */
export interface EvalCase {
  readonly id: string;
  readonly name: string;
  readonly task: string;
  readonly expect: readonly EvalExpect[];
  readonly command: string;
  readonly proof: string;
  readonly status: EvalCaseStatus;
  /** Skąd ten przypadek się wziął. Kandydatka bez tego nie istnieje. */
  readonly because: string;
}

/** Jedna kolumna macierzy: agent plus patch nad jego definicją. */
export interface EvalVariant {
  readonly id: string;
  readonly name: string;
  readonly agent: string;
  readonly overrides: Readonly<Record<string, unknown>>;
}

/** Cały zestaw, jak leży na dysku. */
export interface EvalSet {
  readonly format: number;
  readonly id: string;
  readonly name: string;
  readonly subject: EvalSubject;
  readonly cases: readonly EvalCase[];
  readonly variants: readonly EvalVariant[];
}

/** Zestaw razem z rewizją, na której okno go czyta. */
export interface OpenEvalSet {
  readonly set: EvalSet;
  readonly revision: string;
}

/** Jak skończyła się jedna komórka. */
export type CellOutcome = 'passed' | 'did-not-pass' | 'not-judged';

/** Jedna komórka jednego przebiegu. */
export interface EvalCell {
  readonly case: string;
  readonly variant: string;
  readonly outcome: CellOutcome;
  /** Dlaczego tak. Puste przy przejściu. */
  readonly said: string;
  readonly costUsd: number | null;
}

/** Jeden przebieg zestawu, policzony. */
export interface PastEval {
  readonly folder: string;
  readonly when: string;
  readonly state: string;
  readonly passed: number;
  readonly judged: number;
  readonly costUsd: number | null;
  readonly cells: readonly EvalCell[];
}

/** O ile najnowszy przebieg różni się od poprzedniego. */
export interface EvalMovement {
  readonly gained: number;
  readonly lost: number;
}

/** Wszystko, co ekran rysuje dla jednego zestawu. */
export interface EvalBoard {
  readonly set: OpenEvalSet;
  readonly runs: readonly PastEval[];
  readonly movement: EvalMovement | null;
  /** Zdanie o tym, czego brakuje do uruchomienia. `null` znaczy „można". */
  readonly cannotRun: string | null;
}

/** Poprawka, którą agent proponuje po przebiegu — zanim ktokolwiek ją zastosuje. */
export interface EvalFix {
  readonly agent: string;
  readonly name: string;
  /** Dlaczego. Człowiek czyta to PRZED tekstem. */
  readonly because: string;
  /** Cały nowy tekst instrukcji. */
  readonly instructions: string;
  /** Tekst, który ten agent ma teraz — żeby dało się przeczytać obie strony. */
  readonly insteadOf: string;
  /** Rewizja pliku agenta w chwili propozycji. Wraca z Apply jako oczekiwanie. */
  readonly revision: string | null;
}

/** Co wyszło z tury pisania kandydatek. */
export interface ProposedCases {
  readonly set: OpenEvalSet;
  readonly written: number;
  readonly withoutAReason: number;
  readonly unfinished: number;
}

export function list(folder: string | null): Promise<EvalSet[]> {
  return invoke<EvalSet[]>('list_eval_sets', { folder });
}

export function board(folder: string | null, set: string, howMany: number): Promise<EvalBoard> {
  return invoke<EvalBoard>('read_eval_board', { folder, set, howMany });
}

export function create(
  folder: string | null,
  name: string,
  subject: EvalSubject,
  agent: string,
): Promise<OpenEvalSet> {
  return invoke<OpenEvalSet>('create_eval_set', { folder, name, subject, agent });
}

export function remove(folder: string | null, set: string): Promise<void> {
  return invoke<void>('delete_eval_set', { folder, set });
}

export function propose(folder: string | null, set: string, agent: string): Promise<ProposedCases> {
  return invoke<ProposedCases>('propose_eval_cases', { folder, set, agent });
}

export function proposeFix(folder: string | null, set: string, agent: string): Promise<EvalFix> {
  return invoke<EvalFix>('propose_eval_fix', { folder, set, agent });
}

export function applyFix(
  agent: string,
  instructions: string,
  expectedRevision: string | null,
): Promise<string> {
  return invoke<string>('apply_eval_fix', { agent, instructions, expectedRevision });
}

export function stopProposing(): Promise<void> {
  return invoke<void>('stop_proposing_cases');
}

export function decide(
  folder: string | null,
  set: string,
  which: string,
  keep: boolean,
  expectedRevision: string | null,
): Promise<OpenEvalSet> {
  return invoke<OpenEvalSet>('decide_eval_case', {
    folder,
    set,
    case: which,
    keep,
    expectedRevision,
  });
}

export function putCase(
  folder: string | null,
  set: string,
  one: EvalCase,
  expectedRevision: string | null,
): Promise<OpenEvalSet> {
  return invoke<OpenEvalSet>('put_eval_case', { folder, set, case: one, expectedRevision });
}

export function putVariant(
  folder: string | null,
  set: string,
  variant: EvalVariant,
  expectedRevision: string | null,
): Promise<OpenEvalSet> {
  return invoke<OpenEvalSet>('put_eval_variant', { folder, set, variant, expectedRevision });
}

export function dropVariant(
  folder: string | null,
  set: string,
  variant: string,
  expectedRevision: string | null,
): Promise<OpenEvalSet> {
  return invoke<OpenEvalSet>('drop_eval_variant', { folder, set, variant, expectedRevision });
}

/** The whole edge a real run goes through. Its lines land in the same feed every run uses. */
export interface LabIo {
  readonly list: typeof list;
  readonly board: typeof board;
  readonly create: typeof create;
  readonly remove: typeof remove;
  readonly propose: typeof propose;
  readonly proposeFix: typeof proposeFix;
  readonly applyFix: typeof applyFix;
  readonly stopProposing: typeof stopProposing;
  readonly decide: typeof decide;
  readonly putCase: typeof putCase;
  readonly putVariant: typeof putVariant;
  readonly dropVariant: typeof dropVariant;
}

export const labIo: LabIo = {
  list,
  board,
  create,
  remove,
  propose,
  proposeFix,
  applyFix,
  stopProposing,
  decide,
  putCase,
  putVariant,
  dropVariant,
};
