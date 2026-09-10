import type { RunState } from '../../state/run';

// Stałe tożsamości: odczyt odbiorców wiadomości obserwuje `steps` i zapisuje w tym
// samym magazynie. Nowa pusta tablica przy każdym odczycie zapętlałaby ten efekt.
const emptySteps: RunState['steps'] = [];
const emptyLines: RunState['lines'] = [];
const emptyAgents: RunState['agents'] = [];
const emptySessions: RunState['messageSessions'] = [];

/** 2026-09-10: wynik należy do karty biegu, nie do każdej rozmowy w jego projekcie.
 * Zachowujemy go w magazynie, żeby powrót do tamtej karty nadal pokazywał, co zaszło.
 * Żywa praca pozostaje widoczna także z nowej rozmowy: nadal musi być dostępny Stop. */
export function visibleRun(
  state: RunState,
  terminal: string | null,
  folder: string | null,
): RunState {
  if (state.ended === null || terminal === null || terminal === folder) return state;
  return {
    ...state,
    ended: null,
    progress: null,
    steps: emptySteps,
    links: null,
    lines: emptyLines,
    agents: emptyAgents,
    messageSessions: emptySessions,
    droppedBefore: 0,
    fileName: '',
  };
}
