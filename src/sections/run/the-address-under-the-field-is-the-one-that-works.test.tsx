/* Adres, który wiersz pod polem każe wpisać, musi być adresem, który Enter naprawdę przyjmuje.
 *
 * DWIE WADY JEDNEGO ROZBIORU, obie widoczne dla człowieka i obie kończące się tym samym zdaniem
 * odmowy o niedostępnym kanale:
 *
 *  1. Ekran obiecywał lidera, a zdanie wracało odmową. Wołający podawał rozbiorowi nazwy
 *     WSZYSTKICH kafelków planu, więc zdanie zaczynające się nazwą kroku, który nigdy nie otworzył
 *     kanału, schodziło na ścieżkę wysyłki do agenta i odbijało się od braku sesji. Po biegu
 *     kroki zostają, a kanały są zerowane — czyli w chwili, gdy nic nie biegnie i wiersz pod polem
 *     wprost obiecuje lidera, pytanie „Backend is still broken…" nie docierało do nikogo.
 *
 *  2. Krok z kopiami był nieosiągalny KAŻDYM adresem, który pokazuje ekran. Bieg rejestruje go
 *     jako „Builder (2 of 3)", wiersz każe od tego zacząć linię, a rozbiór czytał wyłącznie
 *     pierwsze słowo — „Builder", którego nie nosi żadna kopia.
 *
 * Kryterium idzie przez PRAWDZIWY komponent i jego prawdziwy handler, bo obie wady mieszkają na
 * szwie: pierwsza w tym, CO ekran podaje rozbiorowi, druga w tym, jak rozbiór to czyta. Test
 * czystej funkcji zobaczyłby tylko drugą połowę.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import type { EntryProps } from './entry/entry';
import type { Step } from '../../state/run';

const { seen, pumps, invoke } = vi.hoisted(() => ({
  seen: [] as EntryProps[],
  pumps: [] as Array<{ onmessage: ((batch: unknown[]) => void) | null }>,
  invoke: vi.fn<(name: string, args: Record<string, unknown>) => Promise<unknown>>(
    async () => undefined,
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    public onmessage: ((batch: unknown[]) => void) | null = null;
    public constructor() {
      pumps.push(this);
    }
  },
}));
vi.mock('./entry/entry', async (importOriginal) => {
  const real = await importOriginal<typeof import('./entry/entry')>();
  return {
    ...real,
    Entry: (props: EntryProps) => {
      seen.push(props);
      return real.Entry(props);
    },
  };
});

const { default: Run } = await import('./index');
const { openChat } = await import('./io');
const { runFor } = await import('../../state/run');
const { useWorkspaces } = await import('../../state/workspaces');
const FOLDER = '/work/addressing';
const RUN = '01980000-0000-7000-8000-000000000001';

afterEach(() => {
  runFor(FOLDER).getState().nowRunning('', []);
  seen.length = 0;
  invoke.mockClear();
});

/** Bieg z tym planem i tymi otwartymi kanałami, narysowany raz. */
async function screen(steps: readonly Step[], channels: readonly unknown[]): Promise<void> {
  useWorkspaces.setState({
    activeId: FOLDER,
    all: [{ id: FOLDER, name: 'Messages', folder: FOLDER }],
  });
  const store = runFor(FOLDER).getState();
  store.nowRunning('Work', steps, FOLDER, 'work.json', []);
  await openChat(FOLDER);
  if (channels.length > 0) pumps.at(-1)?.onmessage?.([...channels]);
  renderToStaticMarkup(<Run />);
}

it('a step that never opened a channel is an ordinary word, and the question reaches the lead whole', async () => {
  invoke.mockImplementation(async () => undefined);
  await screen([{ id: 'backend', name: 'Backend', state: 'running' }], []);
  const entry = seen.at(-1);
  expect(entry).toBeDefined();
  expect(
    entry?.talkingTo,
    'nobody has a channel here, so the row under the field promises the lead agent. Whatever ' +
      'Enter then does has to match that promise.',
  ).toEqual([]);
  const asking = 'Backend is still broken, what happened?';
  const said = await entry?.onSayToAgent(asking, []);
  expect(
    said,
    'the row promised the lead agent, so the question has to go there — not come back as a ' +
      'refusal about a channel the person never asked for. The plan is not a list of addresses.',
  ).toBeNull();
  const reached = invoke.mock.calls.find(([name]) => name === 'say_to_orchestrator');
  expect(
    reached?.[1],
    'and it goes WHOLE: the first word is part of the question, not an address, so taking it ' +
      'off would change the sentence a person wrote without saying so.',
  ).toMatchObject({ text: asking, folder: FOLDER });
  expect(invoke.mock.calls.some(([name]) => name === 'send_to_step')).toBe(false);
});

it('a copy answers to the name the row under the field prints', async () => {
  invoke.mockImplementation(async (name) =>
    name === 'send_to_step'
      ? { runId: RUN, nodeKey: 'builder~2', result: 'acceptedBySession', said: '' }
      : undefined,
  );
  const copies = [
    {
      kind: 'stepSession',
      agent: 'Builder (1 of 3)',
      runId: RUN,
      nodeKey: 'builder',
      canReceive: true,
      finished: false,
    },
    {
      kind: 'stepSession',
      agent: 'Builder (2 of 3)',
      runId: RUN,
      nodeKey: 'builder~2',
      canReceive: true,
      finished: false,
    },
  ];
  await screen([{ id: 'builder', name: 'Builder', state: 'running' }], copies);
  const entry = seen.at(-1);
  /* PISZEMY DOKŁADNIE TO, CO WIERSZ KAZAŁ NAPISAĆ. Adres bierzemy z propsa, którym ten wiersz
   * jest zbudowany, a nie z literału — inaczej kryterium sądziłoby własną fiksturę zamiast
   * obietnicy, którą czyta człowiek. */
  const advertised = entry?.talkingTo?.[1] ?? '';
  expect(
    advertised,
    'a step with copies is registered under its numbered name, and that is the name the row ' +
      'tells a person to start the line with.',
  ).toBe('Builder (2 of 3)');
  const said = await entry?.onSayToAgent(advertised + ' use tabs instead of spaces', []);
  expect(said, 'the run took it, so there is nothing to say back').toBeNull();
  expect(
    invoke,
    'the second copy is the one addressed, and the name comes OFF the sentence. Reading only ' +
      'the first word left "Builder", which no copy answers to, so the only address the screen ' +
      'ever shows was the only one that could not be delivered.',
  ).toHaveBeenCalledWith('send_to_step', {
    folder: FOLDER,
    runId: RUN,
    nodeKey: 'builder~2',
    text: 'use tabs instead of spaces',
  });
  expect(invoke.mock.calls.some(([name]) => name === 'say_to_orchestrator')).toBe(false);
});
