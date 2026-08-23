/* Każdy krok agenta mówi, co się dzieje z robotą, kiedy nie przejdzie.
 *
 * # Zamówienie
 *
 * Właściciel, 2026-08-23, dosłownie: „workflows zawsze ma mieć opcje kontynuacji a nie ślepe
 * punkty".
 *
 * # Co było zepsute
 *
 * Krok, który nie przeszedł, kasował CAŁY stożek potomków — bezwarunkowo i bez wyboru. Jego bieg
 * `20260823-092142` stracił przez to `Syntezę`, `Design` i `Implementation`, mimo że dwie z trzech
 * weryfikacji przeszły. Nie było kontrolki, którą dałoby się powiedzieć, że ma być inaczej.
 *
 * # Czego to kryterium pilnuje, a czego nie
 *
 * Nie pilnuje brzmień pozycji — te wolno poprawiać. Pilnuje trzech rzeczy, których poprawić nie
 * wolno bez zmiany po drugiej stronie granicy:
 *
 * 1. **Wartości** są dokładnie te, które przyjmuje `workflow::WhenItFails` w Ruście. Lista, która
 *    wysyła `"carryOn"` do enuma znającego `"carry-on"`, wygląda na ekranie poprawnie i wywraca
 *    zapis pliku — a to jest ta rodzina wad, przed którą stoi `quick-invoke-args` przy komendach.
 * 2. **Brak pola czyta się jako `stop`.** Każdy istniejący plik workflow nie ma tego klucza;
 *    kontrolka pokazująca przy nim cokolwiek innego kłamie o tym, co ten krok zrobi.
 * 3. **Kontrolka stoi przy KAŻDYM kroku agenta**, nie tylko przy sędzim pętli. Krok, który padł
 *    zwyczajnie, jest tym samym ślepym punktem — a kontrolka tylko przy jednym z nich kazałaby
 *    zgadywać, czemu drugi jej nie ma.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { AgentStep, WhenItFails } from '../../../state/workflows';
import { PanelForStep } from './panel';

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
    tools: 'everything',
    reachesTheWeb: true,
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

/** Krok BEZ tego klucza — czyli dokładnie taki, jak każdy zapisany przed tą zmianą. */
function step(whenItFails?: WhenItFails): AgentStep {
  return {
    kind: 'agent',
    id: 's_test',
    name: 'Tester',
    agent: jarvis().id,
    overrides: {},
    copies: 1,
    instructions: 'Run the suite and say whether it passed.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    ...(whenItFails === undefined ? {} : { whenItFails }),
    at: { x: 24, y: 24 },
  };
}

function noop(): void {
  /* panel sterowany: statyczny render nic z tego nie woła */
}

function markup(whenItFails?: WhenItFails, wayBack: number | null = null): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step(whenItFails)}
      agents={[jarvis()]}
      skills={[]}
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

/** Sam znacznik listy wyboru, albo pusty napis, kiedy listy nie ma. */
function chooser(html: string): string {
  return /<select[^>]*id="step-when-it-fails"[\s\S]*?<\/select>/.exec(html)?.[0] ?? '';
}

/** Wartości pozycji tej listy, w kolejności. */
function offered(html: string): string[] {
  return [...chooser(html).matchAll(/<option value="([^"]*)"/g)].map((hit) => hit[1] ?? '');
}

describe('every agent step says what happens to the work if it does not pass', () => {
  it('offers exactly the three answers the file format accepts', () => {
    expect(
      offered(markup()),
      'these three values travel straight into the workflow file and are read there by a closed ' +
        'set in Rust. A list that offers a fourth, or spells one of them differently, looks ' +
        'right on screen and breaks the file the moment it is saved',
    ).toEqual(['stop', 'carry-on', 'ask-me']);
  });

  it('reads a step with no such key as stopping, because that is what it does', () => {
    expect(
      /value="stop"/.test(chooser(markup())),
      'a step saved before this setting existed has no such key, and every one of them stops ' +
        'the work after it. Showing anything else there tells the person their run will carry ' +
        'on when it will not. It drew: ' +
        JSON.stringify(chooser(markup())),
    ).toBe(true);
  });

  it('shows what was chosen when the step carries one', () => {
    expect(
      /value="ask-me"/.test(chooser(markup('ask-me'))),
      'the control does not show the choice the file already holds, so opening a step and ' +
        'closing it would quietly reset what it does',
    ).toBe(true);
  });

  it('stands on a step with no way back at all', () => {
    expect(
      chooser(markup(undefined, null)),
      'the control is missing on a step that is not a loop judge. A step that simply failed is ' +
        'the same dead end as a judge that ran out of tries, and a control shown for only one ' +
        'of them leaves the person guessing why the other has none',
    ).not.toBe('');
  });
});
