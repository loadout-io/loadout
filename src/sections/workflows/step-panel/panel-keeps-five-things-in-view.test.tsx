/* Panel kroku pokazuje PIĘĆ rzeczy, a resztę trzyma za jedną linią.
 *
 * # Co było zmierzone
 *
 * `panel.tsx` montował 21 kontrolek stałych, a realistycznie 34 (sześć umiejętności, pięć
 * pozycji do pożyczenia) w kolumnie 330 px — bez grup, bez zwijania, bez nadoczek. To jest
 * ~60 elementów niosących tekst, czyli CAŁY sufit widoku z ARCHITECTURE §7, zjedzony przez
 * jedną kolumnę. Nagłówek pliku obiecywał „siedem etykiet, w tej kolejności, i ani jednej
 * ósmej" i był o sześć bloków nieaktualny, bo bloki przyrastały po jednym, a każdy z osobna
 * był do obrony.
 *
 * 2026-09-09 — właściciel wyniósł Context i Plan na wierzch, a Where it works i How many at
 * once schował. PIĄTKA ZOSTAŁA PIĄTKĄ: to budżet gęstości, nie przypadkowo zamrożona lista.
 * Zmiana tego, które pięć wierszy go zużywa, nie rozluźnia zmierzonego sufitu ani nie zaprasza
 * szóstej rzeczy na ten poziom.
 *
 * # Czego to kryterium pilnuje
 *
 * 1. **Pięciu rzeczy w widoku**, w tej kolejności. To jest odpowiedź na pytanie „co ten krok
 *    robi" i jakie ma wejścia: kto, nazwa, co, Context, Plan. Szósta rzecz na tym poziomie jest
 *    tą samą ścianą, tylko o jeden dzień późniejszą.
 * 2. **Reszta ISTNIEJE.** Kryterium liczące same etykiety przeszłoby dla implementacji, która
 *    pozostałe wiersze po prostu skasowała — a każdy z nich niesie wartość, którą krok naprawdę
 *    ma. Dlatego druga połowa pyta o to, czy stoją w ujawnieniu.
 * 3. **Ujawnienie jest ZAMKNIĘTE.** Otwarte od początku nie chowa niczego; przechodziłoby
 *    obie asercje wyżej i nie zmieniało ani jednego piksela pierwszego wrażenia.
 * 4. **Zwinięte NAZYWA LICZBĘ**, i to policzoną z tego, co w środku naprawdę stoi. Napis
 *    wpisany na stałe („8 more settings") przechodzi jednostanowe sprawdzenie i rozjeżdża się
 *    z panelem przy pierwszym wierszu, który dojdzie albo zniknie (niezmiennik 20). Dlatego
 *    liczba jest porównywana z licznikiem wierszy, a druga liczba — z liczbą nadpisań — przy
 *    dwóch różnych krokach.
 *
 * Kliknięcia tu nie ma: w repo nie ma jsdom, więc mierzymy markup, który wychodzi z ekranu.
 * Rozwijanie jest zachowaniem przeglądarki i nie ma własnego stanu, który dałoby się tu
 * przestawić — i to jest powód, dla którego treść jest w drzewie od pierwszego renderu.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { WorkflowContextView } from '../../../state/context';
import type { AgentStep, Overrides, WorkflowPlanView } from '../../../state/workflows';
import { PanelForStep } from './panel';

/** Pięć rzeczy, które zostają w widoku — kontrakt tego kryterium, wpisany ręcznie. */
const IN_VIEW = ['Who does this', 'Name', 'What to do', 'Context', 'Plan'];

/** I te same pięć, po znacznikach wierszy, żeby kolejność nie zależała od brzmienia. */
const IN_VIEW_ROWS = ['who-does-this', 'name', 'what-to-do', 'context', 'plan'];

/** Dziesięć rzeczy, które chowają się za ujawnieniem — wszystkie z działającą wartością
 * dziedziczoną z agenta. */
const FOLDED_AWAY = [
  'Where it works',
  'How many at once',
  'Can it change files',
  'Give up after',
  'Write results to',
  'Try again up to',
  'If this step does not pass',
  'What it hands over',
  'Skills',
  'Borrow from this project',
];

function jarvis(): Agent {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    name: 'Jarvis',
    summary: 'Implements',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    writeResultsTo: 'handoffs/build.md',
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
  };
}

