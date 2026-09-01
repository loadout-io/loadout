/* Na ekranie Triggers jest DOKŁADNIE JEDNA czynność główna — także wtedy, gdy panel jest otwarty.
 *
 * PO CO TO KRYTERIUM. Akcent jest w tym systemie obietnicą: „to jest ta rzecz, po którą tu
 * przyszedłeś". Dwa akcenty naraz znaczą, że nikt nie rozstrzygnął, która nią jest — a przy
 * otwartym panelu rozstrzygnięcie jest oczywiste, bo panel czeka na Save i dopóki go nie
 * dostanie, nic innego nie ma znaczenia. Do 2026-08-31 „Create trigger" świeciło akcentem także
 * wtedy: dwa przyciski tej samej wagi, po dwóch stronach ekranu, w tej samej chwili.
 *
 * DLACZEGO ASERCJA NA LICZBIE, A NIE NA WYGLĄDZIE. Poziom głośności jest w tym repo nazwany
 * klasą prymitywu (`.btn-primary`), więc pytanie „ile rzeczy krzyczy" ma dokładnie jedną
 * mierzalną postać: ile razy ta klasa stoi w markupie CAŁEGO ekranu. Liczba, nie obecność —
 * asercja o obecności przeszłaby także wtedy, gdy akcentów jest pięć (niezmiennik 20).
 *
 * OBIE STRONY, bo samo „nie więcej niż jeden" spełnia ekran, który nie ma akcentu wcale —
 * a taki ekran nie mówi człowiekowi nic o tym, co ma zrobić. Zamknięty panel: dokładnie jeden.
 * Otwarty panel: dalej dokładnie jeden, tyle że drugi.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';
import { useWorkspaces } from '../../state/workspaces';
import type { TriggerIo } from './io';
import TriggersScreen from './index';
import type { TriggerEditorController } from './index';

const CLOCK: TriggerClock = {
  now: () => 0,
  setInterval: () => 1,
  clearInterval: () => undefined,
};

const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 1,
};

const IO: TriggerIo = {
  listTriggers: async () => [],
  setTriggerEnabled: async () => {
    throw new Error('not used');
  },
  checkTrigger: async () => ({ status: 'armed' }),
  resumeTrigger: async () => ({ status: 'armed' }),
  retryTrigger: async () => {
    throw new Error('not used');
  },
  createTrigger: async () => {
    throw new Error('not used');
  },
  updateTrigger: async () => {
    throw new Error('not used');
  },
  deleteTrigger: async () => undefined,
  testLinearConnection: async () => undefined,
};

const SAVED: TriggerView = {
  slug: 'assigned-to-me',
  source: 'Linear',
  condition: 'Assigned to you',
  workflow: 'analysis.json',
  workflowName: 'Analysis',
  workspace: '/project',
  pollEveryMinutes: 1,
  hasApiKey: true,
  enabled: true,
  status: { kind: 'armed' },
};

/** Klasa, którą ten system nazywa czynność główną. Jedna na ekran. */
const LOUD = 'btn-primary';

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function screen(editor?: TriggerEditorController): string {
  const store = createTriggersStore(IO, CLOCK, RUN);
  store.setState({ triggers: [SAVED] });
  return renderToStaticMarkup(
    editor === undefined ? (
      <TriggersScreen store={store} />
    ) : (
      <TriggersScreen store={store} editor={editor} />
    ),
  );
}

/** Panel otwarty na zapisanym triggerze — dokładnie ten stan, który dokłada drugi akcent. */
const OPENED: TriggerEditorController = {
  state: {
    opened: {
      mode: 'edit',
      value: {
        connector: 'linear',
        apiKey: '',
        workflow: 'analysis.json',
        workspace: '/project',
        pollEveryMinutes: 1,
      },
      expected: {
        slug: SAVED.slug,
        source: 'Linear',
        condition: 'Assigned to you',
        workflow: 'analysis.json',
        workspace: '/project',
        enabled: true,
        pollEveryMinutes: 1,
        hasApiKey: true,
      },
    },
    confirmingDelete: false,
    busy: 'idle',
    refusal: null,
    revision: 0,
  },
  change: () => undefined,
};

beforeEach(() => {
  useWorkspaces.setState({
    all: [{ id: '/project', name: 'Project', folder: '/project' }],
    activeId: '/project',
    said: null,
  });
});

describe('the Triggers screen says which single thing is the one to do', () => {
  it('gives the library exactly one main action while nothing else is open', () => {
    expect(
      occurrences(screen(), LOUD),
      'a screen showing saved triggers has to name one thing as the one to do, and Create ' +
        'trigger is it. Nothing accented at all leaves a person reading a list with no idea ' +
        'what this screen is for',
    ).toBe(1);
  });

  it('hands the one main action over to the open panel instead of keeping two', () => {
    const markup = screen(OPENED);
    expect(
      markup,
      'the panel for a saved trigger was never rendered, so this criterion is measuring the ' +
        'wrong screen',
    ).toContain('data-trigger-editor');
    expect(
      occurrences(markup, LOUD),
      'with the panel open two controls carry the accent at once: Create trigger in the header ' +
        'and Save in the panel. A row of equally loud controls means nobody decided which one ' +
        'matters, and here the answer is not in doubt — the panel is waiting to be saved, so ' +
        'Create trigger has to step down to an ordinary control until it closes',
    ).toBe(1);
  });
});
