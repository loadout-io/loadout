/* PIERWSZE OTWARCIE JEST DRZWIAMI, NIE TABLICZKĄ „PUSTO".
 *
 * ZMIERZONE 2026-08-31, na tej gałęzi, przed tym plikiem. Świeży ekran pracy rysował przewodnik
 * złożony z trzech wierszy tekstu w liście `<ol data-first-run>` i ANI JEDNEJ innej rzeczy:
 * bez tytułu ekranu, bez zdania o tym, czym ta aplikacja jest, bez jednego gotowego agenta,
 * bez licznika drogi i bez wyjścia dla kogoś, kto zna drogę. Największy stopień, jaki się na nim
 * pojawiał, to `--text-ui` (13 px) na przycisku pierwszego kroku. Właściciel odrzucił ten ekran
 * dwa razy tymi samymi słowami: „nudne", „UX totalnie nieoczywisty".
 *
 * CO TE KRYTERIA SĄDZĄ, A CZEGO NIE. Sądzą ZACHOWANIE i zdania, które widzi człowiek
 * (niezmiennik 29): każde poniżej czyta markup wyprodukowany przez `<Run />` z zasianego dysku
 * albo woła DOKŁADNIE tę funkcję, którą kontrolka podaje w `onClick`. Ani jedno nie pyta
 * o obecność napisu w pliku źródłowym — napis w pliku to nie jest ekran.
 *
 * SŁABA WERSJA, ŚWIADOMIE ODRZUCONA: „na ekranie stoi napis Welcome to Loadout". Przechodzi ją
 * `<h1>` w wygaszonym kolorze, w stopniu 13 px, obok czterech kafelków, których przyciski nic
 * nie robią — czyli dokładnie ten ekran, który właściciel odrzucił. Dlatego pytania niżej są
 * o STOPIEŃ tytułu, o LICZBĘ akcentów, o to, czy przycisk galerii NAPRAWDĘ dojeżdża do dysku,
 * i o to, czy prowadzenie ZNIKA, kiedy przestaje być potrzebne.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';

/* Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki. Efekty i tak nie biegną pod
 * `renderToStaticMarkup`, ale sam import `@tauri-apps/api/core` musi się rozwiązać, inaczej plik
 * przewraca się na ZBIERANIU, a „nic nie znaleziono" czyta się jak zdana asercja. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: () => Promise.resolve(undefined),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('./index')).default;
const { useWorkspaces } = await import('../../state/workspaces');
const { useRun } = await import('../../state/run');
const { missingForSave } = await import('../../state/agents');
const { forgetWhatIsReady, rememberAgents, rememberRuns, rememberWorkflows, whatIsReady } =
  await import('./whats-ready');
const { STARTERS, forgetStarters, starterWritesTo, whatItMayDo } = await import('./starters');
const { stepAside, wantGuidanceAgain } = await import('./guidance');
const { moveFor } = await import('../../ui/palette/keys');
const { guidanceHears } = await import('./first-run');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

const RUNNABLE: Choice = {
  path: 'ship-a-feature.json',
  name: 'Ship a feature',
  steps: [
    { id: 's1', name: 'Reproduce', state: 'pending', kind: 'agent', at: { x: 40, y: 40 } },
    { id: 's2', name: 'Fix', state: 'pending', kind: 'agent', at: { x: 40, y: 170 } },
  ],
  links: [],
};

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

/** Ekran pracy z dokładnie takim dyskiem, jaki podano. Każdy fakt wchodzi drogą produkcji. */
function screen(there: {
  folders?: number;
  agents?: number;
  workflows?: readonly Choice[];
}): string {
  useWorkspaces.setState({
    all: (there.folders ?? 0) > 0 ? [HERE] : [],
    activeId: (there.folders ?? 0) > 0 ? HERE.id : null,
    said: null,
  });
  useRun.setState({ workflow: '', steps: [], links: null });
  rememberWorkflows(there.workflows ?? []);
  rememberAgents(there.agents ?? 0);
  rememberRuns((there.folders ?? 0) > 0 ? HERE.folder : null, []);
  return readable(renderToStaticMarkup(<Run />));
}

