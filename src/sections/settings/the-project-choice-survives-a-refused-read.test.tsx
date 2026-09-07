/* Karta „Project instructions", kiedy Rust odmawia odczytu (2026-09-07, WF-12).
 *
 * PO CO TO ISTNIEJE. Podgląd źródeł po stronie Rusta chodzi po całym drzewie projektu i wywraca
 * się w CAŁOŚCI na pierwszym pliku, którego nie da się otworzyć — dowiązanie `AGENTS.md ->
 * CLAUDE.md`, podkatalog bez prawa odczytu, folder zabrany spod okna. Karta pokazywała wtedy
 * odmowę i pod nią, na zawsze, zdanie o trwającym odczycie, a kontrolki nie było ŻADNEJ. To
 * jest jedyne miejsce w aplikacji z przełącznikiem projektu i wyborem lidera, więc człowiek
 * z włączoną opcją i jednym nieczytelnym plikiem tracił naraz podgląd i wyłącznik: bieg
 * odmawiał startu, a wyjściem była ręczna edycja `.loadout/project.json`.
 *
 * SŁABA WERSJA: sprawdzić, że na ekranie stoi zdanie odmowy. Przechodzi ją dokładnie ta karta,
 * która zamykała człowieka w pułapce — ona to zdanie pokazywała. Druga słaba wersja: sprawdzić,
 * że kontrolka ISTNIEJE. Przechodzi ją ptaszek pinowany na `false`, który umie wysłać wyłącznie
 * „włącz", więc wyłączenie dalej jest nieosiągalne. Dlatego kryterium sądzi drogę do końca:
 * kontrolka jest, zdanie o trwającym odczycie zniknęło, a wybrane „nie" DOCHODZI do granicy
 * jako `enabled: false` i karta wraca do wyboru z pliku.
 */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/Users/somebody/Projects/instruction-fixture';

/** Zdanie, które Rust naprawdę pisze przy dowiązaniu: `open_regular_file` idzie z `O_NOFOLLOW`. */
const REFUSED =
  'Project instructions at AGENTS.md could not be read: too many levels of symbolic links';

/** To, co plik oddaje po udanym zapisie wyłączenia — a więc po wyjściu z pułapki. */
const SETTINGS = {
  instructions: { enabled: false, includeLocal: false },
  leadInstructions: null,
  sources: [],
  limits: { files: 256, fileBytes: 65536, totalBytes: 524288 },
};

/** Ta sama odpowiedź na każde wywołanie: karta pyta raz, ale kolejka nie może się wyczerpać. */
function always(reply: TauriReply): readonly TauriReply[] {
  return Array.from({ length: 8 }, () => reply);
}

afterAll(async () => {
  await closeEverything();
}, 30_000);

it('keeps a choice that reaches Rust when this project’s files cannot be read', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: always({
        value: [{ id: PROJECT, folder: PROJECT, name: 'Instruction fixture' }],
      }),
      read_project_settings: always({ error: REFUSED }),
      save_project_settings: always({ value: SETTINGS }),
    },
  });
  try {
    await app.page.locator('[data-section-switch="settings"]').click();
    await app.page.locator('[data-settings-screen]').waitFor({ state: 'visible' });
    const section = app.page.getByRole('region', { name: 'Project instructions', exact: true });
    await section.waitFor({ state: 'visible' });

    /* Scena jest naprawdę tą odmówioną, a nie kartą, która czegoś nie zdążyła zapytać. */
    await expect.poll(async () => section.getByRole('alert').count()).toBe(1);
    expect(await section.textContent()).toContain(REFUSED);
    expect(
      await section.textContent(),
      'the card says a read is still going while it is showing why that read was refused, so ' +
        'the person reads two contradictory sentences and waits for something that never comes',
    ).not.toContain('Reading this project');

    const choice = app.page.getByRole('combobox', {
      name: 'Use project instructions',
      exact: true,
    });
    expect(
      await choice.count(),
      'a refused read leaves the card with no control at all: the only switch for this project ' +
        'and the only choice for the lead live here, so the person cannot turn the option off ' +
        'and every run keeps being refused',
    ).toBe(1);
    expect(
      await choice.inputValue(),
      'the card claims to know a value it never read, so a project whose file says "on" reads ' +
        'as "off" on screen',
    ).toBe('');

    await choice.selectOption('off');
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'save_project_settings'))
      .toHaveLength(1);
    const call = (await app.calls()).find((one) => one.cmd === 'save_project_settings');
    expect(
      call?.args,
      'the way out of the trap never reaches the file: a control pinned to one value can only ' +
        'ever ask for the other one, so "off" is unreachable no matter how many times it is used',
    ).toEqual({ folder: PROJECT, patch: { instructions: { enabled: false } } });

    /* Po udanym zapisie karta wraca do tego, co stoi w pliku — bez ponownego odczytu. */
    const checkbox = app.page.getByRole('checkbox', {
      name: 'Use project instructions',
      exact: true,
    });
    await expect.poll(async () => checkbox.count()).toBe(1);
    expect(await checkbox.isChecked()).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
