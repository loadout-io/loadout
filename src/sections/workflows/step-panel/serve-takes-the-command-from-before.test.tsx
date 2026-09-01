/* Człowiek może powiedzieć: „komendę wymyśli krok przede mną".
 *
 * # Zamówienie
 *
 * Właściciel 2026-08-30: „dajmy taki step o nazwie run preview app, tylko że agent sam ma
 * rozkminić jakie komendy użyć do odpalenia, my nie ingerujemy bo nie chcę w każdym projekcie
 * osobno wpisywać na front i backend command".
 *
 * # Czego to kryterium pilnuje
 *
 * Że pole NIE ŻYJE WYŁĄCZNIE W PLIKU. Rust umie je przeczytać, walidator umie je uszanować,
 * a bieg umie po nie sięgnąć — i to wszystko jest bez znaczenia, dopóki człowiek nie ma jak go
 * ustawić inaczej niż ręczną edycją JSON-a. To jest niezmiennik 16 czytany w drugą stronę:
 * kontrolka bez handlera nie wchodzi do repo, a pole bez kontrolki nie jest funkcją produktu.
 *
 * # Dlaczego pole komendy GAŚNIE
 *
 * Bo komenda ma jedno źródło naraz. Dwa wypełnione pola obok siebie każą człowiekowi zgadywać,
 * które wygra — a przegrane pole zostaje na ekranie i wygląda na obowiązujące.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { ServeStep } from '../../../state/workflows';
import { ServePanel } from './serve-panel';

/** Kafelek „uruchom i zostaw" w kształcie z pliku. */
function serve(over: Partial<ServeStep> = {}): ServeStep {
  return {
    kind: 'serve',
    id: 's_app',
    name: 'Run preview app',
    command: '',
    folder: { use: 'same-copy' },
    at: { x: 0, y: 0 },
    ...over,
  };
}

/** Markup panelu dla tego kafelka, w podanym stanie okablowania. */
function markup(
  step: ServeStep,
  wiring: { stepBefore?: string | null; handsItOver?: boolean } = {},
): string {
  return renderToStaticMarkup(
    <ServePanel
      step={step}
      stepBefore={wiring.stepBefore ?? null}
      handsItOver={wiring.handsItOver ?? false}
      onEditStep={() => {
        /* To kryterium pyta o markup, nie o skutek zmiany. */
      }}
      onAskTheStepBefore={() => {
        /* Jak wyżej. */
      }}
    />,
  );
}

describe('the control that hands the command to the step before', () => {
  it('is on the panel at all, or the field lives only in the file', () => {
    expect(
      markup(serve()).includes('data-field="commandFrom"'),
      'without a control, the only way to set this is editing the JSON by hand — and a field ' +
        'nobody can reach is a field the product does not have',
    ).toBe(true);
  });

  it('says in plain words what it does', () => {
    expect(
      markup(serve()).includes('Let the step before this one work out the command'),
      'the sentence has to say what happens, not name a mechanism. This tile is the one thing ' +
        'a person sets up once and reuses across every project',
    ).toBe(true);
  });

  it('turns the command field off while the step before owns it', () => {
    const html = markup(serve({ commandFrom: { field: 'command' } }));
    expect(
      /id="serve-command"[^>]*disabled/.test(html),
      'a command has one source at a time. Two filled fields side by side make the person guess ' +
        'which one wins, and the losing one stays on screen looking like it applies. It ' +
        'rendered: ' +
        html.slice(0, 400),
    ).toBe(true);
  });

  it('leaves the command field usable when the person types it themselves', () => {
    const html = markup(serve({ command: 'npm run dev' }));
    expect(/id="serve-command"[^>]*disabled/.test(html)).toBe(false);
  });

  it('says which step owes it the command, and which field', () => {
    const html = markup(serve({ commandFrom: { field: 'run-preview-app' } }), {
      stepBefore: 'Build it',
      handsItOver: false,
    });

    expect(
      html.includes('Build it') && html.includes('run-preview-app'),
      'the person has to know WHICH step owes it and WHICH field. Naming neither leaves them ' +
        'opening tiles one by one to find out: ' +
        html.slice(0, 400),
    ).toBe(true);
  });

  it('offers to ask that step, as a click and not as a side effect', () => {
    const html = markup(serve({ commandFrom: { field: 'run-preview-app' } }), {
      stepBefore: 'Build it',
      handsItOver: false,
    });

    expect(
      html.includes('data-field="askTheStepBefore"'),
      'ticking a box on THIS tile that quietly edits ANOTHER one is the kind of magic that makes ' +
        'people stop trusting an editor. A button says what it will do before it does it',
    ).toBe(true);
  });

  it('stops offering once that step already hands it over', () => {
    const html = markup(serve({ commandFrom: { field: 'run-preview-app' } }), {
      stepBefore: 'Build it',
      handsItOver: true,
    });

    expect(
      html.includes('data-field="askTheStepBefore"'),
      'a button that repeats work already done reads as work not done',
    ).toBe(false);
    expect(html.includes('and this tile runs it')).toBe(true);
  });

  it('says so plainly when nothing points at the tile at all', () => {
    const html = markup(serve({ commandFrom: { field: 'run-preview-app' } }), {
      stepBefore: null,
    });

    expect(
      html.includes('Draw an arrow'),
      'a tile nothing points at has nobody to work the command out, and sending the person to a ' +
        'step that does not exist is worse than saying so',
    ).toBe(true);
    expect(html.includes('data-field="askTheStepBefore"')).toBe(false);
  });
});
