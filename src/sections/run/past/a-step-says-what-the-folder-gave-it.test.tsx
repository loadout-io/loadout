/* Krok mówi CZŁOWIEKOWI, co wczytał z folderu poza tym, co dał mu bieg (niezmiennik 29).
 *
 * DLACZEGO TO ZDANIE W OGÓLE ISTNIEJE. Krok staje w cudzym repozytorium i bierze stamtąd rzeczy,
 * których Loadout mu nie dał. Ile dokładnie, zależy od wersji aplikacji agenta i raz już zmieniło
 * się po cichu: na 2.1.251 wchodził także plik instrukcji projektu i sześć kroków zapisało przez
 * to pliki wyników wbrew temu, co kazał im Loadout; na 2.1.260 już nie wchodzi (zmierzone
 * 2026-09-04, trzy przebiegi z kontrolą negatywną). Człowiek czytający historię nie miał ani
 * jednego miejsca, w którym mógłby zobaczyć którąkolwiek z tych dwóch sytuacji — i to jest ta
 * dziura, nie jedna czy druga odpowiedź vendora.
 *
 * ZDANIE MÓWI O RÓŻNICY, NIE O CAŁEJ LIŚCIE (2026-09, Z-47). Do tego dnia stało tu „this step
 * also reads N skills", liczone ze WSZYSTKIEGO, co ogłosiło CLI — także z umiejętności, które
 * bieg sam włożył temu krokowi. Zdanie prawdziwe przy każdym kroku każdego biegu nie jest
 * odpowiedzią na pytanie, dla którego ten rekord powstał; różnicę i tak trzeba było policzyć
 * ręcznie, pozycja po pozycji. Liczy ją teraz Rust przy odczycie biegu, bo to on wie, jakimi
 * nazwami bieg przypina swoje rzeczy (`commands::history`, niezmiennik 23).
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(sentenceOf(step)).toBe('…')`. Przechodzi ją czysta
 * funkcja, której panel nigdy nie woła — czyli dokładnie ta klasa, dla której to repo powstało
 * (AGENTS.md, niezmiennik 29). Dlatego montowany jest CAŁY ekran pracy (`<Run />`), ta sama
 * decyzja i ten sam powód co w `history-reaches-the-screen.test.tsx`: sam panel przechodziłby na
 * komponencie, którego ekran nigdzie nie montuje.
 *
 * DRUGI I TRZECI KROK FIKSTURY SĄ CISZĄ I ŻADEN Z NICH NIE JEST OZDOBĄ. Drugi nie ma rekordu
 * wcale — jak każdy krok Codeksa, każdy kafelek „sprawdź" i każdy bieg sprzed 2026-09. Trzeci
 * rekord MA, pełen własności biegu, i milczy mimo to: implementacja rysująca to zdanie z samej
 * obecności rekordu przechodzi wszystko powyżej i mówi o cudzym folderze przy kroku, który nie
 * wziął z niego ani jednej rzeczy.
 *
 * Granica jest atrapą: żadnego żywego Tauri i żadnej przeglądarki.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { PastRun, PastRunRow } from '../io';

/** Bieg, który da się otworzyć. */
const SHIP: PastRunRow = {
  folder: '20260904-101500__0198a1f2-3b4c-7d5e-8f60-000000000007',
  when: '2026-09-04 10:15',
  title: 'Ship a feature',
  workflowFile: 'ship-a-feature.json',
  state: 'succeeded',
  steps: 2,
  costUsd: 1,
  said: null,
};

/** Folder, w którym stanął krok Claude'a — sama nazwa, bo tyle niesie drut. */
const FOLDER = 'ledger-ui';

/** Ile umiejętności wymieniło o sobie CLI tego kroku. */
const SKILLS = [
  'deep-research',
  'frontend-design',
  'brainstorming',
  'executing-plans',
  'writing-plans',
  'writing-skills',
  'systematic-debugging',
  'test-driven-development',
  'using-git-worktrees',
  'verification-before-completion',
  'receiving-code-review',
  'requesting-code-review',
  'finishing-a-development-branch',
  'dispatching-parallel-agents',
  'subagent-driven-development',
  'using-superpowers',
  'code-review',
  'design-sync',
];

