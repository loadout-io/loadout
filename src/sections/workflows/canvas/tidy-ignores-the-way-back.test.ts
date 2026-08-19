/* „Tidy up" układa kafelki po strzałkach BEZ powrotów.
 *
 * POWÓD, ZMIERZONY NA KODZIE. `depths` liczy rząd kroku jako „o jeden więcej niż najgłębszy
 * poprzednik", a powrót idzie z sędziego pętli DO KROKU, KTÓRY BYŁ PRZED NIM. Policzony jako
 * wejście dawał implementerowi rząd większy niż testerowi — czyli „Tidy up" stawiał sędziego
 * NAD krokiem, do którego zawraca, i układ kłamał o kolejności pracy przy każdym kliknięciu.
 *
 * To ta sama reguła i ten sam powód, co przy liczeniu `forward` w walidatorze Rusta: kolejność
 * wyznaczają wyłącznie strzałki znaczące „po".
 *
 * SŁABĄ WERSJĄ jest sprawdzenie, że przycisk nie wiesza się na pętli. To już działa — `depths`
 * ma zbiór `busy` i broni się przed kołem w pliku poprawionym ręcznie — więc takie kryterium
 * świeci na zielono nad kodem, którego nie ma. Sądzone są POZYCJE: implementer wyżej niż tester,
 * a krok za pętlą niżej od obu.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { tidyUp } from './tidy';

function step(id: string): AgentStep {
  return {
    kind: 'agent',
    id,
    name: id,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the work.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    /* Pozycje wyjściowe celowo POMIESZANE: gdyby zostały uporządkowane, kryterium przechodziłoby
     * także dla implementacji, która niczego nie przestawia. */
    at: { x: 240, y: 480 },
  };
}

/** `implement → tester → ship`, i tester z powrotem do implementera. */
function withALoop(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_loop',
    name: 'Implement and test',
    steps: [step('s_impl'), step('s_test'), step('s_ship')],
    links: [
      { from: 's_impl', to: 's_test' },
      { from: 's_test', to: 's_ship' },
      { from: 's_test', to: 's_impl', max_turns: 3 },
    ],
  };
}

/** Pionowa pozycja kroku po ułożeniu. */
function rowOf(file: WorkflowFile, id: string): number {
  const found = file.steps.find((step) => step.id === id);
  return found?.at.y ?? Number.NaN;
}

describe('tidy up with a loop in the graph', () => {
  it('keeps the judge below the step it sends the work back to', () => {
    const tidied = tidyUp(withALoop());

    expect(
      rowOf(tidied, 's_impl'),
      'the implementer comes first and has to stay on top. Counting the way back as an incoming ' +
        'arrow flips the two, and then the layout says the tester runs before the work it tests.',
    ).toBeLessThan(rowOf(tidied, 's_test'));
    expect(rowOf(tidied, 's_test'), 'and the step after the loop stays below both').toBeLessThan(
      rowOf(tidied, 's_ship'),
    );
  });

  it('puts the two steps of the loop in the same column, not side by side', () => {
    const tidied = tidyUp(withALoop());
    const implementer = tidied.steps.find((step) => step.id === 's_impl');
    const tester = tidied.steps.find((step) => step.id === 's_test');

    expect(
      implementer?.at.x,
      'they are a chain, not two branches: one column, one below the other. Two steps sharing ' +
        'a row would mean the layout thinks they can run at the same time.',
    ).toBe(tester?.at.x);
  });
});