function step(
  overrides: Overrides,
  fields: Partial<Pick<AgentStep, 'context' | 'plan'>> = {},
): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: jarvis().id,
    overrides,
    copies: 1,
    instructions: 'Fix the failing parser tests.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    /* Krok pożycza coś, więc wiersz pożyczania stoi także wtedy, gdy Rust jeszcze nie
       odpowiedział, czego ten folder może użyczyć. */
    borrow: { skills: ['code-review'] },
    at: { x: 24, y: 24 },
    ...fields,
  };
}

function noop(): void {
  /* panel sterowany: statyczny render nic z tego nie woła */
}

function markup(
  overrides: Overrides = {},
  wayBack: number | null = 3,
  fields: Partial<Pick<AgentStep, 'context' | 'plan'>> = {},
  context: WorkflowContextView | null = null,
  plan: WorkflowPlanView | null = null,
): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step(overrides, fields)}
      agents={[jarvis()]}
      skills={['code-review', 'deep-research']}
      context={context}
      plan={plan}
      onChooseAgent={noop}
      onCreateAgent={noop}
      onEdit={noop}
      onEditStep={noop}
      onEditCheckpoint={noop}
      onEditServe={noop}
      onReset={noop}
      onChooseSkills={noop}
      wayBack={wayBack}
      onEditWayBack={noop}
    />,
  );
}

const OPENS = '<details';
const CLOSES = '</details>';

/** Markup rozcięty na to, co widać od razu, i to, co leży za ujawnieniami.
 *
 * Zagnieżdżenia liczone, bo listy bez sufitu mają własne ujawnienie w środku tego jednego. */
function cut(html: string): { inView: string; folded: string } {
  let inView = '';
  let folded = '';
  let at = 0;
  while (at < html.length) {
    const start = html.indexOf(OPENS, at);
    if (start === -1) {
      inView += html.slice(at);
      break;
    }
    inView += html.slice(at, start);
    let depth = 0;
    let cursor = start;
    for (;;) {
      const nextOpen = html.indexOf(OPENS, cursor + 1);
      const nextClose = html.indexOf(CLOSES, cursor + 1);
      if (nextClose === -1) {
        cursor = html.length;
        break;
      }
      if (nextOpen !== -1 && nextOpen < nextClose) {
        depth += 1;
        cursor = nextOpen;
        continue;
      }
      if (depth === 0) {
        cursor = nextClose + CLOSES.length;
        break;
      }
      depth -= 1;
      cursor = nextClose;
    }
    folded += html.slice(start, cursor);
    at = cursor;
  }
  return { inView, folded };
}

/** Nagłówki wierszy — elementy, których klasa jest DOKŁADNIE `label`. */
function headings(html: string): string[] {
  const found: string[] = [];
  const rx = /<(label|legend|span)\b[^>]*class="label"[^>]*>([\s\S]*?)<\/\1>/g;
  for (const hit of html.matchAll(rx)) found.push((hit[2] ?? '').replace(/<[^>]*>/g, '').trim());
  return found;
}

/** Znaczniki wierszy, w kolejności renderu. */
function rows(html: string): string[] {
  return [...html.matchAll(/data-row="([^"]*)"/g)].map((hit) => hit[1] ?? '');
}

/** Napis, który niesie zwinięte ujawnienie panelu — czyli to jedno zdanie, które zostaje
 * z dziesięciu wierszy. */
function saysWhenShut(html: string): string {
  const hit =
    /<details\b(?=[^>]*\bdata-more-settings(?:=""|\b))[^>]*>\s*<summary\b[^>]*>([\s\S]*?)<\/summary>/.exec(
      html,
    );
  return (hit?.[1] ?? '').replace(/<[^>]*>/g, '').trim();
}

function contextWithTwoSets(): WorkflowContextView {
  return {
    catalog: [],
    workflow: [],
    steps: [
      {
        stepId: 's_build',
        sets: [
          {
            id: 'rules',
            title: 'Project rules',
            revision: 'r1',
            selectedTopics: 'all',
            topics: [],
            source: 'workflow',
            update: null,
            said: null,
          },
          {
            id: 'brief',
            title: 'Feature brief',
            revision: 'r2',
            selectedTopics: 'all',
            topics: [],
            source: 'step',
            update: null,
            said: null,
          },
        ],
        omitted: [],
        inheritsWorkflow: true,
        protectedScope: false,
        said: null,
      },
    ],
    warnings: [],
  };
}

