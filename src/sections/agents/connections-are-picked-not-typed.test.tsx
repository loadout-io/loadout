/* Połączenia agenta się WYBIERA z biblioteki projektu, a nie wpisuje z pamięci (2026-09-14).
 *
 * PO CO TO ISTNIEJE. Właściciel, 2026-09-11: „a nie że ja mam sam pisać". Pole Connections było
 * `<input>` z nazwami po przecinku, więc literówka albo nazwa połączenia, którego w bibliotece
 * tego projektu nie ma, przechodziła zapis bez słowa — a odmawiał dopiero Start rozmowy, długo
 * po tym, jak człowiek zamknął formularz. Poprzedni bieg (`conn-seed`) dał oknu listę nazw
 * WŁĄCZONYCH połączeń tego projektu i zasiał nimi nowego agenta; ta sama lista jest tutaj
 * jedynym źródłem pozycji do wyboru (niezmiennik 23: druga droga do niej to druga prawda).
 *
 * CO SĄDZIMY: co człowiek widzi i czym klika w prawdziwie uruchomionej aplikacji, a potem to,
 * co po naciśnięciu `Save` naprawdę pojechało na dysk (niezmiennik 29). Wartość zwrócona przez
 * komponent nie mówi nic o tym, czy da się odznaczyć nazwę, której tu nie ma.
 *
 * TRZECIA POZYCJA JEST CAŁYM SEDNEM. `gone` niesie zapisany agent, a biblioteka go nie zna —
 * i taka nazwa ma ZOSTAĆ widoczna, zaznaczona i podpisana, zamiast zniknąć po cichu przy
 * pierwszym zapisie. Cicha ucieczka wygląda z zewnątrz dokładnie tak samo jak zapis, który się
 * udał, a odbiera człowiekowi jedyną chwilę, w której mógłby zauważyć literówkę.
 *
 * DZIŚ: pod nazwą dostępną „Connections" stoi `textbox`, więc listy o tej nazwie jest zero
 * i nie ma ani jednej pozycji do kliknięcia.
 */
import { afterAll, expect, it } from 'vitest';
import type { Locator } from '@playwright/test';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/connections-picked';

/** Zapisany agent niosący JEDNĄ nazwę z biblioteki i jedną, której ta biblioteka nie zna. */
const AGENT = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000091',
  name: 'Researcher',
  summary: 'Reads the design and answers',
  instructions: 'Read the design and say what it asks for.',
  color: 'slate',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'look-only',
  reachesTheWeb: false,
  giveUpAfterMinutes: 20,
  writeResultsTo: '',
  tools: 'everything',
  skills: [],
  connections: ['figma', 'gone'],
};

const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

/** Jedna pozycja listy tak, jak widzi ją człowiek: napis i to, czy jest zaznaczona. */
interface Offered {
  readonly name: string;
  readonly text: string;
  readonly picked: boolean;
}

/** Wszystkie pozycje listy, w kolejności, prosto z dokumentu tej karty. */
function offered(list: Locator): Promise<Offered[]> {
  return list.locator('option').evaluateAll((nodes) =>
    nodes.map((node) => {
      const one = node as HTMLOptionElement;
      return {
        name: one.value,
        text: (one.textContent ?? '').replace(/\s+/g, ' ').trim(),
        picked: one.selected,
      };
    }),
  );
}

it("picks an agent's connections from this project library and lets a name that is not there be taken off", async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Connections picked' }]),
      list_connections: replies(['figma', 'linear-server']),
      list_agents: replies([
        { kind: 'healthy', value: AGENT, path: 'researcher.md', revision: 'initial' },
      ]),
      save_agent: replies('saved'),
    },
  });
  try {
    await app.page.locator('[data-section-switch="agents"]').click();
    await app.page.locator(`[data-agent="${AGENT.id}"]`).click();
    await app.page.getByRole('button', { name: 'More settings', exact: true }).click();

    const list = app.page.getByRole('listbox', { name: 'Connections', exact: true });
    await expect.poll(() => list.count(), { timeout: 10_000 }).toBe(1);

    /* Pozycje przyjeżdżają z dysku, więc czekamy na ich liczbę, a nie na chwilę. Samo czekanie
       NIE jest asercją: co w nich stoi, sądzą trzy pytania niżej. */
    await expect.poll(async () => (await offered(list)).length, { timeout: 10_000 }).toBe(3);

    const shown = await offered(list);
    expect(
      shown.map((one) => one.name),
      'the Connections list offers something other than the two this project has turned on ' +
        'plus the one this agent carries. A library name missing here cannot be picked at all, ' +
        'and a carried name missing here leaves silently the first time anybody presses Save',
    ).toEqual(['figma', 'linear-server', 'gone']);
    expect(
      shown.find((one) => one.name === 'figma')?.picked,
      'figma is written in this agent and stands unticked, so opening the form and saving it ' +
        'again would take it away',
    ).toBe(true);
    expect(
      shown.find((one) => one.name === 'linear-server')?.picked,
      'linear-server is turned on in this project and nobody picked it for this agent, yet it ' +
        'comes ticked',
    ).toBe(false);
    expect(
      shown.find((one) => one.name === 'gone'),
      'gone is written in this agent and this project has nothing of that name. It has to stand ' +
        'here ticked and marked, so a person can see the typo and take it off by hand',
    ).toEqual({ name: 'gone', text: 'gone (not in this project)', picked: true });

    /* WYSOKOŚĆ MIERZONA W PRAWDZIWYM CHROMIUM, bo tylko tam ją widać. Lista wysoka na jeden
       wiersz jest pierwszym wierszem listy, nie listą: reszta pozycji jest wtedy za krawędzią
       i trzeba zgadnąć, że tam są. Porównanie idzie z sąsiednim polem tego samego formularza,
       więc nie powtarza tu żadnej liczby z arkusza. */
    const tall = (await list.boundingBox())?.height ?? 0;
    const oneLine = (await app.page.locator('#agent-skills').boundingBox())?.height ?? 0;
    expect(
      oneLine,
      'the one-line field next to this list has no height at all, so the comparison below would ' +
        'pass on two zeroes',
    ).toBeGreaterThan(0);
    expect(
      tall,
      'the Connections list is no taller than the single-line field above it, so it shows one ' +
        'entry and hides the rest below its own edge',
    ).toBeGreaterThan(oneLine * 2);

    await list.selectOption(['figma', 'linear-server']);
    await app.page.getByRole('button', { name: 'Save', exact: true }).click();

    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'save_agent').length, {
        timeout: 10_000,
      })
      .toBe(1);
    const saved = (await app.calls()).find((one) => one.cmd === 'save_agent')?.args['agent'] as
      { readonly connections?: readonly string[] } | undefined;
    expect(
      saved?.connections,
      'ticking linear-server and unticking gone reached the disk as something else. What the ' +
        'list shows and what the file gets are then two different answers to one question',
    ).toEqual(['figma', 'linear-server']);
  } finally {
    await app.close();
  }
}, 90_000);
