/* Co obie lokalne aplikacje agentów NAPRAWDĘ odpowiedziały — jedyna migawka w całym oknie.
 *
 * DLACZEGO TO W OGÓLE ISTNIEJE. Do 2026-09 stopka bocznego menu pisała stałą „Claude · Codex
 * ready": zdanie wpisane w kod, które nie pytało niczego. Człowiek bez zainstalowanego Codeksa
 * czytał obietnicę gotowości, a pierwszy krok biegu mówił „nie" — kontrolka bez skutku
 * (niezmiennik 16) na jedynej powierzchni, która mówi cokolwiek o otoczeniu aplikacji.
 *
 * CZTERY STANY, NIE DWA, i to jest cała treść tego pliku. `checking` znaczy „nikt jeszcze nie
 * odpowiedział", `not-found` znaczy „pytaliśmy i tego nie ma", `could-not-check` znaczy „nie
 * dowiedzieliśmy się" — a to są trzy różne zdania dla człowieka. Zlanie dwóch ostatnich w jedno
 * kazałoby oknu napisać „zainstaluj to" komuś, kto ma to zainstalowane i tylko nie odpowiedziało
 * w pięć sekund.
 *
 * NIE JEST TO BRAMA STARTU. Odmowę uruchomienia wydaje Rust (`commands::run::check_to_run`)
 * i tylko on; ten magazyn mówi wyłącznie, co widziało okno w chwili pytania.
 */
import { create } from 'zustand';

import { checkAgentApps } from './agent-apps-io';

export type AgentAppStatus =
  | { readonly state: 'checking' }
  | { readonly state: 'found'; readonly version: string }
  | { readonly state: 'not-found' }
  | { readonly state: 'could-not-check' };

export interface AgentAppsState {
  readonly claudeCode: AgentAppStatus;
  readonly codex: AgentAppStatus;
  /** Ponawia obie sondy. Nakładający się wołający dostaje tę samą pracę i tę samą obietnicę. */
  readonly check: () => Promise<void>;
}

type AppKey = 'claudeCode' | 'codex';

const CHECKING: AgentAppStatus = { state: 'checking' };
const COULD_NOT_CHECK: AgentAppStatus = { state: 'could-not-check' };

/** Trwający odczyt, jeśli jakiś jest. Retry na nim NIE uruchamia drugiej pary procesów. */
let inFlight: Promise<void> | null = null;

function appKey(value: unknown): AppKey | null {
  if (value === 'claude-code') return 'claudeCode';
  if (value === 'codex') return 'codex';
  return null;
}

/* Wersja pusta znaczy „nie dowiedzieliśmy się", nie „znaleziono". Zdanie „Claude Code · " bez
 * liczby jest gorsze niż uczciwe „nie dało się sprawdzić": wygląda jak odpowiedź i nią nie jest. */
function statusOf(entry: Record<string, unknown>): AgentAppStatus {
  if (entry.state === 'found') {
    const version = typeof entry.version === 'string' ? entry.version.trim() : '';
    return version === '' ? COULD_NOT_CHECK : { state: 'found', version };
  }
  if (entry.state === 'not-found') return { state: 'not-found' };
  return COULD_NOT_CHECK;
}

/** Każdy slot jest walidowany osobno, więc wadliwy Claude nie kasuje poprawnego Codeksa. */
function fromWire(value: unknown): Pick<AgentAppsState, 'claudeCode' | 'codex'> {
  const statuses: Record<AppKey, AgentAppStatus> = {
    claudeCode: COULD_NOT_CHECK,
    codex: COULD_NOT_CHECK,
  };
  if (!Array.isArray(value)) return statuses;

  const seen = new Set<AppKey>();
  for (const candidate of value) {
    if (typeof candidate !== 'object' || candidate === null || Array.isArray(candidate)) continue;
    const entry = candidate as Record<string, unknown>;
    const key = appKey(entry.app);
    if (key === null) continue;
    /* Dwa wiersze o tej samej aplikacji to odpowiedź, której nie umiemy przeczytać — ani
     * pierwsza, ani druga nie jest bardziej prawdziwa, więc mówimy, że nie wiemy. */
    if (seen.has(key)) {
      statuses[key] = COULD_NOT_CHECK;
      continue;
    }
    seen.add(key);
    statuses[key] = statusOf(entry);
  }
  return statuses;
}

export const useAgentApps = create<AgentAppsState>()((set) => ({
  claudeCode: CHECKING,
  codex: CHECKING,

  check: () => {
    if (inFlight !== null) return inFlight;

    set({ claudeCode: CHECKING, codex: CHECKING });
    const flight = checkAgentApps()
      .then((value) => {
        set(fromWire(value));
      })
      /* Odmowa granicy nie jest brakiem aplikacji: nie wiemy nic o żadnej z dwóch, więc mówimy
       * dokładnie tyle. Zero na tej ścieżce obwiniałoby człowieka za awarię, której nie wywołał. */
      .catch(() => {
        set({ claudeCode: COULD_NOT_CHECK, codex: COULD_NOT_CHECK });
      });
    inFlight = flight;
    void flight.finally(() => {
      if (inFlight === flight) inFlight = null;
    });
    return flight;
  },
}));
