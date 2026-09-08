/* CT-08: odmowa po usunięciu przypiętego zestawu musi trafić do trwałego strumienia karty.
 * Backendowe kryterium sprawdza źródło zdania; tutaj sprawdzamy ostatnią, ludzką granicę
 * UI zgodnie z niezmiennikiem 29. */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';
import type { StartProps } from './start';

const STEP = 'Only step';
const SET = 'Deleted brief';
const REVISION = 'revision-deleted';
const REFUSAL =
  `${STEP} cannot start because reference material ${SET} version ${REVISION} is missing or ` +
  'damaged (Loadout has no context set saved under that name.). Restore it or choose another ' +
  'ready version.';

const { invoked, refusal } = vi.hoisted(() => {
  const refusal = { sentence: '' };
  return {
    refusal,
    invoked: vi.fn((command: string) =>
      command === 'run_workflow' ? Promise.reject(refusal.sentence) : Promise.resolve(undefined),
    ),
  };
});

refusal.sentence = REFUSAL;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { seen } = vi.hoisted(() => ({ seen: [] as unknown[] }));

vi.mock('./start', async (importOriginal) => {
  const real = await importOriginal<typeof import('./start')>();
  return {
    ...real,
    Start: (props: StartProps) => {
      seen.push(props);
      return real.Start(props);
    },
  };
});

const Run = (await import('./index')).default;
const { launchRun } = await import('./launch');
const { useWorkspaces } = await import('../../state/workspaces');

const HERE = {
  id: '/work/deleted-context',
  name: 'Deleted context',
  folder: '/work/deleted-context',
};
const CHOICE: Choice = {
  path: 'deleted-context.json',
  name: 'Deleted context',
  steps: [{ id: 'only', name: STEP, state: 'pending' }],
};

useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
const before = renderToStaticMarkup(<Run />);
const channel = (seen.at(-1) as StartProps | undefined)?.onSaid;
const said = await launchRun(CHOICE, 1);
if (typeof channel === 'function') channel(said);
const after = renderToStaticMarkup(<Run />);

function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function streamOf(markup: string): string {
  const opens = markup.indexOf('data-stream-column');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const closes = rest.indexOf('data-plan-column', 1);
  return readable(closes < 0 ? rest : rest.slice(0, closes));
}

describe('a deleted Context pin refuses Start in the run stream', () => {
  it('keeps the backend sentence intact across the Start boundary', () => {
    expect(said).toBe(REFUSAL);
    expect(REFUSAL).toContain(STEP);
    expect(REFUSAL).toContain(SET);
    expect(REFUSAL).toContain('Restore it or choose another ready version.');
  });

  it('does not claim the refusal before Start', () => {
    expect(streamOf(before)).not.toBe('');
    expect(streamOf(before)).not.toContain(REFUSAL);
  });

  it('shows the actionable refusal where the person reads the run', () => {
    expect(typeof channel).toBe('function');
    const stream = streamOf(after);
    expect(stream).not.toBe('');
    expect(stream).toContain(REFUSAL);
  });
});
