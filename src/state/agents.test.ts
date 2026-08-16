/* Kryterium 7 dla T-11: duplikat agenta jest nowym plikiem, a nie drugą nazwą tego samego
 * obiektu.
 *
 * Słaba wersja tego kryterium to `expect(list).toHaveLength(2)`. Ona przechodzi dla
 * `list.push(list[0])` — dwa wpisy, jeden obiekt. Użytkownik duplikuje agenta, poprawia
 * kopię, a razem z nią po cichu przepisuje oryginał, którego nie tknął. Dowiaduje się o tym
 * przy najbliższym biegu, po zachowaniu, nie po ekranie.
 *
 * Rozróżniają to dwie asercje: `copy.id` różny od `original.id` oraz `skills` oryginału PO
 * mutacji kopii. Druga jest ważniejsza, bo dwa różne identyfikatory da się nadać dwóm
 * odwołaniom do tego samego obiektu.
 *
 * Wszystko idzie przez wstrzyknięte `AgentsIo`. Magazyn nie zna nazw komend i nie ma tu
 * czego mockować — atrapa jest jawnym argumentem, nie podmienioną warstwą transportu.
 */
import { describe, expect, it } from 'vitest';
import type { Agent, AgentsIo } from './agents';
import { createAgentsStore } from './agents';

function forge(): Agent {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
    name: 'Forge',
    summary: 'Writes code',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    tools: 'everything',
    skills: ['rust-tauri'],
    connections: [],
    writeResultsTo: 'handoffs/build.md',
  };
}

function needle(): Agent {
  return {
    ...forge(),
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f78',
    name: 'Needle',
    summary: 'Runs the checks',
    color: 'slate',
    fileAccess: 'look-only',
    skills: [],
  };
}

interface Recorder extends AgentsIo {
  saved: Agent[];
  removed: string[];
}

function recorder(seed: Agent[]): Recorder {
  const saved: Agent[] = [];
  const removed: string[] = [];
  let minted = 0;

  return {
    saved,
    removed,
    list: () => Promise.resolve(seed),
    newId: () => {
      minted += 1;
      return Promise.resolve('019897b4-8f3a-7c21-9d44-0b6a1e2c5f8' + String(minted));
    },
    save: (agent: Agent) => {
      saved.push(agent);
      return Promise.resolve();
    },
    remove: (id: string) => {
      removed.push(id);
      return Promise.resolve();
    },
  };
}

function named(agents: Agent[], name: string): Agent {
  const hit = agents.find((agent) => agent.name === name);
  if (hit === undefined) {
    throw new Error('the store no longer holds the agent this test put in it: ' + name);
  }
  return hit;
}

describe('duplicating an agent makes a second agent, not a second name for the first', () => {
  it('gives the copy its own id and its own name, and leaves the original alone', async () => {
    const io = recorder([forge()]);
    const store = createAgentsStore(io);
    await store.getState().load();

    await store.getState().duplicate(named(store.getState().agents, 'Forge').id);

    const agents = store.getState().agents;
    expect(agents, 'duplicating turns one agent into two').toHaveLength(2);

    const original = named(agents, 'Forge');
    const copy = named(agents, 'Forge (copy)');

    expect(
      copy.id,
      'the copy needs its own id. Two entries sharing one id is one agent listed twice, and ' +
        'every step that points at it will follow whichever one was written last',
    ).not.toBe(original.id);
    expect(
      original.id,
      'and the original keeps the id it had, because steps in saved workflows point at it',
    ).toBe(forge().id);
    expect(original.name, 'and the original keeps its name').toBe('Forge');
  });

  it('copies the lists too, so editing the copy cannot reach back into the original', async () => {
    const io = recorder([forge()]);
    const store = createAgentsStore(io);
    await store.getState().load();

    await store.getState().duplicate(named(store.getState().agents, 'Forge').id);

    const before = named(store.getState().agents, 'Forge').skills.length;
    named(store.getState().agents, 'Forge (copy)').skills.push('pdf');

    expect(
      named(store.getState().agents, 'Forge').skills.length,
      'adding a skill to the copy must not add it to the original. A shallow copy shares the ' +
        'lists, so the user loses an agent they never opened',
    ).toBe(before);
  });

  it('writes the copy out, because a duplicate is a new file', async () => {
    const io = recorder([forge()]);
    const store = createAgentsStore(io);
    await store.getState().load();

    await store.getState().duplicate(named(store.getState().agents, 'Forge').id);

    const copy = named(store.getState().agents, 'Forge (copy)');
    expect(
      io.saved.map((agent) => agent.id),
      'the copy has to be handed over to be written. A duplicate that lives only on screen is ' +
        'gone the next time the app opens',
    ).toContain(copy.id);
  });

  it('deletes exactly one agent and does not touch the rest', async () => {
    const io = recorder([forge(), needle()]);
    const store = createAgentsStore(io);
    await store.getState().load();

    const doomed = named(store.getState().agents, 'Forge');
    await store.getState().delete(doomed.id);

    const left = store.getState().agents;
    expect(
      left.map((agent) => agent.name),
      'deleting one agent leaves every other one exactly where it was',
    ).toEqual(['Needle']);
    expect(
      io.removed,
      'and the file goes with it — an agent that disappears from the screen and stays on disk ' +
        'comes back at the next start',
    ).toEqual([doomed.id]);
  });
});
