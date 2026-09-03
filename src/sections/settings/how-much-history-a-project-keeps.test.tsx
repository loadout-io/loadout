/* Ile historii zostaje w folderze projektu — wybór, który ma DOJŚĆ DO PLIKU (2026-09, Z-9).
 *
 * PO CO TO ISTNIEJE. Folder biegu niesie strumienie agentów, przekazania i kopie notatek, a nic
 * ich nigdy nie przycinało: u właściciela 2026-09-02 leżało w jednym projekcie 87 folderów biegów
 * na 3,8 GB, i jedyną drogą był terminal. Retencję wykonuje Rust przy otwarciu folderu, ale
 * WYŁĄCZNIE na podstawie liczby zapisanej w bibliotece — kontrolka, która tej liczby nie oddaje
 * dyskowi, jest kontrolką bez skutku (niezmiennik 16).
 *
 * SŁABA WERSJA: `expect(markup).toContain('Past runs to keep')`. Przechodzi ją napis nad polem,
 * którego nikt nie czyta i które nigdzie nie prowadzi. Rozstrzygają dwie rzeczy naraz: pole jest
 * na ekranie i wskazuje na zdanie o zerze, a wywołany zapis dowozi klucz do granicy.
 *
 * DRUGA SŁABA WERSJA: sprawdzić sam `chooseKeepLastRuns` i uwierzyć, że ekran go woła. Dlatego
 * markup pochodzi z całego ekranu sekcji, a nie z osobno zamontowanego pola.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _sent?: unknown): Promise<unknown> =>
    Promise.resolve({
      defaultLead: '',
      defaultBudgetUsd: 75,
      navCollapsed: false,
      keepLastRuns: 0,
    }),
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const {
  default: SettingsScreen,
  KEEP_LAST_RUNS_LABEL,
  KEEP_EVERYTHING_SAID,
} = await import('./index');
const { chooseKeepLastRuns } = await import('../../state/settings');

/** Ile biegów człowiek chce zostawić. Nie zero: zero jest tu wartością domyślną. */
const KEEP = 20;

/** Sam tekst, który człowiek czyta: znaczniki znikają RAZEM ZE SWOIMI ATRYBUTAMI. */
function onScreen(markup: string): string {
  return markup.replace(/<[^>]*>/g, ' ');
}

const markup = renderToStaticMarkup(<SettingsScreen />);

invoked.mockClear();
await chooseKeepLastRuns(KEEP);
const askedRust = invoked.mock.calls.at(0);

describe('Settings says how much history a project folder keeps', () => {
  it('draws the control and says on screen what zero does', () => {
    expect(
      markup,
      'there is no field for how many past runs a project folder keeps, so the only way to get ' +
        'rid of a run is still the terminal',
    ).toContain('id="keep-last-runs"');
    expect(
      onScreen(markup),
      'nothing on the screen says what zero does, so the one value that turns this off reads ' +
        'like the one that deletes everything',
    ).toContain(KEEP_EVERYTHING_SAID);

    /* Opis jedzie przez `aria-describedby`, bo tekst wpisany w `<label>` staje się NAZWĄ
       kontrolki, nie jej opisem (zmierzone 2026-08-28 na siedmiu czerwonych kryteriach e2e). */
    const field = /<input[^>]*id="keep-last-runs"[^>]*>/.exec(markup)?.[0] ?? '';
    const describedBy = /aria-describedby="([^"]+)"/.exec(field)?.[1] ?? '';
    const carrier = describedBy
      .split(' ')
      .map((id) => new RegExp(`<p[^>]*\\bid="${id}"[^>]*>([\\s\\S]*?)</p>`).exec(markup)?.[1] ?? '')
      .join(' ');
    expect(
      carrier,
      'the field points at nothing, so a screen reader announces a number with no wording ' +
        'around it and zero stays a guess',
    ).toContain(KEEP_EVERYTHING_SAID);
    expect(
      field,
      'and the field has to carry its own short name, because the description is not a name',
    ).toContain(KEEP_LAST_RUNS_LABEL);
  });

  it('sends the number to the one file that remembers what Loadout does by default', () => {
    expect(
      askedRust?.at(0),
      'the choice reached nothing, so it dies with the window and Rust goes on keeping every run',
    ).toBe('save_settings');
    const sent = (askedRust?.at(1) ?? {}) as Record<string, unknown>;
    expect(
      sent.keepLastRuns,
      'the write carries no such key, so the number the person typed never reaches the file. ' +
        'Tauri matches invoke arguments BY NAME: a missing key is not a smaller call, it is a ' +
        'rejected one.',
    ).toBe(KEEP);
    expect(
      sent,
      'and the write has to carry the whole entry, because the file is one: a call with this key ' +
        'alone would overwrite the lead and the spend limit with whatever the window held',
    ).toMatchObject({ defaultLead: '', defaultBudgetUsd: 75, navCollapsed: false });
  });
});
