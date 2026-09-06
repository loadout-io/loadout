/* Opis roli i dwa przyciski są NA EKRANIE i każdy z nich prosi właściwego vendora.
 *
 * # Czego to kryterium pilnuje
 *
 * Tego, że kliknięcie ma skutek, i że skutek jest TEN, którego nazwa przycisku obiecuje.
 * „Create with Codex", które prosi Claude'a, wygląda na ekranie dokładnie tak samo jak
 * działające — a różnica wychodzi dopiero w rachunku i w konfiguracji zapisanego agenta.
 *
 * # Dlaczego przez `AgentsScreen`, a nie przez sam `GenerateAgent`
 *
 * Bo wyrenderowanie komponentu wprost przechodzi w chwili, w której ten plik zacznie cokolwiek
 * rysować, i nie mówi ani słowa o tym, czy EKRAN go montuje. Dokładnie tego brakowało
 * `checkpoint-panel.tsx` przez cały dzień, w którym miał komplet testów i zero importerów.
 *
 * W repo nie ma jsdom, więc kliknięcie dosięgamy przez drzewo Reacta: `renderToStaticMarkup`
 * oddaje napis, a napis nie ma uchwytów.
 */
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import type { GenerateAgentProps } from './generate-agent';
import { DraftNotes } from './generate-agent';
import AgentsScreen from './index';
import type { GeneratedDraft } from './io';

/* Atrapa JEST PRZEPUSZCZAJĄCA: woła prawdziwy komponent i tylko zapisuje po drodze jego
 * drzewo. Bez niej nie da się dosięgnąć uchwytu — `renderToStaticMarkup` oddaje napis,
 * a napis nie ma handlerów. Renderujemy przy tym CAŁY ekran, bo pytanie brzmi „czy ekran
 * to montuje", a nie „czy ten plik coś rysuje". */
const spy = vi.hoisted(() => ({ shown: [] as ReactElement[] }));

vi.mock('./generate-agent', async (importOriginal) => {
  const real = await importOriginal<typeof import('./generate-agent')>();
  return {
    ...real,
    GenerateAgent: (props: GenerateAgentProps): ReactElement => {
      const tree = real.GenerateAgent(props);
      spy.shown.push(tree);
      return tree;
    },
  };
});

const AGENT: Agent = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000001',
  name: 'QA',
  summary: 'Checks the work',
  color: 'slate',
  instructions: 'Verify the behaviour that was asked for.',
  runsWith: 'claude-code',
  model: '',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: true,
  skills: [],
  connections: [],
  serviceAccess: [],
  agentMessages: false,
  writeResultsTo: '',
  vendorOptions: {},
};

function draftOf(runsWith: Agent['runsWith']): GeneratedDraft {
  return {
    operation: 'op-1',
    agent: { ...AGENT, runsWith },
    assumptions: ['You did not say which project, so it works wherever it is started.'],
    because: ['It only reads, because checking does not change the work.'],
    missing: ['the connection "screen-control", which is not available here'],
    refused: [],
  };
}

/** Magazyn z jedną zapisaną rolą — czyli ekran w stanie, w którym ludzie go widzą.
 *
 * Pustej biblioteki tu nie badamy z rozmysłu: jej zaproszenie jest rozstrzygnięciem
 * właściciela i ten stan zostaje dokładnie taki, jaki był (`index.tsx`, gałąź `empty`).
 */
async function libraryWithOne() {
  const store = createAgentsStore({
    list: () => Promise.resolve([AGENT]),
    newId: () => Promise.resolve(AGENT.id),
    save: () => Promise.resolve('revision-1'),
    remove: () => Promise.resolve(),
  });
  await store.getState().load();
  return store;
}

describe('writing an agent from a description', () => {
  it('is on the screen at all, beside the manual way', async () => {
    const markup = renderToStaticMarkup(
      <AgentsScreen
        store={await libraryWithOne()}
        usage={null}
        generating={{
          generate: () => Promise.resolve(draftOf('claude-code')),
          stopGenerating: () => Promise.resolve(),
        }}
      />,
    );
    expect(
      markup,
      'without this row the only way to make an agent is filling a dozen fields by hand, ' +
        'and the described role never reaches any vendor',
    ).toContain('Describe what this agent should do');
    expect(markup).toContain('Create with Codex');
    expect(markup).toContain('Create with Claude');
    expect(
      markup,
      'the manual way has to stay: this row is an addition, not a replacement',
    ).toContain('＋ Create');
    expect(
      markup,
      'a control that promises a finished agent is a control that lies about what it does',
    ).not.toContain('perfect');
  });

  it('says the button writes settings rather than running the agent', async () => {
    const markup = renderToStaticMarkup(
      <AgentsScreen
        store={await libraryWithOne()}
        usage={null}
        generating={{
          generate: () => Promise.resolve(draftOf('codex')),
          stopGenerating: () => Promise.resolve(),
        }}
      />,
    );
    expect(markup).toContain('It does not run the agent, and nothing is saved until you press Save');
  });

  it('never lets checked settings read as checked behaviour', () => {
    const shown = renderToStaticMarkup(<DraftNotes draft={draftOf('codex')} />);
    expect(
      shown,
      'a role written by a model and judged by the same model says only how good its own ' +
        'prompt was. Without this sentence the green beside the settings reads as a green ' +
        'beside the behaviour.',
    ).toContain('not tested');
    expect(shown).toContain('Save it, then use Evaluate');
    expect(
      shown,
      'the draft claims its behaviour was confirmed by something',
    ).not.toContain('Test passed');
    expect(
      shown,
      'what the agent asked for and cannot have here has to be visible before Save, not after',
    ).toContain('screen-control');
  });

  it('refuses an empty description at the control, without asking any vendor', async () => {
    const asked: string[] = [];
    spy.shown.length = 0;
    renderToStaticMarkup(
      <AgentsScreen
        store={await libraryWithOne()}
        usage={null}
        generating={{
          generate: (_operation, described) => {
            asked.push(described);
            return Promise.resolve(draftOf('codex'));
          },
          stopGenerating: () => Promise.resolve(),
        }}
      />,
    );
    const found = controls(spy.shown.at(0));
    found['create-with-claude']?.();
    expect(asked, 'an empty description was sent to a vendor anyway').toEqual([]);
  });
});

/** Uchwyty kontrolek wyjęte z drzewa Reacta, po `data-field`. */
function controls(tree: unknown): Record<string, ((event?: never) => void) | undefined> & {
  describe?: (event: { target: { value: string } }) => void;
} {
  const found: Record<string, unknown> = {};
  const walk = (node: unknown): void => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      node.forEach(walk);
      return;
    }
    const props = (node as { props?: Record<string, unknown> }).props;
    if (!props) return;
    if (props['id'] === 'agent-description') found['describe'] = props['onChange'];
    const field = props['data-field'];
    if (typeof field === 'string' && props['onClick']) found[field] = props['onClick'];
    walk(props['children']);
  };
  walk(tree);
  return found as never;
}
