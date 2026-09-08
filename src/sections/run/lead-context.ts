/* Podgląd Startu należy do terminalu, tak jak jego rozmowa. Moduł trzyma wyłącznie ostatnie
 * backendowe żądanie; wybór materiałów i autorytet uruchomienia pozostają po stronie Rusta. */
import type { RunRequested } from '../../ipc/types';

/* Kształt powtórzony zamiast zaimportowany z `io.ts`: ten moduł jest jego ZALEŻNOŚCIĄ
 * (`io.ts` woła `rememberLeadContextRequest`), więc import w drugą stronę zamknąłby cykl. */
export type RememberedChoice =
  { readonly place: 'workflow' } | { readonly place: 'steps'; readonly stepIds: readonly string[] };

const requests = new Map<string, RunRequested>();
/* 2026-09-08 (CT-07) — WYBÓR DOSTARCZENIA NIE MOŻE ŻYĆ TYLKO W STANIE PODGLĄDU. Odmowa Startu
 * przenosi żądanie pod kartę biegu (`forget` + `remember`), więc podgląd się PRZEMONTOWUJE
 * i lokalny `useState` wraca do domyślnego „cały workflow". Przycisk „Keep previous selection
 * and start" wysyłał wtedy inny zakres niż ten, który człowiek widział przed odmową — czyli
 * dokładnie tę cichą podmianę, której zabrania kryterium 3. */
const choices = new Map<string, RememberedChoice>();
const listeners = new Set<() => void>();

export function rememberLeadContextRequest(terminal: string, request: RunRequested): void {
  requests.set(terminal, request);
  for (const listener of listeners) listener();
}

export function forgetLeadContextRequest(terminal: string): void {
  const had = requests.delete(terminal);
  choices.delete(terminal);
  if (!had) return;
  for (const listener of listeners) listener();
}

/** Zapamiętuje, gdzie człowiek kazał dostarczyć materiały, żeby przeżyło to przemontowanie. */
export function rememberLeadContextChoice(terminal: string, choice: RememberedChoice): void {
  choices.set(terminal, choice);
}

export function leadContextChoice(terminal: string): RememberedChoice | null {
  return choices.get(terminal) ?? null;
}

export function leadContextRequest(terminal: string): RunRequested | null {
  return requests.get(terminal) ?? null;
}

export function subscribeToLeadContextRequest(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
