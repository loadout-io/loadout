/* Wiersz triggera: STAN JEST TEKSTEM, CZYNNOŚĆ JEST PRZYCISKIEM — i nazwa warunku nigdy nie
 * schodzi do pustki.
 *
 * ZMIERZONE 2026-08-31, dwie wady jednego wiersza.
 *
 * PIERWSZA: etykietą przycisku było sklejenie zdania o stanie z nazwą czynności
 * (`${status.sentence} · ${retryLabel}`). Żeby przeczytać, co się właściwie stało, trzeba było
 * wodzić kursorem po ŻYWYM przycisku „Run again", którego przypadkowe kliknięcie ODPALA BIEG.
 * Zdanie i czynność to dwie różne rzeczy: jedno się czyta, drugie się robi, i nie mają prawa
 * dzielić jednej kontrolki.
 *
 * DRUGA: `conditionName` budowało nazwę z `words[0]?.toUpperCase() + words.slice(1)`, a pustkę
 * łapał osobny warunek `words.length === 0`. Liczył on jednak długość PO zamianie myślników na
 * spacje: warunek zapisany jako `-` albo `_` schodził do pojedynczej spacji, więc miał długość
 * 1, przechodził obok tamtego warunku i lądował na ekranie jako PUSTA KOMÓRKA. Ta sama
 * konstrukcja jest jedyną drogą do dosłownego napisu „undefined…" w tej kolumnie — dziś
 * nieosiągalną wyłącznie dlatego, że pilnuje jej warunek stojący obok, a nie sam sposób
 * budowania napisu.
 *
 * KRYTERIUM SĄDZI EKRAN, nie wartość zwróconą przez funkcję (niezmiennik 29): wiersz powstaje
 * przez `renderToStaticMarkup` prawdziwego `TriggersScreen`, a asercje czytają tekst DOKŁADNIE
 * tych elementów, na które patrzy człowiek.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { createTriggersStore } from '../../state/triggers';
import type { TriggerClock, TriggerRunPath, TriggerView } from '../../state/triggers';
import { useWorkspaces } from '../../state/workspaces';
import type { TriggerIo } from './io';
import TriggersScreen from './index';

const CLOCK: TriggerClock = {
  now: () => 0,
  setInterval: () => 1,
  clearInterval: () => undefined,
};

const RUN: TriggerRunPath = {
  listWorkflows: async () => [],
  launchRun: async () => null,
  atOnce: () => 3,
};

/** Ta scena nic nie wysyła: pyta wyłącznie o to, co stoi na ekranie nad wczytaną biblioteką. */
async function neverAsked(): Promise<never> {
  throw new Error('this row was rendered, not driven');
}

const IO: TriggerIo = {
  listTriggers: async () => [],
  setTriggerEnabled: neverAsked,
  checkTrigger: neverAsked,
  resumeTrigger: neverAsked,
  retryTrigger: neverAsked,
  createTrigger: neverAsked,
  updateTrigger: neverAsked,
  deleteTrigger: neverAsked,
  testLinearConnection: neverAsked,
};

/** Zdanie, które Loadout mówi o biegu, który naprawdę ruszył. */
const STARTED = 'Started Analysis in Project at 2026-08-21 02:12:09 UTC.';

/** Nazwa czynności — jedyne, co ma stać na przycisku obok tamtego zdania. */
const RUN_AGAIN = 'Run again';

/** Zdanie, którym wiersz nazywa warunek, którego nikt nie zapisał. */
const NO_CONDITION = 'No condition saved';

const SAVED = {
  slug: 'assigned-to-me',
  source: 'Linear',
  workflow: 'analysis.json',
  workflowName: 'Analysis',
  workspace: '/project',
  pollEveryMinutes: 1 as const,
  hasApiKey: true,
  enabled: true,
} as const;

/** Wiersz o biegu, który naprawdę ruszył — jedyny stan, który daje czynność „Run again". */
const ACCEPTED: TriggerView = {
  ...SAVED,
  condition: 'Assigned to you',
  status: {
    kind: 'accepted',
    workflow: 'Analysis',
    workspace: '/project',
    receiptAt: Date.UTC(2026, 7, 21, 2, 12, 9, 700),
  },
};

