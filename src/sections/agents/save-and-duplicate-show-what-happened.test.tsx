/* `Save` i `Duplicate` mają WIDOCZNY skutek — inny niż `Cancel` i inny niż nic.
 *
 * WADA, zgłoszona przez właściciela 2026-08-31. Po udanym zapisie panel po prostu znikał, czyli
 * dawał dokładnie ten sam widok, co `Cancel`: dwa przyciski o przeciwnym znaczeniu, jeden skutek
 * na ekranie. Kliknięcie, po którym nic nie drgnie, czyta się jak kliknięcie, które nie doszło —
 * i drugie kliknięcie jest wtedy winą interfejsu, nie człowieka (DESIGN §7).
 *
 * `Duplicate` był gorszy: wołał magazyn, kopia lądowała NA KOŃCU listy, poza ekranem przy
 * dwunastu agentach, a panel dalej pokazywał oryginał. Z miejsca, w którym stoi ten przycisk,
 * nie było widać ani jednego piksela różnicy.
 *
 * DWA SKUTKI, DWIE RÓŻNE RZECZY, i obie są tu sądzone:
 *   1. kopia staje OBOK oryginału, a nie na końcu listy — położenie jest tu treścią, bo skutek
 *      widoczny poza kadrem nie jest widoczny;
 *   2. kafelek, którego dotyczyła udana czynność, dostaje plakietkę `Saved` w akcencie i WCHODZI
 *      ona sprężyną (`.enter` z DESIGN §7) — czyli oko dostaje odpowiedź, i to odpowiedź
 *      SŁOWNĄ, w miejscu, w którym powstała zmiana.
 *
 * SŁABĄ WERSJĄ jest `expect(store.getState().agents).toHaveLength(2)`. Przechodzi dla magazynu,
 * który zrobił kopię, i dla ekranu, który o tym nie mówi ani słowem — czyli dla dokładnie tej
 * wady, którą właściciel zgłosił (niezmiennik 29). Dlatego oznaczenie jest czytane Z MARKUPU,
 * a kontrola negatywna pyta o ekran PRZED czynnością i po czynności ODMÓWIONEJ.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent, AgentsIo } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import AgentsScreen from './index';

function agent(id: string, name: string): Agent {
  return {
    schema: 1,
    id,
    name,
    summary: 'Writes code',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

const FORGE = agent('019897b4-8f3a-7c21-9d44-0b6a1e2c5f71', 'Forge');
const SCOUT = agent('019897b4-8f3a-7c21-9d44-0b6a1e2c5f72', 'Scout');
const MINTED = '019897b4-8f3a-7c21-9d44-0b6a1e2c5f99';

/** Atrapa dysku, która przyjmuje wszystko. `refuse` odwraca to dla kontroli negatywnej. */
function io(refuse = false): AgentsIo {
  return {
    list: () => Promise.resolve([FORGE, SCOUT]),
    newId: () => Promise.resolve(MINTED),
    save: () => (refuse ? Promise.reject('the folder is read-only') : Promise.resolve('rev-2')),
    remove: () => Promise.resolve(),
  };
}

function screenOf(store: ReturnType<typeof createAgentsStore>): string {
  return renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);
}

/** Kawałek markupu od znacznika TEGO kafelka do znacznika następnego — ta sama technika, co
 * `card()` w `./mounted.test.tsx`. Pytanie „czy ten kafelek jest oznaczony" postawione całemu
 * dokumentowi przechodzi także wtedy, gdy oznaczony jest sąsiedni. */
function tileOf(markup: string, id: string): string {
  const start = markup.indexOf('data-agent="' + id + '"');
  if (start < 0) return '';
  const next = markup.slice(start + 1).search(/data-agent="/);
  return next < 0 ? markup.slice(start) : markup.slice(start, start + 1 + next);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('Save and Duplicate leave something on screen that Cancel does not', () => {
  it('marks nothing before anything has been done', async () => {
    const store = createAgentsStore(io());
    await store.getState().load();

    expect(
      occurrences(screenOf(store), 'data-just-saved'),
      'a mark that is on screen before any click is not an answer to a click. Without this ' +
        'control every assertion below also passes on a screen that marks every tile always',
    ).toBe(0);
  });

  it('marks the tile of the agent a successful Save just wrote', async () => {
    const store = createAgentsStore(io());
    await store.getState().load();

    await store.getState().save({ ...FORGE, summary: 'Writes small changes' });

    const markup = screenOf(store);

    expect(
      tileOf(markup, FORGE.id),
      'a save that leaves no trace on screen gives the exact same view as Cancel: the panel is ' +
        'gone and nothing else moved. Two buttons that mean opposite things cannot answer the ' +
        'same way',
    ).toContain('data-just-saved');
    expect(
      tileOf(markup, FORGE.id),
      'the mark has to SAY what happened and ENTER while it does: a colour on its own says only ' +
        '"something here", and a thing that appears without motion is not read as new ' +
        '(DESIGN §7, the .enter primitive)',
    ).toMatch(/class="chip enter[^"]*"[^>]*data-tone="accent"[^>]*>\s*Saved/);
    expect(
      tileOf(markup, SCOUT.id),
      'exactly one tile is the answer to this click. A mark on every tile says nothing at all',
    ).not.toContain('data-just-saved');
    expect(
      markup,
      'the new summary is on the tile too, so the mark is not decorating stale text',
    ).toContain('Writes small changes');
  });

  it('puts the copy next to the original and marks it, instead of hiding it at the end', async () => {
    const store = createAgentsStore(io());
    await store.getState().load();

    await store.getState().duplicate(FORGE.id);

    const listed = store.getState().agents;
    expect(
      listed.map((one) => one.name),
      'the copy went to the END of the list. With a dozen agents on screen that is below the ' +
        'fold: the person clicks Duplicate and sees nothing happen at all',
    ).toEqual(['Forge', 'Forge (copy)', 'Scout']);

    const markup = screenOf(store);
    expect(
      tileOf(markup, MINTED),
      'and the copy has to say it is the new thing — it looks exactly like its original, so ' +
        'without the mark a person cannot tell which of the two just appeared',
    ).toContain('data-just-saved');
    expect(
      tileOf(markup, FORGE.id),
      'the original is not the new thing, and must not be marked as if it were',
    ).not.toContain('data-just-saved');
  });

  it('marks nothing when the disk turned the save down', async () => {
    const store = createAgentsStore(io(true));
    await store.getState().load();

    await store.getState().save({ ...FORGE, summary: 'Never written' });

    const markup = screenOf(store);
    expect(
      occurrences(markup, 'data-just-saved'),
      'a mark after a refused save is a green light over a file that was never written — the ' +
        'worst possible answer, because it is a confident wrong one (invariant 4)',
    ).toBe(0);
    expect(markup, 'and the refusal itself is on screen').toContain('the folder is read-only');
  });
});
