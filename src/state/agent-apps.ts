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
  /** Ponawia obie sondy. Nakładający się wołający dostaje tę samą pracę i tę samą Promise. */
  readonly check: () => Promise<void>;
}

type AppKey = 'claudeCode' | 'codex';

const CHECKING: AgentAppStatus = { state: 'checking' };
const COULD_NOT_CHECK: AgentAppStatus = { state: 'could-not-check' };

let inFlight: Promise<void> | null = null;

function appKey(value: unknown): AppKey | null {
  if (value === 'claude-code') return 'claudeCode';
  if (value === 'codex') return 'codex';
  return null;
}

function statusOf(entry: Record<string, unknown>): AgentAppStatus {
  if (entry.state === 'found') {
    const version = typeof entry.version === 'string' ? entry.version.trim() : '';
    return version === '' ? COULD_NOT_CHECK : { state: 'found', version };
  }
  if (entry.state === 'not-found') return { state: 'not-found' };
  if (entry.state === 'could-not-check') return COULD_NOT_CHECK;
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