/** Tekst bez znaczników, ze ściśniętymi odstępami. */
function words(html: string): string {
  return html
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Element o tym znaczniku, wycięty po głębokości — nie leniwym wzorcem, który kończy za wcześnie. */
function region(markup: string, marker: string): string {
  const open = new RegExp('<([a-z]+)[^>]*\\s' + marker + '\\b[^>]*>');
  const hit = open.exec(markup);
  if (hit === null) return '';
  const name = hit[1] ?? '';
  const from = hit.index;
  const walk = new RegExp('<(/?)' + name + '\\b[^>]*>', 'g');
  walk.lastIndex = from;
  let depth = 0;
  let step = walk.exec(markup);
  while (step !== null) {
    depth += step[1] === '/' ? -1 : 1;
    if (depth === 0) return markup.slice(from, step.index + step[0].length);
    step = walk.exec(markup);
  }
  return markup.slice(from);
}

/** Kontrolki, których człowiek naprawdę może użyć — wyłączona jest widoczna i bezużyteczna. */
function liveButtons(html: string): readonly string[] {
  return [...html.matchAll(/<button\b[^>]*>/g)]
    .map((one) => one[0])
    .filter((tag) => !/\sdisabled\b/.test(tag));
}

/**
 * Czy okno NAPRAWDĘ odpowiada na klawisz narysowany na ekranie.
 *
 * Pytamy tej samej funkcji, którą woła nasłuch okna (`src/ui/palette/keys.ts`, `moveFor`), oraz
 * tej, która obsługuje wyjście z przewodnika. Lista skrótów z palety byłaby słabszą wyrocznią:
 * ona mówi, co ktoś SPISAŁ, a nie na co okno reaguje.
 */
function windowAnswers(cap: string): boolean {
  if (cap === 'Esc') return guidanceHears('Escape', false) !== 'nothing';
  const held = cap.startsWith('⌘');
  const key = held ? cap.slice(1) : cap;
  const move = moveFor(
    { key, metaKey: held, ctrlKey: false, altKey: false, shiftKey: false },
    null,
    false,
  );
  return move.move !== 'none';
}

afterEach(() => {
  forgetWhatIsReady();
  forgetStarters();
  useRun.setState({ workflow: '', steps: [], links: null });
});

describe('the first screen a person ever sees says what to press', () => {
  it('greets with a title in the biggest step of the ladder, one sentence and one loud control', () => {
    const markup = screen({});
    const hero = region(markup, 'data-first-hero');
    expect(
      hero,
      'the empty work area draws no welcome at all. A person landing here reads three grey ' +
        'rows and no word about what this application is for.',
    ).not.toBe('');

    const title = /<h1[^>]*>([\s\S]*?)<\/h1>/.exec(hero)?.[0] ?? '';
    expect(title, 'the welcome carries no title element').not.toBe('');
    expect(
      words(title),
      'the title of the first screen does not greet anybody: ' + JSON.stringify(words(title)),
    ).toContain('Welcome to Loadout');
    expect(
      /\btext-display\b/.test(title),
      'the title is not drawn in the top step of the ladder. Everything on this screen is then ' +
        'the same size as everything else, which is the measured reason the owner called it ' +
        'boring twice: with no hero, hierarchy comes out as a grey box beside a grey box.',
    ).toBe(true);

    expect(
      liveButtons(hero).length,
      'the welcome offers ' +
        String(liveButtons(hero).length) +
        ' controls a person can press. It has to offer exactly one: two of them mean neither ' +
        'is the next move.',
    ).toBe(1);
    expect(
      [...hero.matchAll(/btn-primary/g)].length,
      'the accent fill is not on exactly one control of the welcome. Accent means "press this", ' +
        'so two of them mean neither.',
    ).toBe(1);

    const reassure = region(markup, 'data-first-reassure');
    expect(
      words(reassure),
      'nothing under the big control says how long this takes or where the work stays. A person ' +
        'who does not know either has every reason not to press it.',
    ).toContain('nothing leaves this Mac');
  });

  it('counts the road from what is on disk, not from a number it keeps for itself', () => {
    const nothing = region(screen({}), 'data-road-count');
    expect(
      nothing,
      'the screen shows no count of how far along the first run is, so the road has no ' +
        'beginning and no end a person can see',
    ).not.toBe('');
    expect(words(nothing), 'a fresh install is not at zero of three').toBe('0 of 3 done');

    expect(
      words(region(screen({ agents: 1 }), 'data-road-count')),
      'one agent is saved and the road still reads the same. A road that does not move when ' +
        'the work moves is a picture, not a road.',
    ).toBe('1 of 3 done');

    expect(
      words(region(screen({ agents: 2, workflows: [RUNNABLE] }), 'data-road-count')),
      'an agent and a workflow are there and the road disagrees',
    ).toBe('2 of 3 done');
  });

  it('lights exactly one stop, and it is the first one that is not finished', () => {
    const road = region(screen({ agents: 1 }), 'data-first-run');
    const stops = [...road.matchAll(/<li[^>]*\bdata-first-step\b[\s\S]*?<\/li>/g)].map(
      (one) => one[0],
    );
    const state = (row: string): string => /data-step-state="([^"]*)"/.exec(row)?.[1] ?? '';
    expect(
      stops.map(state),
      'with one agent saved the road has to read done, now, later — anything else tells a ' +
        'person to do something they already did, or hides the one thing that is left',
    ).toEqual(['done', 'now', 'later']);
    expect(
      stops.map((row) => words(row).length > 12),
      'a stop of the road says almost nothing, so the screen shows a line of numbered blanks',
    ).toEqual([true, true, true]);
  });

  /* STRAŻNIK, NIE STEROWNIK, i to jest powiedziane wprost. Ten punkt przechodził PRZED tą
   * przebudową i ma przechodzić po niej: bez folderu nie ma gdzie pracować, więc zaproszenie
   * musi przeżyć każdą zmianę układu. Jest tu, bo nowy ekran przenosi je z osobnego przycisku
   * do wnętrza drogi, a przeniesienie jest dokładnie tą chwilą, w której takie rzeczy giną.
   * `e2e/tests/plus-opens-a-terminal.spec.ts` liczy ten sam znacznik z drugiej strony. */
  it('offers the folder exactly once while there is none, and never once there is one', () => {
    const without = screen({});
    expect(
      [...without.matchAll(/\bdata-add-workspace\b/g)].length,
      'the empty screen offers to pick the first folder ' +
        String([...without.matchAll(/\bdata-add-workspace\b/g)].length) +
        ' times. Without a folder no run can start, and two invitations to the same thing mean ' +
        'a person has to work out which one is real.',
    ).toBe(1);

    const with_ = screen({ folders: 1 });
    expect(
      [...with_.matchAll(/\bdata-add-workspace\b/g)].length,
      'a folder is already chosen and the screen still asks for one',
    ).toBe(0);
  });

  it('shows ready-made agents whose button really writes one to disk', async () => {
    const markup = screen({});
    const gallery = region(markup, 'data-starters');
    expect(
      gallery,
      'the first screen offers nothing a person can take. Writing an agent from nothing is the ' +
        'move eight to eleven steps away from a first run, and it is the only move offered.',
    ).not.toBe('');
    const shown = [...gallery.matchAll(/data-starter="([^"]*)"/g)].map((one) => one[1]);
    expect(shown, 'the ready-made agents are not the three the design draws').toEqual([
      'Scout',
      'Builder',
      'Needle',
    ]);
    expect(
      words(gallery),
      'no card says what pressing it does, so four boxes stand there meaning nothing',
    ).toContain('Use this agent');

    /* CO NAPRAWDĘ DOJECHAŁO NA DYSK. Atrapa jest tą samą granicą, którą wołają te przyciski. */
    const written: { name: string; expected: string | null; id: string }[] = [];
    const back = starterWritesTo({
      newId: () => Promise.resolve('0198-scout'),
      save: (agent, expectedRevision) => {
        written.push({ name: agent.name, expected: expectedRevision, id: agent.id });
        return Promise.resolve('rev-1');
      },
    });
    try {
      const [scout] = STARTERS;
      expect(scout, 'there is no first ready-made agent to take').toBeDefined();
      const landed = await (scout?.take() ?? Promise.resolve(false));
      expect(
        landed,
        'pressing "Use this agent" did not write an agent. A card that looks like it makes one ' +
          'and makes nothing is the dead control invariant 16 forbids.',
      ).toBe(true);
      expect(
        written.map((one) => one.name),
        'nothing reached the one edge this application has for saving an agent',
      ).toEqual(['Scout']);
      expect(
        written[0]?.id,
        'the agent went to disk without the identifier the mint handed out, so it would land ' +
          'under an empty name',
      ).toBe('0198-scout');
      expect(
        written[0]?.expected,
        'a brand new agent was saved as if a file for it already existed, which is how one ' +
          'person quietly overwrites another',
      ).toBe(null);
      expect(
        whatIsReady().agents,
        'the agent is on disk and the screen still counts zero of them, so the road stands ' +
          'still while the work moved',
      ).toBe(1);
    } finally {
      back();
    }
  });

  it('offers ready-made agents that this application can actually save', () => {
    for (const starter of STARTERS) {
      expect(
        missingForSave(starter.agent),
        'the ready-made agent ' +
          starter.agent.name +
          ' is missing something the save refuses without, so pressing its card would only ' +
          'ever produce a refusal',
      ).toBe(null);
    }
    expect(
      STARTERS.map((one) => whatItMayDo(one.agent)),
      'the words on the cards do not come from the dials of the agent behind them. A card ' +
        'saying "reads only" over an agent allowed to change files is a sentence the data does ' +
        'not carry (invariant 17).',
    ).toEqual(['reads only', 'edits files', 'runs commands']);
  });

  it('names only keys this application really answers', () => {
    const row = region(screen({}), 'data-first-keys');
    expect(
      row,
      'the screen never says a keyboard exists. Every screen in the application it is compared ' +
        'against shows its keys, and this one hid them.',
    ).not.toBe('');
    /* CAŁY EKRAN, nie sam wiersz. Klawisz obiecany przy dużym przycisku jest tą samą obietnicą
       co klawisz w wierszu na dole, a `⌘N` z makiety siedziałoby właśnie tam. */
    const everywhere = region(screen({}), 'data-first-open');
    const named = [...everywhere.matchAll(/<kbd[^>]*>([^<]*)<\/kbd>/g)].map((one) =>
      (one[1] ?? '').trim(),
    );
    expect(named.length, 'the screen names no key at all').toBeGreaterThan(2);
    const invented = named.filter((cap) => !windowAnswers(cap));
    expect(
      invented,
      'these keys are drawn on the screen and this application does not answer them: ' +
        JSON.stringify(invented) +
        '. A key printed beside a sentence is a promise, and a promise nothing keeps is the ' +
        'same defect as a button with no handler.',
    ).toEqual([]);
  });

  it('lets a person who already knows the way step out of the guidance', () => {
    const standing = screen({});
    expect(
      region(standing, 'data-step-aside'),
      'the guidance has no way out. A person who has used this application before now reads a ' +
        'welcome and a road they do not need, on the screen where their work belongs, with no ' +
        'way to put it away.',
    ).not.toBe('');

    stepAside();
    try {
      const after = screen({});
      expect(
        region(after, 'data-first-run'),
        'the way out was taken and the road is still standing, so the control says one thing ' +
          'and does another',
      ).toBe('');
      expect(
        region(after, 'data-starters'),
        'the way out was taken and the ready-made agents still hold the work area',
      ).toBe('');
      expect(
        [...after.matchAll(/\sdata-empty\b/g)].length,
        'stepping out of the guidance left the work area with no invitation at all. An empty ' +
          'screen that says nothing is the notice DESIGN §6 rules out, not the invitation it ' +
          'asks for.',
      ).toBe(1);
    } finally {
      wantGuidanceAgain();
    }
  });

  /* STRAŻNIK, NIE STEROWNIK. Przechodził przed tą przebudową, bo przewodnik był trzema
   * wierszami tekstu i `index.tsx` przestawał go rysować po komplecie. Po przebudowie w to
   * miejsce wchodzi CAŁY ekran powitalny, więc ten sam warunek zaczyna pilnować dużo więcej. */
  it('takes the guidance away the moment it stops being needed', () => {
    const done = screen({ folders: 1, agents: 1, workflows: [RUNNABLE] });
    expect(
      region(done, 'data-first-run'),
      'everything is set up and the road is still on the screen. Three ticked rows standing in ' +
        'the place the work belongs is a list about nothing (invariant 17).',
    ).toBe('');
    expect(
      region(done, 'data-first-hero'),
      'everything is set up and the screen still welcomes a person who has been here for weeks',
    ).toBe('');
    expect(
      region(done, 'data-starters'),
      'everything is set up and the ready-made agents still take the work area',
    ).toBe('');
    expect(
      words(done).includes('Your first run'),
      'the words "Your first run" still stand on a screen where the first run is behind us',
    ).toBe(false);
  });
});
