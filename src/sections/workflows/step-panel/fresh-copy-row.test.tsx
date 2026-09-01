/* Trzy miejsca pracy agenta — jeden wybór, który przeżywa statyczny render panelu.
 *
 * PO CO ISTNIEJE, zmierzone 2026-08-19 na workflow właściciela „Reaserch + implement": dwa kroki
 * researchu wchodzące do jednego kroku syntezy, czyli najzwyklejszy wachlarz. Walidator mówi
 * o nich „C1 and C2 can run at the same time and both work in the project folder. Give one of
 * them a fresh copy." — a aplikacja nie miała ANI JEDNEGO miejsca, w którym dałoby się to
 * zrobić. Pole `folder` leży w schemacie od T-12 i w makiecie (linia 620) od początku; nie miało
 * kontrolki nigdzie w `src/`. Zdanie, które każe zrobić rzecz niewykonalną w oknie, jest gorsze
 * niż brak zdania: człowiek szuka kontrolki, której nie ma, i kończy w edytorze tekstu.
 *
 * CZEGO TU NIE MA I DLACZEGO. Kliknięcia. W repo nie ma jsdom [T3 §2.3, ryzyko 7], więc komponent
 * sprawdzamy statycznym renderem. Każda z trzech wartości musi jednak zaznaczyć dokładnie swoje
 * radio; kontrolka zawsze wyłączona albo binarny przełącznik nie przejdą tego kryterium.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../state/agents';
import type { AgentStep, Folder } from '../../../state/workflows';
import { PanelForStep } from './panel';

function riczi(): Agent {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    name: 'Riczi',
    summary: 'Researches',
    color: 'clay',
    instructions: 'Find out how other people solved this.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'look-only',
    giveUpAfterMinutes: 20,
    writeResultsTo: '',
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
  };
}

function step(folder: Folder): AgentStep {
  return {
    kind: 'agent',
    id: 's_2',
    name: 'C1',
    agent: riczi().id,
    overrides: {},
    copies: 1,
    instructions: 'do a reaserch about ux/ui',
    skills: 'all',
    folder,
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

function noop(): void {
  /* panel sterowany: statyczny render nic z tego nie woła */
}

function markup(folder: Folder): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step(folder)}
      agents={[riczi()]}
      skills={[]}
      onChooseAgent={noop}
      onCreateAgent={noop}
      onEdit={noop}
      onEditStep={noop}
      onEditCheckpoint={noop}
      onEditServe={noop}
      onReset={noop}
      onChooseSkills={noop}
      /* Ten krok nie ma powrotu, więc wiersza liczby rund w nim nie ma — a to jest dokładnie
         stan, w którym mierzymy przełącznik świeżej kopii. */
      wayBack={null}
      onEditWayBack={noop}
    />,
  );
}

/** Wszystkie radia z markupu, bez zakładania kolejności atrybutów renderera. */
function radios(html: string): string[] {
  return html.match(/<input\b[^>]*type="radio"[^>]*>/g) ?? [];
}

function checked(html: string, use: string): boolean {
  return radios(html).some(
    (input) => input.includes(`value="${use}"`) && /\bchecked(?:="")?/.test(input),
  );
}

describe('where an agent works', () => {
  it('offers all three choices in plain language', () => {
    const html = markup({ use: 'project' });

    expect(html).toContain('Work in the project folder');
    expect(html).toContain('Start in a new copy of the files');
    expect(html).toContain('Continue in the same files as the previous step');
    expect(html.toLowerCase()).not.toContain('worktree');
    expect(html.toLowerCase()).not.toContain('branch');
    expect(html.toLowerCase()).not.toContain('automatically merge');
  });

  it('shows exactly one selected choice for every supported file value', () => {
    for (const use of ['project', 'fresh-copy', 'same-copy'] as const) {
      const html = markup({ use });
      expect(radios(html)).toHaveLength(3);
      expect(radios(html).filter((input) => /\bchecked(?:="")?/.test(input))).toHaveLength(1);
      expect(checked(html, use)).toBe(true);
    }
  });

  it('does not pretend a hand-written path is one of the three choices', () => {
    const picked: Folder = { use: 'pick', path: '/Users/x/api' };
    const html = markup(picked);

    expect(radios(html)).toHaveLength(3);
    expect(radios(html).filter((input) => /\bchecked(?:="")?/.test(input))).toHaveLength(0);
  });
});
