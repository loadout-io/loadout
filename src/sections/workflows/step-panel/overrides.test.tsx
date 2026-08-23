/* Kryterium 4 dla T-13: edycja kroku zmienia KROK i nie zmienia AGENTA.
 *
 * To jest ta porażka, przed którą użytkownik ostrzegł wprost: poprawiasz jeden krok, a zmienia
 * się agent, więc pięć innych workflow po cichu zaczyna działać inaczej. Wszystko wygląda dobrze
 * i wszystkie testy przechodzą, bo testy pytają „czy krok ma teraz thinking: deep?", a nigdy
 * „czy agent jest dokładnie taki, jak był?".
 *
 * Słaba wersja tego kryterium to `expect(step.overrides.thinking).toBe('deep')`. Przechodzi dla
 * implementacji, która przy okazji zapisuje też plik agenta — czyli dokładnie dla tej złej.
 * Rozróżniają to trzy rzeczy naraz, wszystkie w pierwszym `it`: głęboka równość magazynu agentów
 * z kopią sprzed edycji, zero wywołań `io.saveAgent` i zero wywołań zapisu w magazynie agentów.
 * Dwa ostatnie liczą się osobno, bo są dwiema różnymi drogami do tego samego pliku.
 *
 * Ostatnie dwa `it` pytają o to, co widzi użytkownik. Licznik „N changed" jest sprawdzany
 * PRZECIWKO LICZBIE KLUCZY W DOKUMENCIE i przy dwóch różnych stanach — `expect(html).toContain(
 * '1 changed')` przechodzi na napisie wpisanym na stałe (niezmiennik 20), a dwa różne stany
 * rozstrzygają to jedną linią.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent, AgentsIo } from '../../../state/agents';
import { createAgentsStore } from '../../../state/agents';
import type {
  AgentStep,
  Note,
  Overrides,
  WorkflowFile,
  WorkflowIo,
} from '../../../state/workflows';
import { createWorkflowStore } from '../../../state/workflows';
import { applyPanelEdit } from './overrides';
import { StepPanel } from './panel';

/** Siedem wierszy z makiety, `docs/mockup/index.html:599-618`, w tej kolejności. */
const SEVEN = [
  'Name',
  'Who does this',
  'What to do',
  'How many at once',
  'Can it change files',
  'Give up after',
  'Write results to',
];

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
    reachesTheWeb: false,
    skills: [],
    connections: [],
    writeResultsTo: 'handoffs/build.md',
  };
}

function build(overrides: Overrides): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: forge().id,
    overrides,
    copies: 1,
    instructions: 'Fix the failing parser tests. Keep the public API unchanged.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 168 },
  };
}

function file(overrides: Overrides): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [build(overrides)],
    links: [],
  };
}

interface WorkflowRecorder extends WorkflowIo {
  savedFiles: WorkflowFile[];
  savedAgents: Agent[];
}

function workflowIo(): WorkflowRecorder {
  const savedFiles: WorkflowFile[] = [];
  const savedAgents: Agent[] = [];
  return {
    savedFiles,
    savedAgents,
    save: (target: WorkflowFile) => {
      savedFiles.push(target);
      return Promise.resolve();
    },
    check: () => Promise.resolve([] as Note[]),
    saveAgent: (agent: Agent) => {
      savedAgents.push(agent);
      return Promise.resolve();
    },
  };
}

interface AgentsRecorder extends AgentsIo {
  written: Agent[];
}

function agentsIo(seed: Agent[]): AgentsRecorder {
  const written: Agent[] = [];
  return {
    written,
    list: () => Promise.resolve(seed.map((one) => structuredClone(one))),
    newId: () => Promise.resolve('019897b4-8f3a-7c21-9d44-0b6a1e2c5f78'),
    save: (agent: Agent) => {
      written.push(agent);
      return Promise.resolve();
    },
    remove: () => Promise.resolve(),
  };
}

function agentStep(doc: WorkflowFile, id: string): AgentStep {
  const hit = doc.steps.find((one) => one.id === id);
  if (hit === undefined || hit.kind !== 'agent') {
    throw new Error('the document no longer holds an agent step called ' + id);
  }
  return hit;
}

function firstAgent(agents: Agent[]): Agent {
  const hit = agents[0];
  if (hit === undefined) throw new Error('the agents store came back empty');
  return hit;
}

function noop(): void {
  /* sterowany panel: w statycznym renderze nic tego nie woła */
}

function markup(step: AgentStep): string {
  return renderToStaticMarkup(
    <StepPanel
      step={step}
      agent={forge()}
      /* Biblioteka jedzie tu od 2026-08-18, bo wiersz „Who does this" jest listą WYBORU także
       * po wyborze: do tego dnia był nieklikalnym `<span>` z nazwą agenta, więc pomyłka przy
       * przypisaniu była nieodwracalna z okna. Jeden wpis, ten sam agent — kryterium niżej
       * pyta o etykiety wierszy, nie o zawartość listy. */
      agents={[forge()]}
      onChooseAgent={noop}
      onCreateAgent={noop}
      onEdit={noop}
      onEditStep={noop}
      onReset={noop}
    />,
  );
}

