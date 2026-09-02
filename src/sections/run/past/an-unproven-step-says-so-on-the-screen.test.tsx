/* Z-4, druga połowa kryterium 1: zdanie o ocalałym dochodzi na EKRAN, nie tylko do `run.json`.
 *
 * Rust dowodzi, że to zdanie powstaje i że przeżywa zapis oraz odczyt (`tests/it/
 * a_run_always_settles.rs`). To jest pytanie następne i osobne: czy człowiek je zobaczy.
 * Niezmiennik 29 mówi wprost, że wartość zwrócona z funkcji dowodzi istnienia mechanizmu,
 * a zdanie w markupie dowodzi, że produkt działa — i że kryterium o komunikacie nie wolno
 * poprzestać na tym pierwszym.
 *
 * SŁABA WERSJA: zamontować sam panel (`<PastRuns />`) albo poszukać zdania gdziekolwiek
 * w markupie. Pierwsza przechodzi na komponencie, którego ekran pracy nigdzie nie montuje;
 * druga przechodzi, gdy zdanie wyląduje w podsumowaniu kroku albo w atrybucie `title`, czyli
 * tam, gdzie nikt go nie szuka. Dlatego montowany jest CAŁY ekran sekcji (`<Run />`), historię
 * otwiera ta sama komenda, którą woła wiersz wejścia, a zdanie musi stać w `[data-step-problem]`
 * WEWNĄTRZ sekcji tego jednego kroku.
 *
 * TEN PLIK JEST ZIELONY TAKŻE NA STARYM KODZIE i to jest powiedziane wprost: wada Z-4 siedziała
 * po stronie Rusta (bieg nie wracał, więc zdania nie było wcale), a panel rysował `error` od
 * zawsze. Czerwień przed poprawką dowodzą testy rustowe; ten plik zamyka ostatnie ogniwo drogi,
 * którego tamte nie widzą, i pilnuje go przed regresją.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki (to repo nie ma jsdom).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastRun, PastRunRow } from '../io';

/**
 * Zdanie, które Rust zapisuje krokowi po grupie, której nie umiał uznać za martwą.
 *
 * Kopia stałej `STEP_SURVIVOR_ERROR` z `commands/run.rs`, co do znaku. Drut niesie je jako zwykły
 * tekst pola `error`, więc dwie kopie są tu nieuniknione — ta strona nie ma jak zapytać tamtej
 * o napis. Rozjazd łapie test rustowy, który asertuje dokładnie ten sam ciąg w `run.json`.
 */
const SURVIVOR =
  'This step finished its work, but Loadout could not make sure everything it started had ' +
  'stopped, so some of it may still be running.';

/** Bieg, w którym taki krok stoi. */
const RUN: PastRunRow = {
  folder: '20260902-101500__0198a1f2-3b4c-7d5e-8f60-000000000904',
  when: '2026-09-02 10:15',
  title: 'Ship a feature',
  state: 'failed',
  steps: 2,
  costUsd: 0.5,
  said: null,
};

/** Krok, który zszedł czysto — po to, żeby zdanie o ocalałym miało z czym się nie mylić. */
const CLEAN = '01a02b3c-15f5-7f13-a86f-f2f856e4d901';

/** Krok, którego grupa nie odpowiedziała `ESRCH`. */
const LEFTOVER = '01a02b3c-15f5-7f13-a86f-f2f856e4d902';

const OPENED: PastRun = {
  folder: RUN.folder,
  when: RUN.when,
  title: RUN.title,
  state: RUN.state,
  workflowFile: 'ship-a-feature.json',
  steps: [
    {
      id: CLEAN,
      tile: 's_plan',
      name: 'Plan',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Wrote the plan.',
      error: '',
      costUsd: 0.25,
      lines: [],
    },
    {
      id: LEFTOVER,
      tile: 's_build',
      name: 'Build',
      agent: 'claude',
      state: 'failed',
      // Podsumowanie MÓWI, że praca się udała, i to jest treść tej fikstury: człowiek dostaje
      // krok, który wygląda na skończony, i tylko czerwone zdanie niżej mówi mu, że coś mogło
      // po nim zostać. Bez tej pary asercja niżej przechodziłaby na pustym kroku.
      summary: 'Stored the greeting in the file.',
      error: SURVIVOR,
      costUsd: 0.25,
      lines: [],
    },
  ],
  handoffs: [],
  said: null,
};

/* Atrapa granicy oddaje `Promise<unknown>` JAWNIE, a nie z wnioskowania: bez adnotacji `vi.fn`
 * zamraża typ pierwszego ciała, a to niżej podmieniamy na takie, które oddaje bieg z historii. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string): Promise<unknown> => Promise.resolve(undefined)),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('../index')).default;
const { openHistoryFromLine, openOneRun } = await import('../history-command');
const { useWorkspaces } = await import('../../../state/workspaces');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };
useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function screen(): string {
  return readable(renderToStaticMarkup(<Run />));
}

/**
 * Czerwone zdanie TEGO kroku — albo pusty napis, kiedy w jego sekcji żadnego nie ma.
 *
 * Zawężenie do jednej sekcji jest tu treścią, nie ostrożnością: zdanie znalezione gdziekolwiek
 * w otwartym biegu przechodziłoby także wtedy, gdy panel przypnie je do niewłaściwego kroku —
 * a wtedy człowiek idzie sprawdzać maszynę po kroku, który zszedł czysto.
 */
function problemOf(markup: string, step: string): string {
  const opens = markup.indexOf('data-past-step="' + step + '"');
  if (opens < 0) return '';
  const next = markup.indexOf('data-past-step="', opens + 1);
  const section = markup.slice(opens, next < 0 ? markup.length : next);
  const marker = section.indexOf('data-step-problem');
  if (marker < 0) return '';
  const text = section.indexOf('>', marker);
  const ends = section.indexOf('</p>', text);
  if (text < 0 || ends < 0) return '';
  return section.slice(text + 1, ends);
}

const beforeAnything = screen();

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve([RUN]);
  if (command === 'read_run') return Promise.resolve(OPENED);
  return Promise.resolve(undefined);
});

await openHistoryFromLine('');
await openOneRun(HERE.folder, RUN.folder);
const withTheRun = screen();

describe('a step whose leftovers could not be ruled out says so where a person reads it', () => {
  it('says nothing of the sort before anybody opens that run', () => {
    expect(
      beforeAnything.includes(SURVIVOR),
      'the work screen may not carry this sentence before the run is opened, or the check below ' +
        'would pass on a screen that says the same thing whatever happened',
    ).toBe(false);
  });

  it('draws the sentence in the opened run, hung on the step it belongs to', () => {
    expect(
      withTheRun,
      'picking the row has to open that run, addressed by the folder the row carried',
    ).toContain('data-past-run="' + RUN.folder + '"');
    expect(
      problemOf(withTheRun, LEFTOVER),
      'the one sentence telling a person that something this step started may still be going ' +
        'has to reach the screen, word for word, in the red slot of that step. Kept only in ' +
        'run.json it is a fact nobody meets: the step reads as finished work and the machine ' +
        'keeps paying for what was left behind.',
    ).toBe(SURVIVOR);
  });

  it('leaves the step that came down cleanly without a red word', () => {
    expect(
      problemOf(withTheRun, CLEAN),
      'a step that came down cleanly may not borrow the sentence of the one that did not, or ' +
        'the panel sends a person looking in the wrong place',
    ).toBe('');
  });
});