/** Wiersz czekający na pierwsze zgłoszenie: pytanie dotyczy kolumny warunku, nie stanu. */
const WATCHING: TriggerView = {
  ...SAVED,
  condition: 'Assigned to you',
  status: { kind: 'armed' },
};

function screenWith(trigger: TriggerView): string {
  const store = createTriggersStore(IO, CLOCK, RUN);
  store.setState({ triggers: [trigger] });
  return renderToStaticMarkup(<TriggersScreen store={store} />);
}

/** Jeden wiersz biblioteki, wycięty z całego ekranu. */
function rowOf(markup: string, slug: string): string {
  return (
    new RegExp(`<li[^>]*data-trigger-row=["']${slug}["'][^>]*>[\\s\\S]*?<\\/li>`).exec(
      markup,
    )?.[0] ?? ''
  );
}

function readable(inside: string): string {
  return inside
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Znacznik, w którym stoi zdanie o stanie — razem z jego nazwą, bo o nią tu chodzi. */
function statusCarrier(row: string): { readonly tag: string; readonly text: string } {
  const found = /<([a-z]+)\b[^>]*\bdata-trigger-status\b[^>]*>([\s\S]*?)<\/\1>/.exec(row);
  return { tag: found?.[1] ?? '', text: readable(found?.[2] ?? '') };
}

/** Napis na przycisku, który puszcza bieg jeszcze raz. */
function actionLabel(row: string): string {
  return readable(
    /<button\b[^>]*\bdata-trigger-run-again\b[^>]*>([\s\S]*?)<\/button>/.exec(row)?.[1] ?? '',
  );
}

/** Komórki tekstowe wiersza, W KOLEJNOŚCI — także te, które wyszły puste. */
function cells(row: string): readonly string[] {
  return [...row.matchAll(/<span\b[^>]*\bdata-trigger-text\b[^>]*>([\s\S]*?)<\/span>/g)].map(
    (match) => readable(match[1] ?? ''),
  );
}

beforeEach(() => {
  useWorkspaces.setState({
    all: [{ id: '/project', name: 'Project', folder: '/project' }],
    activeId: '/project',
    said: null,
  });
});

describe('a trigger row separates what happened from what a person can do', () => {
  it('leaves the sentence about what happened outside the live control', () => {
    const row = rowOf(screenWith(ACCEPTED), SAVED.slug);
    expect(row, 'the row for a trigger that already started was never rendered').not.toBe('');

    const carrier = statusCarrier(row);
    expect(
      carrier.tag,
      'reading what happened means pointing at a live control that starts work. One slip of ' +
        'the hand and the whole thing goes out again, so the only safe way to read this row is ' +
        'not to touch it',
    ).not.toBe('button');
    expect(
      carrier.text,
      'the sentence about what happened has to stand on its own, word for word, where it can ' +
        'be read without reaching for anything that acts',
    ).toBe(STARTED);
  });

  it('gives the action a button that says only what it does', () => {
    const row = rowOf(screenWith(ACCEPTED), SAVED.slug);

    expect(
      actionLabel(row),
      'the button carries the sentence about what happened glued to the name of the action, ' +
        'so a person cannot tell what will happen from what already did. A control is named ' +
        'after what it does and after nothing else',
    ).toBe(RUN_AGAIN);
  });
});

describe('a trigger row always has a name for its condition', () => {
  it('names a condition that normalises away to nothing instead of leaving the cell blank', () => {
    const row = rowOf(screenWith({ ...WATCHING, condition: '-' }), SAVED.slug);

    expect(
      cells(row)[1],
      'a saved condition of "-" turns into a single space on its way to the screen, so the ' +
        'column stands empty and the row says nothing at all about when it fires. The wording ' +
        'for a condition nobody saved exists, and this row walks straight past it',
    ).toBe(NO_CONDITION);
  });

  it('answers every empty way of writing a condition with the same sentence', () => {
    for (const condition of ['', '   ', '-', '__', ' - - ']) {
      const row = rowOf(screenWith({ ...WATCHING, condition }), SAVED.slug);
      expect(
        cells(row)[1],
        `a saved condition of "${condition}" reached the screen as something else. Every one ` +
          'of these is the same fact — nobody said when this trigger fires — and the row has ' +
          'one sentence for it',
      ).toBe(NO_CONDITION);
    }
  });
});