/**
 * Zdanie, po które człowiek tu przychodzi — złożone przez Rust, tu tylko przepisane.
 *
 * NIE MA W NIM PLIKU INSTRUKCJI PROJEKTU, i to jest rozstrzygnięcie, nie skrócenie. Kryterium
 * Z-16 pisano, gdy plik ten docierał do kroku (2.1.251); sonda z 2026-09-04 na 2.1.260 pokazała,
 * że przy dzisiejszym argv Loadouta nie dociera, a granica nie ma ani jednego pola, po którym
 * dałoby się poznać, że jednak. Zdanie mówi więc wyłącznie to, co aplikacja agenta sama o sobie
 * ogłosiła — nazwanie pliku byłoby zgadywaniem podanym człowiekowi jako fakt.
 *
 * Liczby są tu tymi, które policzyłby Rust z rekordu niżej: osiemnaście umiejętności i plugin
 * `superpowers` przyszły z folderu, a `auto` jest przekierowaną pamięcią biegu, więc nie wchodzi.
 */
const SENTENCE =
  'This step also read 18 skills and a plugin from ' + FOLDER + ' that Loadout did not give it';

/** Kawałek tamtego zdania, po którym poznać je nawet w innym brzmieniu — do dowodzenia CISZY. */
const ANY_OF_IT = 'also read';

/** Nazwa, której na tym ekranie nie ma prawa być, dopóki nikt nie donosi o jej wczytaniu. */
const HOST_INSTRUCTIONS = 'CLAUDE.md';

/** Krok Claude'a — ten, który się przedstawił, więc wiadomo, co dobrał z folderu. */
const CLAUDE_STEP = '01a02b3c-15f5-7f13-a86f-f2f856e4d781';

/** Krok bez tego rekordu: kafelek „sprawdź" nie woła agenta, więc nie ma kto się przedstawić. */
const CHECK_STEP = '01a02b3c-15f5-7f13-a86f-f2f856e4d782';

/** Krok, który ogłosił pełen rekord — i nie ma w nim ani jednej rzeczy spoza biegu. */
const OWN_STEP = '01a02b3c-15f5-7f13-a86f-f2f856e4d783';

const OPENED: PastRun = {
  folder: SHIP.folder,
  when: SHIP.when,
  title: SHIP.title,
  state: SHIP.state,
  workflowFile: SHIP.workflowFile,
  steps: [
    {
      id: CLAUDE_STEP,
      tile: 's_build',
      name: 'Build',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Wrote the greeting.',
      error: '',
      costUsd: 0.75,
      memory: [],
      loadedByTheApp: {
        folder: FOLDER,
        plugins: ['superpowers'],
        slashCommands: ['deep-research', 'design-sync'],
        skills: SKILLS,
        mcpServers: ['figma'],
        memoryPaths: ['auto'],
        agents: ['Explore', 'Plan'],
      },
      whatLoadoutDidNotGive: SENTENCE,
      lines: [],
    },
    {
      id: CHECK_STEP,
      tile: 's_check',
      name: 'Check',
      agent: '',
      state: 'succeeded',
      summary: 'The checks passed.',
      error: '',
      costUsd: null,
      memory: [],
      lines: [],
    },
    {
      /* KROK Z PEŁNYM REKORDEM I BEZ ANI JEDNEJ RZECZY SPOZA BIEGU. Umiejętność jedzie
         z przedrostkiem pluginu, który zakłada Loadout, plugin nazywa się tak samo, a `auto` to
         przekierowana pamięć biegu — więc różnicy nie ma i Rust nie przysyła zdania. */
      id: OWN_STEP,
      tile: 's_tidy',
      name: 'Tidy',
      agent: 'claude',
      state: 'succeeded',
      summary: 'Tidied up.',
      error: '',
      costUsd: 0.25,
      memory: [],
      loadedByTheApp: {
        folder: FOLDER,
        plugins: ['loadout-skills'],
        slashCommands: [],
        skills: ['loadout-skills:pdf'],
        mcpServers: [],
        memoryPaths: ['auto'],
        agents: [],
      },
      whatLoadoutDidNotGive: null,
      lines: [],
    },
  ],
  handoffs: [],
  said: null,
};

