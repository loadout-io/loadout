/* Formularz agenta: dziewięć wierszy i przycisk, który rozwija trzy.
 *
 * SZKIELET. Komponent renderuje pusty fragment, żeby kryterium 5 padło na ASERCJI, a nie na
 * nierozwiązanym imporcie: `vitest` przewraca się już na zbieraniu plików i „Cannot find
 * module" nie jest czerwienią, tylko sprawdzeniem, które nic nie poświadczyło
 * (AGENTS.md §2a p. 5).
 *
 * Formularz jest STEROWANY: wartości i stan rozwinięcia przychodzą propsami, a każda zmiana
 * wychodzi przez `onChange`. Powód nie jest architektoniczny, tylko testowy — w repo nie ma
 * `jsdom` ani `@testing-library/react` (`package.json` jest na liście DENIED w
 * `checks/quick-scope.sh`), więc formularz sprawdzamy przez `renderToStaticMarkup`. Statyczny
 * HTML wystarcza na kolejność etykiet i na atrybut `disabled`, a stan trzymany wewnątrz
 * komponentu byłby dla takiego testu niewidoczny.
 *
 * Dziewięć wierszy jest wiążące: `docs/mockup/index.html`, panel `Forge` w sekcji Agents.
 * Pole wchodzi tu tylko wtedy, gdy zauważyłbyś jego brak w pierwszej godzinie [T4 §3].
 */
import type { ReactElement } from 'react';
import type { Agent } from '../../state/agents';

export interface AgentFormProps {
  value: Agent;
  /** Czy `More settings` jest rozwinięte. Stan mieszka wyżej — patrz nagłówek pliku. */
  expanded: boolean;
  onChange: (next: Agent) => void;
  onToggleMore: () => void;
  onSave: () => void;
}

export function AgentForm(_props: AgentFormProps): ReactElement {
  return <></>;
}
