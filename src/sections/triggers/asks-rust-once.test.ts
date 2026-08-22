import { readFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ipcSource, windowSideArguments } from '../ipc-signature';
import type { TriggerDraft, TriggerSnapshot } from './io';

const { invoked } = vi.hoisted(() => ({ invoked: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const io = await import('./io');
const GOLDEN = new URL('../../../src-tauri/commands.golden.txt', import.meta.url);
const KEY = 'lin_api_1234567890123456789012345678901234567890';
const DRAFT: TriggerDraft = {
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  workspace: '/project',
  pollEveryMinutes: 5,
  apiKey: KEY,
};
const EXPECTED: TriggerSnapshot = {
  slug: 'linear-0198ca82-ded0-7000-8000-000000000074',
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  workspace: '/project',
  enabled: true,
  pollEveryMinutes: 5,
  hasApiKey: true,
};

interface Edge {
  readonly exported: string;
  readonly command: string;
  readonly rustArguments: readonly string[];
  readonly sent: readonly unknown[];
  readonly call: () => Promise<unknown>;
}

const EDGES: readonly Edge[] = [
  {
    exported: 'listTriggers',
    command: 'list_triggers',
    rustArguments: [],
    sent: ['list_triggers'],
    call: () => io.listTriggers(),
  },
  {
    exported: 'setTriggerEnabled',
    command: 'set_trigger_enabled',
    rustArguments: ['slug', 'enabled'],
    sent: ['set_trigger_enabled', { slug: 'assigned-to-me', enabled: false }],
    call: () => io.setTriggerEnabled('assigned-to-me', false),
  },
  {
    exported: 'checkTrigger',
    command: 'check_trigger',
    rustArguments: ['slug'],
    sent: ['check_trigger', { slug: 'assigned-to-me' }],
    call: () => io.checkTrigger('assigned-to-me'),
  },
  {
    exported: 'retryTrigger',
    command: 'retry_trigger',
    rustArguments: ['slug'],
    sent: ['retry_trigger', { slug: 'assigned-to-me' }],
    call: () => io.retryTrigger('assigned-to-me'),
  },
  {
    exported: 'createTrigger',
    command: 'create_trigger',
    rustArguments: ['draft'],
    sent: ['create_trigger', { draft: DRAFT }],
    call: () => io.createTrigger(DRAFT),
  },
  {
    exported: 'updateTrigger',
    command: 'update_trigger',
    rustArguments: ['slug', 'expected', 'draft'],
    sent: ['update_trigger', { slug: EXPECTED.slug, expected: EXPECTED, draft: DRAFT }],
    call: () => io.updateTrigger(EXPECTED.slug, EXPECTED, DRAFT),
  },
  {
    exported: 'deleteTrigger',
    command: 'delete_trigger',
    rustArguments: ['slug', 'expected'],
    sent: ['delete_trigger', { slug: EXPECTED.slug, expected: EXPECTED }],
    call: () => io.deleteTrigger(EXPECTED.slug, EXPECTED),
  },
  {
    exported: 'testLinearConnection',
    command: 'test_linear_connection',
    rustArguments: ['slug', 'apiKey'],
    sent: ['test_linear_connection', { slug: null, apiKey: KEY }],
    call: () => io.testLinearConnection(null, KEY),
  },
];

const known = readFileSync(GOLDEN, 'utf8')
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line !== '' && !line.startsWith('#'));
const rust = ipcSource();

describe('every Triggers edge asks Rust once, by its registered name and arguments', () => {
  beforeEach(() => invoked.mockClear());

  it('covers every function exported by the Triggers edge', () => {
    const exported = Object.entries(io)
      .filter(([, value]) => typeof value === 'function')
      .map(([name]) => name)
      .sort();
    expect(EDGES.map((edge) => edge.exported).sort()).toEqual(exported);
  });

  for (const edge of EDGES) {
    it(edge.exported + ' calls ' + edge.command + ' exactly once', async () => {
      expect(known.length, 'the golden command list was empty').toBeGreaterThan(0);
      expect(known).toContain(edge.command);
      expect(windowSideArguments(rust, edge.command)).toEqual(edge.rustArguments);

      await edge.call();
      expect(invoked).toHaveBeenCalledTimes(1);
      expect(invoked).toHaveBeenCalledWith(...edge.sent);
    });
  }
});
