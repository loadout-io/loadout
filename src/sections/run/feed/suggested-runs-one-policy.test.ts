/* Kliknięcie w propozycję idzie TĄ SAMĄ drogą, co Enter w wierszu wejścia — czwarte kryterium
 * T-61.
 *
 * PO CO. „Który workflow, ile naraz, w którym folderze" ma jedną odpowiedź (niezmiennik 23),
 * a `startFromLine` jest tą odpowiedzią: czyta katalog workflow w chwili naciśnięcia, bierze
 * limit z tego samego modułu, z którego czyta go suwak obok Startu, i oddaje zdanie odmowy.
 * Druga droga startu byłaby drugą odpowiedzią, a pierwszy rozjazd między nimi jest cichy —
 * liczba jest wczytywana, logowana i inna.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: sama asercja negatywna („nie woła niczego zabronionego").
 * Przechodzi dla implementacji, która nie robi NIC — bo wtedy też nie woła niczego. Dlatego
 * przy każdej takiej asercji stoi kontrola: atrapa polityki musi zostać zawołana DOKŁADNIE raz,
 * z napisem porównanym co do znaku.
 *
 * DLACZEGO NIE PRZEZ KLIKNIĘCIE. To repo nie ma jsdom, więc `onClick` nie odpala się w teście,
 * a `renderToStaticMarkup` nie uruchamia efektów (`start-invokes.test.tsx`, nagłówek). Klikamy
 * więc to, co klika przycisk: czynność z `./suggested`. Kryterium wymagające przeglądarki nie
 * umie być czerwone z właściwego powodu, bo „Failed to launch" stoi na liście `NOT_A_REAL_RED`.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { runSuggestion } from './suggested';

/* Wszystkie atrapy są podniesione razem z `vi.mock`, żeby moduł pod testem dostał JE, a nie
 * prawdziwe krawędzie. Trzy, nie jedna: pytanie „czy poszło jedną drogą" ma sens tylko wtedy,
 * kiedy pozostałe dwie są obserwowane. */
const { started, launched, invoked, ran } = vi.hoisted(() => ({
  started: vi.fn((_rest: string): Promise<string | null> => Promise.resolve(null)),
  launched: vi.fn(),
  invoked: vi.fn(),
  ran: vi.fn(),
}));

vi.mock('../run-command', () => ({ startFromLine: started }));
vi.mock('../launch', () => ({ launchRun: launched }));
vi.mock('../io', () => ({ start: ran }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/** Komenda z wiersza propozycji, znak w znak taka, jaką napisał lider. */
const COMMAND = '/run nightly-cleanup Delete the run folders older than a week';

/**
 * Co ma dostać polityka startu. WPISANE, nie policzone z `COMMAND`.
 *
 * Wyliczone tą samą operacją, którą robi implementacja, byłoby oczekiwaniem pochodzącym od
 * sprawdzanego kodu — a to jest ta odmiana zieleni, która przechodzi także wtedy, gdy obie
 * strony mylą się identycznie. Ten sam napis podaje Enter w wierszu wejścia
 * (`entry.tsx`: wszystko po `/run`, przycięte).
 */
const REST = 'nightly-cleanup Delete the run folders older than a week';

/** Zdanie odmowy, jakie oddaje polityka, kiedy takiego workflow nie ma na dysku. */
const REFUSAL =
  'There is no workflow called "nightly-cleanup". These are the ones you have: ship-a-feature.';

describe('pressing a suggested run goes the one way a run is started', () => {
  beforeEach(() => {
    started.mockClear();
    launched.mockClear();
    invoked.mockClear();
    ran.mockClear();
    started.mockResolvedValue(null);
  });

  it('hands the start policy the rest of the line, character for character', async () => {
    const said = await runSuggestion(COMMAND);

    expect(
      started.mock.calls.length,
      'the one policy that starts runs was never called, so pressing the button did nothing at ' +
        'all. Zero here is also what makes every negative assertion in this file pass for free, ' +
        'which is why it is asked first.',
    ).toBe(1);
    expect(
      started.mock.calls.at(0)?.at(0),
      'the policy has to get everything after `/run`, character for character — the same string ' +
        'Enter hands it from the input line. A button that reassembles the command its own way ' +
        'is a second answer to "what should run", and the two go apart quietly.',
    ).toBe(REST);
    expect(said, 'and nothing is left to say when the run actually went').toBe(null);
  });

  it('reaches for no other way of starting work', async () => {
    await runSuggestion(COMMAND);

    expect(
      {
        launchRun: launched.mock.calls.length,
        invoke: invoked.mock.calls.length,
        start: ran.mock.calls.length,
      },
      'the button went around the start policy. Straight to `launchRun`, to `start` or to Rust ' +
        'means the limit on how many at once, the folder the work happens in and the refusals ' +
        'all get answered a second time, in a second place (invariant 23). The reference repo ' +
        'lost secret scanning exactly this way: the policy was rewritten next door, both copies ' +
        'looked right, and the older one was the one wired up.',
    ).toEqual({ launchRun: 0, invoke: 0, start: 0 });
    expect(
      started.mock.calls.length,
      'and the control on the assertion above: the policy really was called, so those three ' +
        'zeros are a route taken, not an implementation that does nothing',
    ).toBe(1);
  });

  it('brings the refusal back instead of dropping it on the floor', async () => {
    started.mockResolvedValue(REFUSAL);

    const said = await runSuggestion(COMMAND);

    expect(
      said,
      'a proposal naming a workflow that is not on disk has to end in a sentence, not in ' +
        'silence. The policy already writes that sentence and names the workflows that do ' +
        'exist; swallowing it here leaves a person pressing a button and watching nothing ' +
        'happen, which reads as a broken app (DESIGN §8).',
    ).toBe(REFUSAL);
    expect(
      started.mock.calls.length,
      'and it came from the policy, not from a sentence written here',
    ).toBe(1);
  });
});