function planView(mode: 'create' | 'use', inherited = false): WorkflowPlanView {
  return {
    steps: [
      {
        stepId: 's_build',
        mode,
        source: inherited
          ? { stepId: 's_plan', name: 'Planner', said: 'Inherited from Planner.' }
          : null,
        earlier: [],
        said: null,
      },
    ],
    warnings: [],
  };
}

describe('the panel of an agent step answers what the step does, and folds the rest away', () => {
  it('opens with who, name, what to do, Context and Plan — and nothing else', () => {
    const { inView } = cut(markup());

    expect(
      headings(inView),
      'the panel opens with more than the five things that say what this step does. Every row ' +
        'here is defensible on its own, and together they are a wall of controls in a 330 px ' +
        'column — one column eating the whole ceiling for the view',
    ).toEqual(IN_VIEW);
    expect(
      rows(inView),
      'the five in view are not the five that answer the question, or they stand in an order ' +
        'that does not: who does it, its name, what it does, Context, Plan',
    ).toEqual(IN_VIEW_ROWS);
  });

  it('still carries every one of the ten it folded away', () => {
    const { inView, folded } = cut(markup());

    for (const thing of FOLDED_AWAY) {
      expect(
        folded,
        'folding this away turned into deleting it: "' +
          thing +
          '" is nowhere behind the fold, and the step really does carry that value',
      ).toContain(thing);
      expect(
        inView,
        '"' + thing + '" is still standing in the open, so nothing was folded away at all',
      ).not.toContain(thing);
    }
  });

  it('starts shut, or it folded nothing away', () => {
    const opening =
      /<details\b(?=[^>]*\bdata-more-settings(?:=""|\b))[^>]*>/.exec(markup())?.[0] ?? '';

    expect(opening, 'the panel has no fold at all').not.toBe('');
    expect(
      / open(?:=|\s|>)/.test(opening),
      'the fold stands open from the first render, so the person opening a step meets the same ' +
        'wall as before and the line naming what is inside buys nothing',
    ).toBe(false);
  });

  it('says how many things are inside, counted off the panel and not written down', () => {
    for (const wayBack of [3, null]) {
      const html = markup({}, wayBack);
      const { folded } = cut(html);
      const inside = rows(folded).length;

      expect(
        inside,
        'nothing at all is behind the fold, so the sentence below would be counting air',
      ).toBeGreaterThan(0);
      expect(
        saysWhenShut(html),
        'the shut fold does not name how many things it holds. A person who cannot see the count ' +
          'has no way to tell an empty fold from one holding settings they inherited',
      ).toBe(String(inside) + ' more settings');
    }
  });

  it('shows the Context state while its picker stays folded away', () => {
    const none = cut(markup());
    const selected = cut(markup({}, 3, {}, contextWithTwoSets()));

    expect(none.inView).toContain('No context');
    expect(selected.inView).toContain('2 selected');
    expect(selected.inView).not.toContain('Choose context');
    expect(selected.folded).toContain('Choose context');
  });

  it('shows both explicit and inherited Plan modes where the person can see them', () => {
    const create = cut(markup({}, 3, { plan: { mode: 'create' } }, null, planView('create')));
    const inherited = cut(markup({}, 3, {}, null, planView('use', true)));

    expect(create.inView).toContain('Plan: Create');
    expect(inherited.inView).toContain('Plan: Use');
    expect(inherited.inView).toContain('Inherited from Planner.');
  });

  it('also says how many of them differ from the agent, and says nothing when none do', () => {
    const two = markup({ thinking: 'deep', giveUpAfterMinutes: 45 });
    const { folded } = cut(two);

    expect(
      saysWhenShut(two),
      'the count of what this step changed does not reach the shut fold, so a step that ' +
        'quietly differs from its agent in two places reads exactly like one that differs in ' +
        'none. Two states are compared, because a written-down "2 changed" passes one of them',
    ).toBe(String(rows(folded).length) + ' more settings, 2 changed');
    expect(
      saysWhenShut(markup({ thinking: 'deep' })),
      'and the number follows the step, rather than standing where it was put',
    ).toContain('1 changed');
    expect(
      saysWhenShut(markup()),
      'a step that changed nothing is told it changed nothing, which is a line of noise on ' +
        'every untouched step in the workflow',
    ).not.toContain('changed');
  });
});
