/* Przełącznik „Fresh copy of the files" — jedyne miejsce, w którym da się spełnić odmowę Startu.
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
 * sprawdzamy statycznym renderem, a jedyną DECYZJĘ tego wiersza — osobną funkcją czystą. Podział
 * jest treścią, nie wygodą: bez niej rozstrzygnięcie o trzeciej wartości schematu (`pick`)
 * nie miałoby kryterium i skasowałby je pierwszy refaktor.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie samego `nextFolder` na dwóch wartościach. Przechodzi ją
 * implementacja, która renderuje przełącznik ZAWSZE wyłączony — czyli kontrolka kłamiąca
 * o stanie kroku, który sama przed chwilą zapisała. Dlatego oba stany są renderowane i czytane
 * z markupu.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../state/agents';
import type { AgentStep, Folder } from '../../../state/workflows';
import { PanelForStep, nextFolder } from './panel';

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
      onReset={noop}
      onChooseSkills={noop}
    />,
  );
}

/** Czy pole wyboru w markupie jest zaznaczone. */
function checked(html: string): boolean {
  return /<input[^>]*type="checkbox"[^>]*checked[^>]*>/.test(html);
}

describe('fresh copy of the files', () => {
  it('is a control the panel actually has, worded as the mockup words it', () => {
    const html = markup({ use: 'project' });

    expect(
      html,
      'the validator tells the person to give a step a fresh copy; a window with no such ' +
        'control sends them to a text editor to satisfy its own refusal',
    ).toContain('Fresh copy of the files');
    expect(
      html,
      'the second half of the mockup sentence is the half that prevents a wrong belief: ' +
        'this is protection from two steps overwriting one another, not a security boundary',
    ).toContain('Not a security sandbox.');
  });

  it('shows the state the step is really in, in both directions', () => {
    expect(checked(markup({ use: 'fresh-copy' }))).toBe(true);
    expect(
      checked(markup({ use: 'project' })),
      'a switch rendered always-off lies about the value it just wrote, and the person turns ' +
        'it on twice wondering why nothing sticks',
    ).toBe(false);
  });

  it('turns the project folder into an own copy, and back', () => {
    expect(nextFolder({ use: 'project' })).toEqual({ use: 'fresh-copy' });
    expect(nextFolder({ use: 'fresh-copy' })).toEqual({ use: 'project' });
  });

  it('never quietly throws away a hand-written path', () => {
    /* `pick` nie ma kontrolki i powstaje wyłącznie z ręcznej poprawki pliku. Przełącznik
     * pokazuje go jako wyłączony, więc jedyne kliknięcie, jakie ma sens, znaczy „chcę własną
     * kopię" — wyjście na `project` kasowałoby cudzą ścieżkę pod pozorem włączania czegoś. */
    const picked: Folder = { use: 'pick', path: '/Users/x/api' };

    expect(checked(markup(picked))).toBe(false);
    expect(nextFolder(picked)).toEqual({ use: 'fresh-copy' });
  });
});