/* Atrapa granicy oddaje `Promise<unknown>` JAWNIE, a nie z wnioskowania: bez adnotacji `vi.fn`
 * zamraża typ pierwszego ciała, a to niżej podmieniamy na takie, które oddaje otwarty bieg. */
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

/**
 * Kawałek markupu należący do JEDNEGO kroku.
 *
 * Kroki są rodzeństwem, więc blok kroku ciągnie się do znacznika następnego albo do końca
 * dokumentu. Bez tego cięcia druga asercja pytałaby o cały ekran, na którym zdanie pierwszego
 * kroku stoi z pełnym prawem — czyli nie pytałaby o nic.
 */
function blockOf(markup: string, step: string): string {
  const opens = markup.indexOf('data-past-step="' + step + '"');
  if (opens < 0) return '';
  const next = markup.indexOf('data-past-step="', opens + 1);
  return next < 0 ? markup.slice(opens) : markup.slice(opens, next);
}

invoked.mockImplementation((command: string): Promise<unknown> => {
  if (command === 'list_runs') return Promise.resolve([SHIP]);
  if (command === 'read_run') return Promise.resolve(OPENED);
  return Promise.resolve(undefined);
});

await openHistoryFromLine('');
await openOneRun(HERE.folder, SHIP.folder);
const withTheRun = readable(renderToStaticMarkup(<Run />));

describe('an opened run says what each step took beyond what it was given', () => {
  it('shows on the screen that the step also read what Loadout did not give it', () => {
    expect(
      withTheRun,
      'the whole point of this record is one sentence a person can read in history. A value that ' +
        'reaches the window and never reaches the markup leaves everybody exactly where they ' +
        'were: with six steps that wrote files nobody asked them to write, and no way to see why.',
    ).toContain(SENTENCE);
  });

  it('names only what the agent app announced, never a file it merely found on disk', () => {
    expect(
      withTheRun.includes(HOST_INSTRUCTIONS),
      'the screen names the project instruction file, and nothing on this wire says the agent ' +
        'read it. Measured 2026-09-04 on 2.1.260: with the flags this app uses, that file does ' +
        'not reach the agent at all. Whoever opens this run to find out why a step wrote a file ' +
        'it was told not to write would be sent to the wrong page — which is the exact fault ' +
        'this record exists to end, made one layer up.',
    ).toBe(false);
  });

  it('puts that sentence under the heading that already says what the step knew', () => {
    const block = blockOf(withTheRun, CLAUDE_STEP);
    expect(block, 'the step that carries the record has to be on the screen at all').not.toBe('');
    expect(
      block.indexOf('data-step-memory') >= 0 && block.indexOf(SENTENCE) >= 0,
      'what a step knew is one question with one answer on the screen (invariant 13). A second ' +
        'region somewhere else on the card would make a person read two places to learn one ' +
        'thing. The step drew: ' +
        block,
    ).toBe(true);
  });

  it('says nothing at all about a step whose record nobody kept', () => {
    const block = blockOf(withTheRun, CHECK_STEP);
    expect(block, 'the second step has to be on the screen too').not.toBe('');
    expect(
      block.includes('skills from'),
      'a step without that record is every step of every run written before 2026-09, every step ' +
        'of Codex and every tile that only runs the checks. Crediting the folder with skills ' +
        'none of them announced is worse than silence. The step drew: ' +
        block,
    ).toBe(false);
    expect(
      block.includes(ANY_OF_IT),
      'and not a word of that sentence may stand next to a step that never said anything',
    ).toBe(false);
  });

  it('says nothing about a step that read only what the run itself put there', () => {
    const block = blockOf(withTheRun, OWN_STEP);
    expect(block, 'the third step has to be on the screen too').not.toBe('');
    expect(
      block.indexOf('data-step-memory') >= 0,
      'the fixture is wrong if this step drew no such region at all — then its silence proves ' +
        'nothing about the sentence. The step drew: ' +
        block,
    ).toBe(true);
    expect(
      block.includes(ANY_OF_IT),
      'this step announced a skill, a plugin and a memory folder, and every one of them is a ' +
        'thing the run itself handed it. A sentence here would fire on nearly every step of ' +
        'every run — and then the one step that really did read a folder nobody handed it ' +
        'reads exactly like all the others. The step drew: ' +
        block,
    ).toBe(false);
  });
});
