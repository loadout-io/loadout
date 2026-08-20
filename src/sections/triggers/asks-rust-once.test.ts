import { readFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ipcSource, windowSideArguments } from '../ipc-signature';

const { invoked } = vi.hoisted(() => ({ invoked: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const io = await import('./io');
const GOLDEN = new URL('../../../src-tauri/commands.golden.txt', import.meta.url);

describe('the Triggers edge asks Rust once, by the registered name and argument', () => {
  beforeEach(() => invoked.mockClear());

  it('executes every exported function and sends the slug exactly once', async () => {
    const known = readFileSync(GOLDEN, 'utf8')
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line !== '' && !line.startsWith('#'));
    expect(known.length, 'the golden command list was empty').toBeGreaterThan(0);
    expect(known).toContain('check_trigger');
    expect(windowSideArguments(ipcSource(), 'check_trigger')).toEqual(['slug']);

    await io.checkTrigger('assigned-to-me');
    expect(invoked).toHaveBeenCalledTimes(1);
    expect(invoked).toHaveBeenCalledWith('check_trigger', { slug: 'assigned-to-me' });
  });
});
