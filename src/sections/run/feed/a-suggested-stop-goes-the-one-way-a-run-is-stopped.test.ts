/* Wiersz `/stop` od lidera zatrzymuje bieg TEJ karty — i mówi to zdaniem, które już istnieje.
 *
 * PO CO. Od 2026-09 (Z-39) most ma czasownik `stop_run`, a jego jedyną drogą do biegu jest ta
 * sama, którą jedzie start: `Line::Suggested { auto: true }` → `autoStarts` → `runSuggestion`.
 * Bez gałęzi na `/stop` ten wiersz odbijał się od `runSuggestion` zdaniem „That line does not
 * name a workflow" — czyli lider meldowałby człowiekowi, że zatrzymał bieg, który dalej idzie
 * i dalej płaci. To jest ta klasa wady, dla której to repo powstało: kryterium zielone po
 * stronie Rusta, funkcja martwa po stronie okna.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(said).toBe(null)`. Przechodzi dla implementacji, która nie
 * robi NIC — bo wtedy też nie ma czego powiedzieć. Rozstrzyga ADRES: komenda `stop_run` musi
 * pojechać dokładnie raz i dostać folder tej rozmowy, bo `null` znaczy po tamtej stronie
 * „katalog, pod którym wstała aplikacja" — czyli potrafi zdjąć bieg sąsiedniej karty.
 *
 * ATRAPĄ JEST TU WYŁĄCZNIE GRANICA PROCESU, i to jest cała różnica wobec pierwszej wersji tego
 * pliku. Podmieniony jest `invoke` — czyli sam kabel do Rusta, którego w vitescie nie ma — a nie
 * krawędź `io.stop`. Atrapa krawędzi dowodziłaby, że okno woła własną funkcję; podmieniony
 * `invoke` dowodzi, że jedzie PRAWDZIWA komenda `stop_run` z prawdziwym adresem. Że po tamtej
 * stronie ta komenda naprawdę kładzie bieg, dowodzi
 * `src-tauri/tests/it/z39_the_lead_stops_a_run.rs` — na żywej grupie procesów i na `ESRCH`.
 *
 * DLACZEGO NIE PRZEZ KLIKNIĘCIE. Wiersz z mostu ma `auto: true` i nikt go nie klika: odpala go
 * `openChat`. Klikamy więc to, co odpala okno — czynność z `./suggested`.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { NOTHING_RUNS } from '../entry/entry';
import { runSuggestion } from './suggested';

/* Atrapy podniesione razem z `vi.mock`. Polityka startu jest obserwowana, żeby „poszło Stopem"
 * znaczyło cokolwiek: bez niej ta sama zieleń należy się implementacji, która myli jedno
 * z drugim. */
const { started, invoked } = vi.hoisted(() => ({
  started: vi.fn((_rest: string): Promise<string | null> => Promise.resolve(null)),
  invoked: vi.fn((_command: string, _args: unknown): Promise<unknown> => Promise.resolve(true)),
}));

vi.mock('../run-command', () => ({ startFromLine: started }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/** Komenda, którą niesie wiersz z mostu — znak w znak ta sama, którą wpisałby człowiek. */
const STOP = '/stop';

/** Folder rozmowy, z której ten wiersz przyszedł. */
const FOLDER = '/Users/someone/dev/ledger-ui';

describe('a suggested stop goes the one way a run is stopped', () => {
  beforeEach(() => {
    started.mockClear();
    invoked.mockClear();
    invoked.mockResolvedValue(true);
  });

  it('sends the stop command Rust answers, addressed with this folder', async () => {
    const said = await runSuggestion(STOP, FOLDER);

    expect(
      invoked.mock.calls.length,
      'nothing crossed the wire at all, so the lead told this person the run was over while it ' +
        'kept going and kept paying. Zero here is also what makes every negative assertion below ' +
        'pass for free, which is why it is asked first.',
    ).toBe(1);
    expect(
      invoked.mock.calls.at(0),
      'the command name and the address are the whole contract with Rust: `stop_run` is what ' +
        'ipc.rs registered, and `folder` is the key it matches by name. `null` there means "the ' +
        'folder the app started under" on the other side, so an unaddressed stop can take down ' +
        "the neighbouring card's run — the defect Z-35 closed on the Stop button.",
    ).toEqual(['stop_run', { folder: FOLDER }]);
    expect(said, 'and there is nothing to say when the run really went down').toBe(null);
  });

  it('never mistakes a stop for a start', async () => {
    await runSuggestion(STOP, FOLDER);

    expect(
      started.mock.calls.length,
      'the stop went through the start policy. That means a row saying "stopping" starts work ' +
        'instead, and the person finds out by watching the agents carry on.',
    ).toBe(0);
    expect(
      invoked.mock.calls.at(0)?.at(0),
      'and the control on the assertion above: something really did cross the wire, so that zero ' +
        'is a route taken and not an implementation that does nothing',
    ).toBe('stop_run');
  });

  it('says what happened when there was nothing to stop', async () => {
    invoked.mockResolvedValue(false);

    const said = await runSuggestion(STOP, null);

    expect(
      said,
      'the answer comes from the same sentence the input line and the Stop button use ' +
        '(invariant 13). Written again here it would be a second answer to one question, and ' +
        'the two go apart quietly — which is how `/stop` once said "Nothing is running." over a ' +
        'run that had been working for forty minutes.',
    ).toBe(NOTHING_RUNS);
  });
});
