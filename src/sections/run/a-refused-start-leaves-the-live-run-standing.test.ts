/* Start, który nigdy nie ruszył, nie ogłasza końca biegu, który pracuje.
 *
 * 2026-08-23 — ZE ZRZUTU WŁAŚCICIELA, dwa zdania pod rząd w jednym terminalu:
 *
 *   Loadout ❯ …
 *   Loadout   A run is already going, and Loadout leads one at a time so that Stop always
 *             reaches the one that is working. Press Stop first, then ask again.
 *   Loadout ❯ /stop
 *   Loadout   Nothing is running.
 *
 * Bieg pracował wtedy czterdzieści minut. Odmowa nazywa następny ruch, a tego ruchu nie było:
 * z tego wiersza nie dało się już zatrzymać niczego.
 *
 * PRZYCZYNA NIE JEST W `/stop`. Każdy start zapisuje „teraz biegnie to" PRZED `invoke`, bo
 * komenda po tamtej stronie trwa tyle, co bieg. `ask` nie ma przy tym zapadki `going` i ma jej
 * nie mieć — drugie `/ask` ma dostać ZDANIE, a nie cudzy bieg — więc dochodzi do Rusta, dostaje
 * odmowę, a jego `finally` woła `nowRunning('', [], null)`. To zdanie jest prawdziwe o biegu,
 * który nie ruszył, i FAŁSZYWE o tym, który pracuje: obu dotyczy jeden wpis w jednej sesji
 * zakresu. Od tej chwili `workflow === ''`, czyli w całej aplikacji „nic nie biegnie" — a z tego
 * pola żyje przycisk Stop, `/stop` w wierszu i pasek żywych biegów.
 *
 * SŁABA WERSJA: „po odmowie sesja dalej coś trzyma". Przechodzi ją implementacja, która nie
 * czyści NIGDY — a wtedy Stop zostaje na ekranie po biegu, którego nie ma, czyli jest kontrolką
 * bez roboty (niezmiennik 16). Dlatego drugi punkt puszcza prawdziwy bieg do końca i żąda, żeby
 * wpis zniknął.
 */
import { describe, expect, it, vi } from 'vitest';

const { invoked, endTheRun, refuseTheNextOne } = vi.hoisted(() => {
  let release: (() => void) | null = null;
  let refuse = false;
  return {
    invoked: vi.fn((): Promise<void> =>
      refuse
        ? Promise.reject(new Error('A run is already going'))
        : new Promise<void>((resolve) => {
            release = resolve;
          }),
    ),
    endTheRun: (): void => {
      release?.();
      release = null;
    },
    refuseTheNextOne: (yes: boolean): void => {
      refuse = yes;
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { start, ask } = await import('./io');
const { runFor } = await import('../../state/run');

const HERE = '/Users/x/urc-monorepo';

/** Odpowiedź całej aplikacji na „czy coś biegnie" — jedno pole, jedno miejsce. */
function whatRuns(): string {
  return runFor(HERE).getState().workflow;
}

describe('a start that was turned down leaves the live run alone', () => {
  it('keeps the working run on the bar when a second one is refused, and lets it go when it ends', async () => {
    const live = start('deep-research.json', 3, { name: 'Deep research', steps: [] }, HERE);

    expect(
      whatRuns(),
      'the run that just started is not on the bar at all, so nothing below is about a live run',
    ).toBe('Deep research');

    refuseTheNextOne(true);
    await expect(
      ask({ id: 'a-1', name: 'Forge' }, 'have a look at this', 3, HERE),
      'the refusal has to reach the person who asked',
    ).rejects.toThrow();

    expect(
      whatRuns(),
      'a refused /ask wiped the live run off the bar. Stop is drawn from this one field, so the ' +
        'person is told "press Stop first" by Loadout and then told "Nothing is running." by ' +
        'Stop, with a run working the whole time and no way left to reach it',
    ).toBe('Deep research');

    /* DRUGI PUNKT: kiedy prawdziwy bieg naprawdę zejdzie, wpis znika. Bez tego naprawą byłoby
     * „nie czyść nigdy", a wtedy Stop zostaje na ekranie na zawsze. */
    refuseTheNextOne(false);
    endTheRun();
    await live;

    expect(
      whatRuns(),
      'the run is over and the bar still shows it. Stop would stand there with nothing to stop',
    ).toBe('');
  });
});
