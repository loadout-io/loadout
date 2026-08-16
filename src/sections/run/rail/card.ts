/* Kafelek agenta w liście agentów [DESIGN §6 `agent-card`].
 *
 * Ograniczenie całego katalogu i jedyna rzecz, którą trzeba o tym pliku wiedzieć:
 * **kafelek nie liczy, kafelek pokazuje.** Cztery sloty niosące tekst — imię, rola, jedno
 * zdanie, stan — i ani jednego więcej. Piąta linia jest błędem projektowym wpisanym do
 * tabeli sufitu gęstości [ARCHITECTURE §7], nie kwestią gustu: wiersz metadanych
 * („12 files · 2m 04s") wygląda dobrze na jednym agencie i rozjeżdża listę przy czterech.
 *
 * Dwa pola nie niosą tekstu i każde z innego powodu:
 *   `id`      podpis agenta w strumieniu. Klucz, nigdy napis na ekranie.
 *   `square`  NAZWA tokenu tożsamości (`--color-id-3`), nie hex — hex w kodzie komponentu
 *             jest zakazany [DESIGN §9], a kolor kwadratu przydziela `colour.ts`.
 *
 * Stan agenta jest SŁOWEM, nie kolorem kwadratu [DESIGN §3 „Tożsamość ≠ stan"]. To jest ta
 * reguła, przez którą w referencyjnym redesign poprzedniego prototypu agent Forge miał dokładnie ten hex,
 * który na sąsiednim kafelku znaczył „czeka na twoją decyzję".
 */
import type { Who } from '../../../state/run';
import { identityToken } from './colour';
import type { Utterance } from './say';
import { sayFor } from './say';

/**
 * Sześć stanów, w jakich lista agentów pokazuje agenta.
 *
 * To NIE jest `StepState` z `src/state/run.ts` i nie ma być: tamten opisuje krok grafu
 * (siedem wartości, w tym `pending` i `ready`), ten opisuje agenta, który już coś nadał —
 * a agent, który nic nie nadał, nie ma kafelka w ogóle (niezmiennik 17).
 */
export type AgentStatus = 'working' | 'waiting' | 'needs you' | 'failed' | 'done' | 'stopped';

/**
 * Jedno zdanie na kafelku i jedno słowo o tym, kto je powiedział.
 *
 * `who` jest tu obowiązkowe, nie ozdobne. Blok „latest note from this agent" karmiony
 * czymkolwiek, co przyszło ostatnie, podaje zdanie Loadouta („3 of 40 tests failed") jako
 * cytat agenta — czyli `agent said` w rubryce `happened`, tylko mniejszą czcionką
 * [00-SYNTHESIS §2.2].
 */
export interface Say {
  readonly text: string;
  readonly who: Who;
}

/** Kafelek. Sześć pól, z czego cztery niosą tekst. */
export interface RailCard {
  readonly id: string;
  readonly name: string;
  readonly role: string;
  readonly say: Say;
  /** Nazwa tokenu tożsamości. Nigdy token stanu, nawet dla agenta `failed`. */
  readonly square: string;
  readonly status: AgentStatus;
}

/**
 * Agent tak, jak widzi go lista agentów: fakty z definicji plus to, co sam nadał.
 *
 * `lines` to `Utterance`, czyli para (rodzaj, zdanie) — najmniejszy kształt, jaki wystarcza
 * zdaniu kafelka. Linia z drutu (`FeedLine`) jest nim bez żadnej przeróbki, a wiersz historii
 * daje się do niego sprowadzić jednym mapowaniem [roster.ts]. To nie jest ubożenie typu, tylko
 * jedyny sposób, żeby polityka „kto to powiedział" istniała RAZ: kafelki w prawdziwym biegu
 * powstają z wierszy historii, a `railCard` musi dawać tę samą odpowiedź w obu wywołaniach
 * (niezmiennik 23).
 */
export interface AgentInRun {
  readonly id: string;
  readonly name: string;
  readonly role: string;
  readonly status: AgentStatus;
  /** Wszystko, co ten agent nadał, w kolejności napłynięcia. */
  readonly lines: readonly Utterance[];
}

/**
 * Kafelek tego agenta.
 *
 * Sześć pól i ani jednego więcej — funkcja jest krótka i taka ma zostać. Każdy licznik,
 * który kusi, żeby go tu dopisać („12 files · 2m 04s"), jest piątą linią kafelka: wygląda
 * dobrze przy jednym agencie i rozjeżdża listę przy czterech [ARCHITECTURE §7].
 */
export function railCard(agent: AgentInRun): RailCard {
  return {
    id: agent.id,
    name: agent.name,
    role: agent.role,
    say: sayFor(agent.lines),
    /* Z `id`, nie z `name`: podpis w strumieniu jest tym, co się nie zmienia. */
    square: identityToken(agent.id),
    status: agent.status,
  };
}
