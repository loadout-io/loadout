import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAgentApps } from '../../state/agent-apps';
import { AgentAppsStatus } from './agent-apps-status';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const SIGN_IN = 'Sign-in is checked when you first run an agent.';

function occurrences(text: string, fragment: string): number {
  return text.split(fragment).length - 1;
}

function markup(): string {
  return renderToStaticMarkup(<AgentAppsStatus />);
}

describe('the one agent app status in the persistent footer', () => {
  beforeEach(() => {
    invoked.mockReset();
    useAgentApps.setState({
      claudeCode: { state: 'checking' },
      codex: { state: 'checking' },
    });
  });

  it('renders every public state from the real global store', () => {
    let visible = markup();
    expect(visible).toContain('Checking Claude Code…');
    expect(visible).toContain('Checking Codex…');
    expect(occurrences(visible, SIGN_IN)).toBe(1);
    expect(visible).not.toContain('Retry');

    useAgentApps.setState({
      claudeCode: { state: 'found', version: 'claude-code 4.3.2' },
      codex: { state: 'found', version: 'codex-cli 9.8.7' },
    });
    visible = markup();
    expect(visible).toContain('Claude Code · claude-code 4.3.2');
    expect(visible).toContain('Codex · codex-cli 9.8.7');
    expect(occurrences(visible, SIGN_IN)).toBe(1);
    expect(visible).not.toContain('Retry');

    useAgentApps.setState({
      claudeCode: { state: 'not-found' },
      codex: { state: 'could-not-check' },
    });
    visible = markup();
    expect(visible).toContain('Claude Code wasn&#x27;t found.');
    expect(visible).toContain('Loadout couldn&#x27;t check Codex.');
    expect(occurrences(visible, SIGN_IN)).toBe(1);
    expect(occurrences(visible, 'Retry')).toBe(1);
    expect(visible.toLowerCase()).not.toContain('ready');
  });

  it('shares an overlapping check and keeps the valid app beside a malformed one', async () => {
    let complete: (value: unknown) => void = () => undefined;
    invoked.mockReturnValueOnce(
      new Promise((resolve) => {
        complete = resolve;
      }),
    );

    const first = useAgentApps.getState().check();
    const second = useAgentApps.getState().check();
    expect(second).toBe(first);
    expect(invoked).toHaveBeenCalledTimes(1);
    expect(invoked).toHaveBeenCalledWith('check_agent_apps');

    complete([
      { app: 'claude-code', state: 'found', version: 'claude first' },
      { app: 'claude-code', state: 'not-found' },
      { app: 'codex', state: 'found', version: 'codex intact' },
    ]);
    await first;

    expect(useAgentApps.getState().claudeCode).toEqual({ state: 'could-not-check' });
    expect(useAgentApps.getState().codex).toEqual({ state: 'found', version: 'codex intact' });
  });

  it('turns null, rejection, missing and unknown entries into local failures', async () => {
    invoked.mockResolvedValueOnce(null);
    await useAgentApps.getState().check();
    expect(useAgentApps.getState().claudeCode.state).toBe('could-not-check');
    expect(useAgentApps.getState().codex.state).toBe('could-not-check');

    invoked.mockRejectedValueOnce('the boundary refused');
    await useAgentApps.getState().check();
    expect(useAgentApps.getState().claudeCode.state).toBe('could-not-check');
    expect(useAgentApps.getState().codex.state).toBe('could-not-check');

    invoked.mockResolvedValueOnce([
      { app: 'codex', state: 'found', version: 'codex after missing Claude' },
    ]);
    await useAgentApps.getState().check();
    expect(useAgentApps.getState().claudeCode.state).toBe('could-not-check');
    expect(useAgentApps.getState().codex).toEqual({
      state: 'found',
      version: 'codex after missing Claude',
    });

    invoked.mockResolvedValueOnce([
      { app: 'claude-code', state: 'something-new' },
      { app: 'codex', state: 'found', version: 'codex after unknown Claude' },
    ]);
    await useAgentApps.getState().check();
    expect(useAgentApps.getState().claudeCode.state).toBe('could-not-check');
    expect(useAgentApps.getState().codex).toEqual({
      state: 'found',
      version: 'codex after unknown Claude',
    });
  });
});
