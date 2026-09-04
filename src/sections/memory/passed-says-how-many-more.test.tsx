/* Z-49: człowiek widzi, że sekcja pokazuje tylko wczytane biegi, i może dobrać jedną paczkę.
 *
 * Sama wartość `moreRuns` nie wystarcza: kryterium czyta zdanie i prawdziwy przycisk w markupie
 * strefy przekazań (niezmienniki 16 i 29). Akcję napędza ten sam magazyn, który przycisk
 * dostaje w produkcie; atrapa stoi dopiero na transporcie do Rusta.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Handoff, HandoffPage } from '../../state/memory';
import { useMemory } from '../../state/memory';
import NotesShelf from './shelf';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string, _args?: unknown): Promise<unknown> => Promise.resolve([])),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const FOLDER = '/Users/x/large-project';

function handoff(id: string, run: string): Handoff {
  return {
    id,
    run,
    from: 'Scout',
    to: ['Forge'],
    kind: 'findings',
    title: 'What the parser does',
    status: 'current',
    created: '2026-09-04T10:00:00Z',
    path: `.loadout/runs/${run}/handoffs/01__scout__findings.md`,
    bytes: 3174,
  };
}

const FIRST = handoff('h-first', '20260904-100000__wf_first');
const SECOND = handoff('h-second', '20260903-100000__wf_second');

function page(handoffs: Handoff[], runsRead: number, moreRuns: number): HandoffPage {
  return { handoffs, runsRead, moreRuns };
}

/** Kawałek markupu od strefy przekazań do następnej strefy. */
function passedZone(): string {
  const markup = renderToStaticMarkup(<NotesShelf store={useMemory} />);
  const start = markup.indexOf('data-zone="passed"');
  if (start < 0) return '';
  const next = markup.slice(start + 1).search(/data-zone="/);
  return next < 0 ? markup.slice(start) : markup.slice(start, start + 1 + next);
}

function buttonFor(html: string, label: string): string {
  const at = html.indexOf(label);
  if (at < 0) return '';
  const opens = html.lastIndexOf('<button', at);
  if (opens < 0) return '';
  return html.slice(opens, html.indexOf('>', opens) + 1);
}

function argsOfListCall(index: number): Record<string, unknown> | undefined {
  const calls = invoked.mock.calls.filter((call) => call[0] === 'list_handoffs');
  return calls[index]?.[1] as Record<string, unknown> | undefined;
}

beforeEach(() => {
  useMemory.setState({
    notes: [],
    catalogFolder: FOLDER,
    notesFolder: FOLDER,
    generation: 0,
    passed: [],
    passedRunsRead: 0,
    passedMoreRuns: 0,
    message: null,
    passedProblem: null,
    choice: null,
    read: true,
    pendingDiscard: null,
  });
  invoked.mockReset();
  invoked.mockImplementation((command: string): Promise<unknown> => {
    if (command === 'list_notes') return Promise.resolve([]);
    return Promise.resolve([]);
  });
});

describe('the passed-files shelf says how much history is loaded', () => {
  it('shows the first ten-run boundary and a real control when fifteen remain', () => {
    useMemory.setState({ passed: [FIRST], passedRunsRead: 10, passedMoreRuns: 15 });

    const zone = passedZone();
    expect(
      zone,
      'the sentence must stand in the shelf a person reads, not only in a paging value',
    ).toContain('Showing the last 10 runs · 15 more');
    expect(buttonFor(zone, 'Show 10 more'), 'loading more must be a real control').toContain(
      'class="btn-quiet"',
    );
  });

  it.each([3, 10])('says nothing about more history when all %i runs are loaded', (runsRead) => {
    useMemory.setState({ passed: [FIRST], passedRunsRead: runsRead, passedMoreRuns: 0 });

    const zone = passedZone();
    expect(zone).not.toContain('Showing the last');
    expect(zone).not.toContain('Show 10 more');
  });

  it('adds one ten-run page and asks for no more than that page', async () => {
    let handoffAsk = 0;
    invoked.mockImplementation((command: string): Promise<unknown> => {
      if (command === 'list_notes') return Promise.resolve([]);
      if (command === 'list_handoffs') {
        handoffAsk += 1;
        return Promise.resolve(handoffAsk === 1 ? page([FIRST], 10, 15) : page([SECOND], 10, 5));
      }
      return Promise.resolve([]);
    });

    await useMemory.getState().load(FOLDER);
    await useMemory.getState().loadMorePassed();

    expect(argsOfListCall(0)).toMatchObject({ folder: FOLDER, afterRuns: 0, howManyRuns: 10 });
    expect(
      argsOfListCall(1),
      'the control must ask for the next batch, not for the rest of the archive',
    ).toMatchObject({ folder: FOLDER, afterRuns: 10, howManyRuns: 10 });
    expect(useMemory.getState().passed.map((one) => one.id)).toEqual([FIRST.id, SECOND.id]);
    expect(passedZone()).toContain('Showing the last 20 runs · 5 more');
  });
});
