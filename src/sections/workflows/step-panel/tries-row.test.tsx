/* Wiersz „Try again up to" — liczba rund powrotu, i tylko na kroku, z którego powrót wychodzi.
 *
 * TEJ KONTROLKI NIE MA W MAKIECIE, i to jest zapisane wprost, bo makieta jest jedyną wyrocznią
 * wyglądu tej aplikacji. Grep na `loop`, `again`, `retry` i `turns` w `docs/mockup/index.html` nie
 * daje ani jednego trafienia: pętla powstała po makiecie, na prośbę właściciela.
 *
 * DLACZEGO NA KROKU, SKORO LICZBA NALEŻY DO STRZAŁKI. Panelu strzałki w repo nie ma. Rust już
 * rozstrzygnął miejsce: uwaga o złym zakresie (`check::turns_out_of_range`) czepia się kroku,
 * z którego powrót WYCHODZI, a kliknięcie tej uwagi otwiera panel dokładnie tego kroku. Kontrolka
 * gdzie indziej znaczyłaby, że uwaga prowadzi tam, gdzie nie da się jej spełnić.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie samego „wiersz istnieje". Przechodzi ją implementacja, która
 * pokazuje go na KAŻDYM kroku — czyli stawia pole „ile rund" przy kroku bez pętli. To jest
 * kontrolka bez skutku (niezmiennik 16), i to gorszego rodzaju: wygląda na ustawienie, które
 * czeka na włączenie gdzie indziej. Dlatego oba stany są niżej, na tym samym panelu.
 *
 * Zaciskania zakresu nie da się tu kliknąć (brak jsdom [T3 §2.3, ryzyko 7]), więc mierzone jest
 * to, co jest funkcją: obecność wiersza i wartość, którą pokazuje.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../state/agents';
import type { AgentStep } from '../../../state/workflows';
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
    writeResultsTo: '',
    tools: 'everything',
    skills: [],
    connections: [],
  };
}

function step(): AgentStep {
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
    at: { x: 24, y: 24 },
  };
}

function noop(): void {
  /* panel sterowany: statyczny render nic z tego nie woła */
}

function markup(wayBack: number | null): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step()}
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

/** Wartość pola liczby rund w markupie, albo `null`, kiedy pola nie ma. */
function triesField(html: string): string | null {
  const hit = /<input[^>]*id="step-tries"[^>]*value="([^"]*)"/.exec(html);
  return hit?.[1] ?? null;
}

describe('the number of tries on a way back', () => {
  it('shows the limit on the step the way back leaves from', () => {
    const html = markup(3);

    expect(html).toContain('Try again up to');
    expect(triesField(html)).toBe('3');
    expect(
      html,
      'the sentence has to say what the number does, not just name it: „3" alone tells nobody ' +
        'that the work comes back until it passes',
    ).toContain('until it passes');
  });

  it('is absent on a step with no way back', () => {
    const html = markup(null);

    expect(
      html.includes('Try again up to'),
      'a tries field on a step with no loop is a control with no effect — and the worse kind, ' +
        'because it reads as a setting waiting to be switched on somewhere else',
    ).toBe(false);
    expect(triesField(html)).toBeNull();
  });

  it('shows one try as one, not as an empty field', () => {
    expect(
      triesField(markup(1)),
      'the range starts at one, so this is the value a person really sees after typing it',
    ).toBe('1');
  });
});
