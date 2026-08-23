/* Bieg, który nie zaczął się od Startu, jest dla okna biegiem — inaczej nie da się go zatrzymać.
 *
 * 2026-08-23 — ZE ZRZUTU WŁAŚCICIELA. Nacisnął `/stop` nad pracującym agentem i dostał
 * **„Nothing is running."**, a krok pracował dalej: Playwright robił snapshot już po tej
 * odpowiedzi. Przyczyna nie była w `/stop`. `rerunStep` i `resumeRun` wpinały kanał linii i nic
 * poza tym — nie mówiły magazynowi, że bieg ruszył. A „czy coś biegnie" to w całej aplikacji
 * dokładnie `workflow !== ''` (`state/run.ts`, komentarz przy polu), z czego żyje przycisk Stop,
 * `/stop` w wierszu i pasek żywych biegów.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie samego pola po starcie. Przechodzi ją implementacja, która pola
 * nigdy nie czyści — a wtedy Stop zostaje na ekranie na zawsze i jest kontrolką bez roboty
 * (niezmiennik 16). Dlatego każdy przypadek pyta o OBIE chwile: w trakcie i po.
 *
 * DRUGA POŁOWA, i ona jest o pieniądzach: obie drogi mają brać ZAPADKĘ. Bieg położony na biegu
 * to dwa procesy piszące po tych samych plikach (niezmiennik 12) — a zapadka jest jedynym
 * miejscem, które temu zapobiega.
 */
import { describe, expect, it, vi } from 'vitest';

const { invoked, resolveIt } = vi.hoisted(() => {
  let release: ((value: string | null) => void) | null = null;
  return {
    invoked: vi.fn(
      (): Promise<string | null> =>
        new Promise<string | null>((resolve) => {
          release = resolve;
        }),
    ),
    resolveIt: (): void => {
      release?.(null);
      release = null;
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { rerunStep, resumeRun } = await import('./io');
const { runFor } = await import('../../state/run');

/* Zdanie odmowy wpisane RĘCZNIE, nie zaimportowane: `io.ts` trzyma je prywatnie, a kryterium
 * porównujące wartość samą ze sobą przechodzi także wtedy, gdy nikt jej nigdy nie wypowie. */
const ONE_RUN_AT_A_TIME =
  'That run is still going, and Loadout leads one at a time so that Stop always reaches the one ' +
  'that is working. Press Stop first, then press Run again.';

const HERE = '/Users/x/ledger-ui';

/** Odpowiedź całej aplikacji na pytanie „czy coś biegnie" — jedno pole, jedno miejsce. */
function somethingRuns(): boolean {
  return runFor(HERE).getState().workflow !== '';
}

const WAYS = [
  {
    what: 'a step run again',
    press: (): Promise<string | null> => rerunStep('ship-a-feature.json', 's_9', 3, HERE),
  },
  {
    what: 'a run picked up from history',
    press: (): Promise<string | null> =>
      resumeRun('20260816-194804__x', 's_9', 3, HERE, 'Ship a feature', 'ship-a-feature.json'),
  },
] as const;

describe('every kind of run tells the window it is running', () => {
  for (const way of WAYS) {
    it(`${way.what} can be stopped while it lasts, and lets go when it ends`, async () => {
      expect(somethingRuns(), 'nothing may be running before the test presses anything').toBe(
        false,
      );

      const started = way.press();

      expect(
        somethingRuns(),
        'the window has to know this run exists BEFORE the command comes back — the command ' +
          'lasts as long as the run does. Without this the person presses Stop over a working ' +
          'agent and is told nothing is running, which is exactly what happened on screen.',
      ).toBe(true);

      resolveIt();
      await started;

      expect(
        somethingRuns(),
        'and it has to let go when the run ends, or Stop stays on screen for a run that is over',
      ).toBe(false);
    });

    it(`${way.what} refuses to start on top of another run`, async () => {
      const first = way.press();

      await expect(
        way.press(),
        'two runs in one folder means two agents writing over the same files, and each one ' +
          'finishes reporting success (invariant 12). The latch is the only thing that stops it.',
      ).rejects.toBe(ONE_RUN_AT_A_TIME);

      resolveIt();
      await first;
    });
  }
});
