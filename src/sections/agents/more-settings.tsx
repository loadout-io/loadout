/* Trzy wiersze pod `More settings`: Tools, Skills, Connections. Trzy i ani jeden więcej.
 *
 * SZKIELET. Renderuje pusty fragment — patrz nagłówek `agent-form.tsx`.
 *
 * Przy Codeksie `Tools` jest wygaszone i pod spodem stoi jedno zdanie. Bez ikony ostrzeżenia,
 * bez modala, bez czerwieni [T4 §8.1]: to nie jest błąd użytkownika ani awaria, tylko fakt
 * o drugiej aplikacji. Precedens jest cudzy i mocny — `claude import codex --dry-run` mapuje
 * wyłącznie serwery narzędziowe, a resztę wypisuje prostym zdaniem z powodem [T4 §6.2].
 *
 * Który to stan, mówi tabela z `capabilities.ts`, nie ten plik. Warunek `if vendor === 'codex'`
 * postawiony tutaj byłby drugą kopią polityki, a druga kopia zawsze w końcu mówi co innego
 * (niezmiennik 23).
 */
import type { ReactElement } from 'react';
import type { Agent } from '../../state/agents';

export interface MoreSettingsProps {
  value: Agent;
  onChange: (next: Agent) => void;
}

export function MoreSettings(_props: MoreSettingsProps): ReactElement {
  return <></>;
}
