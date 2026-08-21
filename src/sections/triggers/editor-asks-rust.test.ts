/* AC-6 dla T-74: cztery nowe akcje mają jedną krawędź, literalną nazwę i komplet argumentów.
 * Funkcje są wykonywane na atrapie `invoke`; grep po źródle przechodziłby dla martwej gałęzi.
 * Lista przypadków porównuje się z nowymi eksportami io, więc piąty eksport bez sędziego pali. */
import { readFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ipcSource, windowSideArguments } from '../ipc-signature';
import type { TriggerDraft, TriggerSnapshot } from './io';

const { invoked } = vi.hoisted(() => ({ invoked: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const io = await import('./io');
const GOLDEN = new URL('../../../src-tauri/commands.golden.txt', import.meta.url);
const WIRED = new URL('../commands-wired.test.ts', import.meta.url);

const KEY = 'lin_api_1234567890123456789012345678901234567890';
const DRAFT: TriggerDraft = {
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  pollEveryMinutes: 5,
  apiKey: KEY,
};
const EXPECTED: TriggerSnapshot = {
  slug: 'linear-0198ca82-ded0-7000-8000-000000000074',
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
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

const LEGACY = new Set(['listTriggers', 'setTriggerEnabled', 'checkTrigger']);
const known = readFileSync(GOLDEN, 'utf8')
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line !== '' && !line.startsWith('#'));
const rust = ipcSource();

describe('every Linear editor action crosses its one named Rust edge', () => {
  beforeEach(() => invoked.mockClear());

  it('has an executed case for every non-legacy function exported by trigger io', () => {
    const editorExports = Object.entries(io)
      .filter(([, value]) => typeof value === 'function')
      .map(([name]) => name)
      .filter((name) => !LEGACY.has(name))
      .sort();
    expect(EDGES.map((edge) => edge.exported).sort()).toEqual(editorExports);
  });

  it('also places all four real calls in the repository-wide EDGES table', () => {
    const source = readFileSync(WIRED, 'utf8');
    for (const edge of EDGES) {
      const row = new RegExp(
        `where:\\s*['"]triggers['"][\\s\\S]*?what:\\s*['"]${edge.exported}['"][\\s\\S]*?command:\\s*['"]${edge.command}['"]`,
      );
      expect(source, `${edge.exported} has no triggers row in commands-wired.test.ts`).toMatch(row);
    }
  });

  for (const edge of EDGES) {
    it(`${edge.exported} calls ${edge.command} once with every named argument`, async () => {
      expect(known.length, 'the golden command list was empty').toBeGreaterThan(0);
      expect(known).toContain(edge.command);
      expect(windowSideArguments(rust, edge.command)).toEqual(edge.rustArguments);

      let refusal: unknown = null;
      try {
        await edge.call();
      } catch (error) {
        refusal = error;
      }
      expect(
        refusal instanceof Error ? refusal.message : String(refusal ?? ''),
        `${edge.exported} is still only a red-before skeleton`,
      ).not.toContain('not implemented');
      expect(invoked).toHaveBeenCalledTimes(1);
      expect(invoked).toHaveBeenCalledWith(...edge.sent);
    });
  }

  it('carries the key only on explicit Test or Save requests, never Delete', async () => {
    for (const edge of EDGES) {
      try {
        await edge.call();
      } catch {
        // The explicit assertions below distinguish a skeleton from a real request.
      }
    }
    const calls = invoked.mock.calls.map((call) => JSON.stringify(call));
    expect(calls.filter((call) => call.includes(KEY))).toHaveLength(3);
    expect(calls.find((call) => call.includes('delete_trigger'))).not.toContain(KEY);
  });
});
