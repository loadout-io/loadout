/* Lista kafelków bierze się ze STRUMIENIA, nie z planu [T2 §9.2].
 *
 * Cicha porażka numer trzy z tego zadania: lista zbudowana z definicji workflow. Pokazuje
 * kafelki agentów, którzy nigdy nie wystartują, i nie pokazuje pod-agentów, którzy
 * wystartowali naprawdę — czyli rysuje relację, której w danych nie ma (niezmiennik 17).
 * Wygląda przy tym lepiej od poprawnej wersji, bo „widać, co się będzie działo".
 *
 * Kafelek istnieje wtedy i TYLKO wtedy, gdy agent pojawił się w strumieniu. Kolejność jest
 * kolejnością pierwszego pojawienia się, nie kolejnością kroków w grafie.
 *
 * Dwa wejścia i podział między nimi jest tezą tego pliku:
 *   `view`    strumień. Jedyne źródło tego, KTÓRZY agenci istnieją i w jakiej kolejności.
 *   `agents`  co Loadout wie o agencie poza strumieniem: jak się nazywa, po co jest i na
 *             jakim kroku stoi. Nigdy źródło istnienia kafelka — sam wpis tutaj nie daje
 *             agentowi kafelka i o to w tym kryterium chodzi.
 *
 * Dlaczego stan kroku w ogóle tu jest: agent, którego krok anulowano po tym, jak coś nadał,
 * ma zostać na liście ze stanem `stopped`. Strumień tego nie mówi — nie ma rodzaju linii,
 * który by to niósł [T2 §7.2] — więc musi to powiedzieć plan. `Step` w `src/state/run.ts`
 * nie niesie nazwy agenta, a ten plik nie jest właścicielem tamtego, więc para
 * (agent, stan kroku) jest zadeklarowana tutaj i składa ją ten, kto montuje ekran pracy.
 */
import type { StepState } from '../../../state/run';
import type { FeedView } from '../feed/model';
import type { RailCard } from './card';

/** Co Loadout wie o agencie poza strumieniem. */
export interface AgentFacts {
  /** Podpis agenta w strumieniu — to samo, co `line.agent`. */
  readonly id: string;
  readonly name: string;
  /** Po co ten agent jest, jednym wyrażeniem po angielsku (`writes code`). */
  readonly role: string;
  /**
   * Stan kroku, który ten agent wykonuje. `null` dla agenta spoza planu — pod-agent
   * rozpuszczony w trakcie biegu nie stoi na żadnym kroku i nigdy nie będzie.
   */
  readonly step: StepState | null;
}

export interface RosterInput {
  readonly view: FeedView;
  readonly agents: readonly AgentFacts[];
}

/** Kafelki, w kolejności pierwszego pojawienia się w strumieniu. */
export function roster(_state: RosterInput): readonly RailCard[] {
  throw new Error('not implemented');
}
