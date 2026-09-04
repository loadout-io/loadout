/* 2026-09: czekający trigger NAZYWA FOLDER w prawdziwym wierszu ekranu (niezmiennik 29).
 * Szablon czytamy z Rusta, żeby granica IPC nie utrzymywała drugiej kopii zdania
 * (niezmienniki 13 i 23). Brak pliku daje pusty napis i czerwień na asercji, nie przy imporcie. */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';
import { useWorkspaces } from '../../state/workspaces';
import type { TriggerIo } from './io';
import TriggersScreen from './index';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const IPC = resolve(ROOT, 'src-tauri/src/ipc.rs');

function rustTextAfter(source: string, declaration: string): string {
  const at = source.indexOf(declaration);
  if (at < 0) return '';
  const tail = source.slice(at + declaration.length);
  const closes = tail.indexOf(';');
  const literal = closes < 0 ? '' : tail.slice(0, closes).trim();
  const joined = literal.replace(/\\\r?\n\s*/g, '').trim();
  const quoted = /^"((?:[^"\\]|\\.)*)"/.exec(joined);
  return (quoted?.[1] ?? '').replace(/\\"/g, '"');
}

const rust = existsSync(IPC) ? readFileSync(IPC, 'utf8') : '';
const TEMPLATE = rustTextAfter(rust, 'const WAITING_IN_THE_FOLDER: &str =');
const WORKSPACE = { id: '/work/a', name: 'Workspace A', folder: '/work/a' };
const WAITING = TEMPLATE.replace('{name}', WORKSPACE.name);

const CLOCK: TriggerClock = {
  now: () => 0,
  setInterval: () => 1,
  clearInterval: () => undefined,
};

const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};

async function neverAsked(): Promise<never> {
  throw new Error('this screen only checks one trigger');
}

const IO: TriggerIo = {
  listTriggers: neverAsked,
  setTriggerEnabled: neverAsked,
  checkTrigger: async () => ({ status: 'busy', sentence: WAITING }),
  resumeTrigger: neverAsked,
  retryTrigger: neverAsked,
  createTrigger: neverAsked,
  updateTrigger: neverAsked,
  deleteTrigger: neverAsked,
  testLinearConnection: neverAsked,
};

const TRIGGER: TriggerView = {
  slug: 'in-a',
  source: 'Linear',
  condition: 'assigned-to-me',
  workflow: 'ship-it',
  workflowName: 'Ship it',
  workspace: WORKSPACE.folder,
  enabled: true,
  pollEveryMinutes: 1,
  hasApiKey: true,
  status: { kind: 'unchecked' },
};

function statusText(markup: string): string {
  const carrier = /<span\b[^>]*\bdata-trigger-status\b[^>]*>([\s\S]*?)<\/span>/.exec(markup);
  return (carrier?.[1] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

beforeEach(() => {
  useWorkspaces.setState({ all: [WORKSPACE], activeId: WORKSPACE.id, said: null });
});

describe('a trigger waiting for its own folder', () => {
  it('shows the Rust sentence in the activity row', async () => {
    const store = createTriggersStore(IO, CLOCK, RUN);
    store.setState({ triggers: [TRIGGER] });

    await store.getState().tick();
    const markup = renderToStaticMarkup(<TriggersScreen store={store} />);

    expect(TEMPLATE, 'the Rust waiting sentence is missing').toContain('{name}');
    expect(statusText(markup)).toBe(WAITING);
  });
});
