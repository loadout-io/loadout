/* „N of M slots in use — an agent in X is waiting for a free one" wreszcie ma z czego powstać.
 *
 * SŁABE WERSJE:
 *   1. Policzenie WSZYSTKICH kroków, które jeszcze nie ruszyły. Przechodzi na dziesięciu krokach
 *      `pending` w pierwszej sekundzie biegu i wtedy zdanie mówi o kolejce po miejsce, w której
 *      nikt nie stoi: `pending` znaczy „czeka na poprzednika", a nie „czeka na permit"
 *      [ARCHITECTURE §5]. Dlatego jest tu przypadek z `pending` i `running` obok `ready`.
 *   2. `boolean` „ktoś czeka". Nie mówi, gdzie zajrzeć, a to jedyny powód, dla którego to
 *      zdanie stoi na pasku.
 *   3. Zwrócenie nazwy zawsze, gdy folder jest znany. Zdanie o kolejce, której nie ma, jest
 *      gorsze niż brak zdania (niezmiennik 17) — więc przy zerowej kolejce ma być `null`.
 */
import { describe, expect, it } from 'vitest';

import type { Step } from '../../../state/run';
import { waitingFor, waitingWhere } from './waiting';

const FOLDER = '/Users/x/ledger-ui';

/** Plan biegu, w którym jeden krok idzie, dwa stoją w kolejce po miejsce, jeden czeka na innych. */
const PLAN: readonly Step[] = [
  { id: 's1', name: 'Build', state: 'running' },
  { id: 's2', name: 'Docs', state: 'ready' },
  { id: 's3', name: 'Tests', state: 'ready' },
  { id: 's4', name: 'Ship', state: 'pending' },
  { id: 's5', name: 'Old', state: 'succeeded' },
];

describe('the queue for a free slot is counted from step states, never guessed', () => {
  it('counts only the steps that are ready and have no permit yet', () => {
    expect(
      waitingFor(PLAN),
      'only `ready` is the queue for a slot. Counting `pending` too says "four agents are ' +
        'waiting for a free slot" about a plan where two of them are waiting for another STEP.',
    ).toBe(2);
    expect(waitingFor([]), 'a run without a plan has nobody queued').toBe(0);
    expect(
      waitingFor(PLAN.filter((step) => step.state !== 'ready')),
      'with nothing ready the queue is empty, not "unknown"',
    ).toBe(0);
  });

  it('names the folder to look in, and says nothing when nobody is queued', () => {
    expect(
      waitingWhere(PLAN, FOLDER),
      'the sentence has to name where to look. "Something somewhere is waiting" is the version ' +
        'that sends a person hunting through three projects.',
    ).toBe('ledger-ui');
    expect(
      waitingWhere(
        PLAN.filter((step) => step.state !== 'ready'),
        FOLDER,
      ),
      'nobody is queued, so there is no sentence. A queue notice left on the bar after the ' +
        'queue drained teaches people not to read the bar.',
    ).toBe(null);
    expect(
      waitingWhere(PLAN, null),
      'steps are queued but we do not know what to call the place. A sentence with an empty ' +
        'name is worse than one sentence less (invariant 17).',
    ).toBe(null);
  });
});
