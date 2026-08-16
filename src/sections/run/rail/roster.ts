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
import type { AgentStatus, RailCard } from './card';
import { railCard } from './card';
import type { Utterance } from './say';

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

/**
 * Stan kroku → stan agenta na kafelku.
 *
 * `Record`, nie `switch`: ósmy stan kroku dopisany do `StepState` przestaje TU się
 * kompilować, zamiast wpaść do gałęzi „reszta" i pokazać agenta jako pracującego.
 *
 * Dwa wiersze warto przeczytać razem z kryterium. `cancelled` daje `stopped`, a nie brak
 * kafelka: agent zdążył coś zrobić, zanim krok odwołano, i skasowanie kafelka wymazałoby
 * pracę, która naprawdę się wydarzyła. `skipped` daje to samo z drugiej strony — krok się
 * nie wydarzy, więc „working" pokazywałoby agenta, którego tam nie ma. Agent pominiętego
 * kroku, który nic nie nadał, nie ma kafelka w ogóle i rozstrzyga to strumień, nie ta tabela.
 */
const OF_STEP: Readonly<Record<StepState, AgentStatus>> = {
  pending: 'waiting',
  ready: 'waiting',
  running: 'working',
  succeeded: 'done',
  failed: 'failed',
  cancelled: 'stopped',
  skipped: 'stopped',
};

/**
 * Stan agenta: krok mówi, co się z nim dzieje, a pytanie bez odpowiedzi bije wszystko.
 *
 * Pytanie bije, bo krok wtedy dalej „biegnie" i sam z siebie nie odróżnia agenta, który
 * pisze kod, od agenta, który stoi i czeka na ciebie — a to jest jedyna rzecz na tym
 * ekranie, którą musisz zobaczyć, żeby bieg ruszył dalej.
 *
 * `null` znaczy „nie ma go w planie", czyli pod-agent rozpuszczony w trakcie biegu. Nadał
 * coś, więc pracuje; nic w strumieniu nie mówi, żeby przestał.
 */
function statusOf(step: StepState | null, waitsOnYou: boolean): AgentStatus {
  if (waitsOnYou) return 'needs you';
  if (step === null) return 'working';
  return OF_STEP[step];
}

/** Kafelki, w kolejności pierwszego pojawienia się w strumieniu. */
export function roster(state: RosterInput): readonly RailCard[] {
  const known = new Map(state.agents.map((facts) => [facts.id, facts]));
  const answered = new Set(state.view.answers.map((answer) => answer.questionId));

  /* Kolejność wstawienia do `Map` JEST kolejnością pierwszego pojawienia się w strumieniu —
   * dlatego lista kafelków bierze się stąd, a nie z `state.agents`. Plan wymienia agentów
   * w kolejności grafu, a w biegu równoległym te dwa porządki nie zgadzają się prawie nigdy. */
  const said = new Map<string, Utterance[]>();
  const waitingOnYou = new Set<string>();

  for (const row of state.view.history) {
    const before = said.get(row.agent);
    const utterances = before ?? [];
    if (before === undefined) said.set(row.agent, utterances);
    /* Etykieta wiersza, nie tekst linii: kafelek ma powiedzieć to samo, co strumień, więc
     * sklejona grupa mówi „Read 6 files" w obu miejscach albo w żadnym (kryterium 4 tego
     * samego zadania, tylko o jeden ekran wyżej). */
    utterances.push({ kind: row.kind, text: row.label });
    if (row.kind === 'asked' && !answered.has(row.id)) waitingOnYou.add(row.agent);
  }

  const cards: RailCard[] = [];
  for (const [id, lines] of said) {
    const facts = known.get(id);
    cards.push(
      railCard({
        id,
        /* Pod-agenta nie ma w planie i nigdy nie będzie — żaden workflow nie umie go nazwać
         * z góry. Zostaje to, co wiemy: podpis, którym nadaje. Rola jest pusta, bo pustego
         * slotu kafelek po prostu nie rysuje, a wymyślona rola byłaby relacją, której
         * w danych nie ma (niezmiennik 17). */
        name: facts?.name ?? id,
        role: facts?.role ?? '',
        status: statusOf(facts?.step ?? null, waitingOnYou.has(id)),
        lines,
      }),
    );
  }
  return cards;
}
