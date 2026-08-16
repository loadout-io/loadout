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
import type { FeedLine } from '../../../state/run';
import type { Who } from '../../../state/run';

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
 * `lines` to linie z drutu, nie wiersze historii, i to jest różnica z powodem: `say` musi
 * odróżnić prozę agenta od podsumowania sprawdzeń, a `ran` niesie `ok` wyłącznie przed
 * sklejeniem. Wiersz historii tę informację już zgubił.
 */
export interface AgentInRun {
  readonly id: string;
  readonly name: string;
  readonly role: string;
  readonly status: AgentStatus;
  /** Wszystko, co ten agent nadał, w kolejności napłynięcia. */
  readonly lines: readonly FeedLine[];
}

/** Kafelek tego agenta. */
export function railCard(_agent: AgentInRun): RailCard {
  throw new Error('not implemented');
}