function plain(fragment: string): string {
  return fragment
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

function labelsOf(html: string): string[] {
  return [...html.matchAll(/<label\b[^>]*>([\s\S]*?)<\/label>/g)].map((hit) => plain(hit[1] ?? ''));
}

/** Liczba ze znacznika „N changed", albo `null`, kiedy znacznika nie ma wcale. */
function changedChip(html: string): number | null {
  const hit = /(\d+) changed/.exec(plain(html));
  return hit === null ? null : Number(hit[1]);
}

describe('editing a step edits the step, and the agent it inherits from stays where it was', () => {
  it('writes the difference onto the step and touches the agent by neither of the two roads', async () => {
    const library = agentsIo([forge()]);
    const agents = createAgentsStore(library);
    await agents.getState().load();

    const io = workflowIo();
    const store = createWorkflowStore(io, file({}));

    const untouched = structuredClone(agents.getState().agents);

    store.getState().editStep('s_build', firstAgent(agents.getState().agents), {
      thinking: 'deep',
    });

    const step = agentStep(store.getState().document, 's_build');
    expect(
      Object.keys(step.overrides).sort(),
      'the step keeps the DIFFERENCE, not a copy of the agent. A full copy means a later edit ' +
        'of the agent never reaches this workflow, and nobody finds out until a run behaves oddly',
    ).toEqual(['thinking']);
    expect(step.overrides.thinking, 'and the difference is the value that was typed').toBe('deep');

    expect(
      agents.getState().agents,
      'this is the whole criterion: the agent is byte for byte what it was before the edit. ' +
        'Reaching into the object it was handed is the same failure one call earlier',
    ).toEqual(untouched);
    expect(
      io.savedAgents,
      'and no agent file was handed over to be written. The panel can write one — that is how ' +
        '"Create a new agent…" works — which is exactly why this line has to be here',
    ).toEqual([]);
    expect(
      library.written,
      'and not by the other road either. The agents store has its own way to disk, so proving ' +
        'one of the two is quiet proves nothing about the other',
    ).toEqual([]);
  });

  it('hands back a new step and mutates neither of the two objects it was given', () => {
    const agent = forge();
    const agentBefore = structuredClone(agent);
    const step = build({});
    const stepBefore = structuredClone(step);

    const next = applyPanelEdit(step, agent, { thinking: 'deep' });

    expect(next.overrides, 'the new step carries the difference and only the difference').toEqual({
      thinking: 'deep',
    });
    expect(
      agent,
      'the agent is an argument, not a destination. Writing into it is how one edited step ' +
        'quietly changes every other workflow that names this agent',
    ).toEqual(agentBefore);
    expect(step, 'and the step it was given is not edited in place either').toEqual(stepBefore);
  });

  it('takes one row out on Reset and leaves the other changes alone', async () => {
    const agents = createAgentsStore(agentsIo([forge()]));
    await agents.getState().load();
    const store = createWorkflowStore(workflowIo(), file({}));

    store.getState().editStep('s_build', firstAgent(agents.getState().agents), {
      thinking: 'deep',
      giveUpAfterMinutes: 45,
    });
    store.getState().resetRow('s_build', 'thinking');

    expect(
      agentStep(store.getState().document, 's_build').overrides,
      'Reset is one row. Emptying the whole patch is a different control with a different ' +
        'sentence next to it — "Use agent\'s settings"',
    ).toEqual({ giveUpAfterMinutes: 45 });

    store.getState().resetRow('s_build', 'giveUpAfterMinutes');
    expect(
      agentStep(store.getState().document, 's_build').overrides,
      'and resetting the last one leaves a step that has gone back to inheriting everything',
    ).toEqual({});
  });

  it('counts the changes off the document and says what the agent would have used', () => {
    const clean = build({});
    const one = build({ thinking: 'deep' });
    const two = build({ thinking: 'deep', giveUpAfterMinutes: 45 });

    expect(
      changedChip(markup(one)),
      'the number comes from the patch in the document. Two states, because a hard-coded ' +
        '"1 changed" passes a single-state check without ever counting anything',
    ).toBe(Object.keys(one.overrides).length);
    expect(changedChip(markup(two)), 'and it follows the patch when a second row changes').toBe(
      Object.keys(two.overrides).length,
    );
    expect(
      changedChip(markup(clean)),
      'a step that changed nothing carries no mark at all — not "0 changed"',
    ).toBeNull();

    expect(
      plain(markup(one)),
      'under a changed row the panel says what it diverged from, so nobody has to open the ' +
        'Agents section to find out',
    ).toContain('Agent uses: Balanced');
    expect(
      plain(markup(clean)),
      'and an untouched step says nothing of the sort, because there is nothing to compare to',
    ).not.toContain('Agent uses');
  });

  it('is exactly the seven rows of the mockup, and the third toggle is not one of them', () => {
    const html = markup(build({ thinking: 'deep' }));

    expect(
      labelsOf(html),
      'these seven, in this order, and no eighth. An eighth row here is the first step towards ' +
        'the settings page nobody fills in, and it is always defensible on its own',
    ).toEqual(SEVEN);
    expect(
      html,
      'no schema field carries how deep an agent may delegate, and both research reports rule ' +
        'it out of v1. Copying the mockup one to one puts a third toggle here that looks ' +
        'exactly like the two that work',
    ).not.toContain('Let it split into helpers');
  });
});
