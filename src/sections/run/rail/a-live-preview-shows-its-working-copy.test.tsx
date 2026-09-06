/* WF-25: prawdziwa droga list_processes → refreshStarted → otwarty panel.
 * Atrapa zastępuje wyłącznie transport Tauri. Nie podajemy danych wprost komponentowi:
 * pole zgubione w adapterze albo porównaniu magazynu ma zgasić to kryterium. */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { registry, invoked } = vi.hoisted(() => {
  const registry: { rows: readonly unknown[] } = { rows: [] };
  return {
    registry,
    invoked: vi.fn((command: string): Promise<unknown> =>
      Promise.resolve(command === 'list_processes' ? registry.rows : null),
    ),
  };
});
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { closeStarted, openStarted, refreshStarted, startedThings, stopStarted } =
  await import('./processes');
const { StartedThings } = await import('./rail');

const CWD = '/projects/app/.loadout/runs/01950000-0000-7000-8000-000000000001/work/s_preview';
const SERVICE = {
  workspace: '/projects/app',
  run_id: '01950000-0000-7000-8000-000000000001',
  node_key: 's_preview',
  service_id: '01950000-0000-7000-8000-000000000002',
  generation: 1,
};
function row(lifetime: string = 'window', cwd: string = CWD): Record<string, unknown> {
  return {
    pgid: 4242,
    command: 'npm run dev',
    alive: true,
    said: 'Listening',
    service: SERVICE,
    cwd,
    lifetime,
  };
}
async function panel(): Promise<string> {
  await refreshStarted();
  const one = startedThings()[0];
  expect(one, 'the real list_processes reply did not reach the list').toBeDefined();
  openStarted(one?.id ?? 'missing');
  return renderToStaticMarkup(<StartedThings />);
}
beforeEach(async () => {
  registry.rows = [];
  await refreshStarted();
  closeStarted();
  invoked.mockClear();
});

describe('a live preview explains the folder it keeps and when it stops', () => {
  it('shows the isolated working folder and window lifetime after the real refresh', async () => {
    registry.rows = [row()];
    const html = await panel();
    expect(invoked).toHaveBeenCalledWith('list_processes', { opened: null });
    expect(html).toContain(CWD);
    expect(html).toContain('Working folder');
    expect(html).toContain('Keeps running after the workflow ends.');
    expect(html).toContain('Stop it here or close the window.');
    expect(html).toContain('Stop</button>');
  });

  it('shows a run-owned service without promising it will survive the run', async () => {
    registry.rows = [row('run')];
    const html = await panel();
    expect(html).toContain('Stops when this workflow ends.');
    expect(html).not.toContain('Keeps running after the workflow ends.');
    expect(html).toContain(CWD);
  });

  it('publishes changed ownership facts even when command, process and output did not change', async () => {
    registry.rows = [row()];
    await panel();
    const before = startedThings();
    registry.rows = [row('run', CWD + '-new')];
    await refreshStarted();
    expect(startedThings(), 'ownership-only changes must wake the actual panel').not.toBe(before);
    const html = renderToStaticMarkup(<StartedThings />);
    expect(html).toContain(CWD + '-new');
    expect(html).toContain('Stops when this workflow ends.');
  });

  it('preserves stable ownership and the same store snapshot on an unchanged refresh', async () => {
    registry.rows = [row()];
    await panel();
    const before = startedThings();
    registry.rows = [{ ...row(), service: { ...SERVICE } }];
    await refreshStarted();
    expect(startedThings()[0]).toMatchObject({ service: SERVICE, cwd: CWD, lifetime: 'window' });
    expect(startedThings()).toBe(before);
  });

  it('does not invent an isolated folder or a workflow lifetime for a legacy manual command', async () => {
    registry.rows = [{ pgid: 4242, command: 'npm run dev', alive: true, said: '' }];
    const html = await panel();
    expect(html).toContain('npm run dev');
    expect(html).toContain('Stop</button>');
    expect(html).not.toContain('Working folder');
    expect(html).not.toContain('workflow ends');
  });

  it('does not promise window lifetime for an unknown future lifetime', async () => {
    registry.rows = [row('future-scope')];
    const html = await panel();
    expect(html).toContain(CWD);
    expect(html).toContain('When this stops is not known.');
    expect(html).not.toContain('Keeps running after the workflow ends.');
  });
});

describe('readiness belongs to the exact running instance', () => {
  it('sends the exact service identity when the visible Stop is used', async () => {
    registry.rows = [row()];
    expect(await panel()).toContain('Stop</button>');
    const current = startedThings()[0];
    await stopStarted(current?.id ?? 'missing');
    expect(invoked).toHaveBeenCalledWith('stop_process', { pgid: 4242, service: SERVICE });
  });

  it('does not let an old panel address a new generation that reused its process number', async () => {
    registry.rows = [row()];
    await panel();
    const old = startedThings()[0]?.id ?? 'missing';
    registry.rows = [{ ...row(), service: { ...SERVICE, generation: 2 } }];
    await refreshStarted();
    const current = startedThings()[0]?.id;
    invoked.mockClear();
    await stopStarted(old);
    expect(current, 'a new generation reused the old clickable identity').not.toBe(old);
    expect(invoked).not.toHaveBeenCalledWith('stop_process', expect.anything());
    expect(startedThings()).toHaveLength(1);
  });
  it('shows waiting as the backend described it, not as ready', async () => {
    registry.rows = [
      {
        ...row(),
        readiness: { state: 'waiting', message: 'Waiting for the app to respond.' },
        endpoints: [],
      },
    ];
    const html = await panel();
    expect(html).toContain('Waiting for the app to respond.');
    expect(html).not.toContain('Ready to use.');
  });

  it('shows the confirmed named endpoint after readiness-only changes', async () => {
    registry.rows = [
      {
        ...row(),
        readiness: { state: 'waiting', message: 'Waiting for the app to respond.' },
        endpoints: [],
      },
    ];
    await panel();
    const before = startedThings();
    registry.rows = [
      {
        ...row(),
        readiness: { state: 'ready', message: 'Ready to use.' },
        endpoints: [
          {
            service: SERVICE,
            name: 'web',
            host: '127.0.0.1',
            port: 8123,
            url: 'http://127.0.0.1:8123',
            state: 'ready',
          },
        ],
      },
    ];
    await refreshStarted();
    expect(
      startedThings(),
      'readiness must update even if PID/output/command are unchanged',
    ).not.toBe(before);
    const html = renderToStaticMarkup(<StartedThings />);
    expect(html).toContain('Ready to use.');
    expect(html).toContain('http://127.0.0.1:8123');
    expect(html).toContain('web');
    expect(html).not.toContain('Waiting for the app to respond.');
  });

  it('does not keep an old ready sentence when the instance stops responding', async () => {
    registry.rows = [
      { ...row(), readiness: { state: 'ready', message: 'Ready to use.' }, endpoints: [] },
    ];
    await panel();
    registry.rows = [
      {
        ...row(),
        readiness: { state: 'failed', message: 'The app stopped before it was ready.' },
        endpoints: [],
      },
    ];
    await refreshStarted();
    const html = renderToStaticMarkup(<StartedThings />);
    expect(html).toContain('The app stopped before it was ready.');
    expect(html).not.toContain('Ready to use.');
  });
});
