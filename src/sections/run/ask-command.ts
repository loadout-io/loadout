/* `/ask <agent> <zadanie>` — jeden agent i jedno zdanie, bez zakładania workflow.
 *
 * PO CO TO ISTNIEJE. Zamówienie właściciela 2026-08-20: „odpalać nasze workflows/agents".
 * Workflow ma drogę z wiersza wejścia (`/run`), agent nie ma żadnej — bo jednostką pracy jest
 * PLIK. Żeby puścić jednego agenta z jednym zdaniem, człowiek musi wejść do edytora, założyć
 * workflow, postawić jeden kafelek, zapisać go i wrócić. To jest cena płacona za najczęstszą
 * czynność dnia, a płaci się ją za każdym razem.
 *
 * DLACZEGO OBOK `run-command.ts`, A NIE W NIM. Bo to są dwa różne rozbiory jednej linii i tylko
 * jedna z tych komend ma w ogóle listę workflow: `/run` tłumaczy pierwsze słowo na plik, `/ask`
 * tłumaczy je na agenta z biblioteki. Wspólna funkcja z flagą „czego szukamy" miałaby dwa
 * znaczenia w jednym miejscu — a wspólne jest to, co naprawdę wspólne: kształt nazwy do wpisania
 * (`typable`) i kształt podpowiedzi (`Named`), oba brane stamtąd, nie przepisane tutaj.
 *
 * DLACZEGO ROZBIÓR JEST CZYSTĄ FUNKCJĄ. To repo nie ma jsdom, więc naciśnięcia Enter nie da się
 * odpalić w kryterium. Polityka zamknięta w komponencie byłaby kodem, którego nic nie sądzi —
 * ta sama rodzina, z której wzięło się siedemnaście kłamiących kontrolek.
 *
 * SZKIELET T-62. Ciała rzucają, żeby kryterium padło NA ZACHOWANIU, w czasie wykonania: spec,
 * który nie umie się załadować, nie uruchomił niczego i nie poświadcza wyroczni (AGENTS.md §2a).
 * Podkreślenie przy argumentach jest po to, żeby `noUnusedParameters` z `checks/tsconfig.strict.json`
 * nie zamienił szkieletu w czerwień typów — nazwy zostają, bo to one opisują wejście.
 */
import type { Agent } from '../../state/agents';
import type { Named } from './run-command';

/**
 * Co `/ask` z tej linii znaczy: albo para (agent, zadanie), albo zdanie odmowy.
 *
 * AGENT, NIE JEGO NAZWA: po tamtej stronie granicy `run_agent` bierze IDENTYFIKATOR, bo ten
 * przeżywa zmianę nazwy [T3 §3.1]. Wiersz wejścia tłumaczy więc wpisane słowo na definicję,
 * zanim cokolwiek pojedzie na drut, i robi to w jednym miejscu.
 */
export type AskLine =
  { readonly agent: Agent; readonly task: string } | { readonly refusal: string };

/**
 * Nazwy agentów do podpowiedzenia po `/ask ` — w postaci DO WPISANIA.
 *
 * Kształt `Named` jest ten sam, co przy workflow, i to nie jest oszczędność: lista pod polem
 * rysuje się jednym kodem, więc dwa rodzaje podpowiedzi nie mają jak rozjechać się wyglądem.
 * Różnica jest wyłącznie w tym, skąd pochodzą wiersze.
 */
export function agentNames(_agents: readonly Agent[]): readonly Named[] {
  throw new Error('not implemented');
}

/**
 * Co znaczy to, co człowiek dopisał po `/ask`.
 *
 * PIERWSZE SŁOWO JEST AGENTEM, RESZTA JEST ZADANIEM. Reszta idzie dalej CO DO ZNAKU — zdanie
 * dla agenta jest tekstem człowieka, a nie listą słów: rozbiór, który sklei wielokrotne spacje,
 * przepisuje polecenie, za które ktoś zaraz zapłaci turą.
 */
export function readAskLine(_agents: readonly Agent[], _rest: string): AskLine {
  throw new Error('not implemented');
}
