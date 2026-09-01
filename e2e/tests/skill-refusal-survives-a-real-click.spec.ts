/* AC-1 dla T-85: odmowa o umiejętności, której krok nie dostał, dochodzi na ekran PO PRAWDZIWYM
 * KLIKNIĘCIU — nie po wywołaniu callbacku ręcznie z testu.
 *
 * PO CO TO ISTNIEJE OBOK KRYTERIUM T-79. `src/sections/run/skills-refusal-is-visible.test.tsx`
 * woła `launchRun`, a potem SAM wywołuje przechwycone `onSaid`. Dowodzi więc, że kanał działa,
 * kiedy się go zawoła — nie że kontrolka startu go woła. Implementacja, w której `go()` nigdy nie
 * sięga po `onSaid` po odrzuconym starcie, przechodzi tamto kryterium bez jednej zmiany. To jest
 * dokładnie klasa z niezmiennika 29: kryterium zielone, funkcja martwa.
 *
 * DLACZEGO PRZEGLĄDARKA, A NIE `renderToStaticMarkup` — zmierzone przez pisarza T-79 w rundzie
 * naprawczej, nie zgadnięte. `go()` czyta listę workflow, którą wypełnia `useEffect`, a render
 * statyczny efektów nie uruchamia: pod nim nie ma czego uruchomić, przycisk jest wygaszony,
 * a kliknięcie poszłoby w inną gałąź i skończyło się innym zdaniem. DOM-u obok nie ma —
 * `vitest` biegnie w środowisku `node`, a `jsdom`, `happy-dom`, `@testing-library`
 * i `react-test-renderer` nie leżą w `node_modules`. Rozstrzygnięcie właściciela z 2026-08-22:
 * `e2e/`, nie nowa zależność; tę rolę AGENTS.md §29 przypisuje temu harnessowi wprost.
 *
 * CZEGO TEN PLIK NIE DOWODZI, i to jest granica, nie kompromis do ukrycia. Rust jest tu atrapą
 * (`../harness.ts`): to, CZY umiejętność kroku istnieje w bibliotece, rozstrzyga się po TAMTEJ
 * stronie i sądzą to kryteria rustowe T-79. Ten plik nie udaje więc, że wybór agenta w oknie
 * zmienia tamten wyrok — dwa przebiegi niżej różnią się tym, co ODPOWIADA granica, i jest to
 * napisane wprost zamiast zainscenizowane (niezmiennik 20). Mierzona jest bliższa połowa drogi
 * i tylko ona: czy kliknięcie naprawdę dochodzi do granicy i czy jej odpowiedź dochodzi
 * człowiekowi przed oczy.
 *
 * ZDANIA NIE MA TU JAKO LITERAŁU i to jest połowa jego wartości — ta sama zasada, co
 * w `src-tauri/tests/it/skills_missing_stops_the_run.rs` i w kryterium T-79. Szablon czytamy
 * z atrybutu `#[error(…)]` przy `skills::Missing` w tym samym biegu testu: druga kopia jednego
 * zdania jest zawsze tą nieaktualną (niezmiennik 23), a przepisana ręcznie przechodziłaby też
 * wtedy, gdy ekran pokazuje zdanie, którego bieg nigdy nie wypowiada. Kontrola przeciw pustemu
 * porównaniu stoi w pierwszym `it`: parser, który cicho nic nie dopasował, dałby puste napisy,
 * a `includes('')` przechodzi na wszystkim.
 *
 * PARSER SZABLONU JEST TU PRZEPISANY, NIE ZAIMPORTOWANY, i to jest wybór z podanym powodem.
 * Jedyna druga kopia stoi w `src/sections/run/skills-refusal-is-visible.test.tsx`, czyli w pliku,
 * który przy imporcie odpala CAŁE swoje ciało modułu — atrapy `vi.mock`, zapis do magazynu
 * zakresów i dwa renderowania ekranu. Import wciągnąłby tamtą scenę do tej. Wspólny dom dla tych
 * dwudziestu linii jest do zrobienia i jest zapisany jako dług, a nie przemilczany; nie mieści
 * się w bloku OWNS tego zadania.
 *
 * TEN PLIK JEST ZIELONY OD PIERWSZEJ MINUTY i to nie jest wada — jest odziedziczoną zielenią.
 * Drogę zbudowało T-79 (`index.tsx` podaje kontrolce startu `sayWhatDidNotStart`, a ta wkłada
 * zdanie do strumienia); T-85 nie zmienia zachowania produktu, tylko dokłada brakujący dowód.
 * Dlatego wartość tego pliku mierzy się MUTACJĄ, nie kolorem, i mutacja została wykonana
 * 2026-08-23. Zdjęte `setSaid(…)` z `go()` w `start.tsx` — czyli dokładnie ta implementacja,
 * którą TASK.md nazywa przechodzącą kryterium T-79 „bez jednej zmiany":
 *
 *   src/sections/run/skills-refusal-is-visible.test.tsx   3 passed   (ślepy na to)
 *   ten plik                                              1 failed   (na asercji, w oknie)
 *
 * Padło tu i wyłącznie tu, na treści ekranu, z wypisanym zrzutem całego okna. Kontrola przeciw
 * ekranowi mówiącemu to zawsze (trzeci `it`) i tak została wtedy zielona, więc czerwień wskazała
 * jedną brakującą drogę, a nie „coś się zepsuło". Po przywróceniu linii: 3 passed.
 *
 * Plik Rusta czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { LEAD_LABEL } from '../../src/sections/run/lead';
import { TASK_LABEL } from '../../src/sections/run/start';
import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Korzeń repo: ten plik leży w `e2e/tests/`, więc dwa katalogi wyżej. */
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SKILLS = resolve(ROOT, 'src-tauri/src/skills/mod.rs');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ciało atrybutu `#[error(…)]` stojącego bezpośrednio przed tą deklaracją. */
function errorAttributeBefore(source: string, declaration: string): string {
  const at = source.indexOf(declaration);
  if (at < 0) return '';
  const head = source.slice(0, at);
  const opens = head.lastIndexOf('#[error(');
  const closes = head.lastIndexOf(')]');
  if (opens < 0 || closes < opens) return '';
  return head.slice(opens + '#[error('.length, closes);
}

/**
 * Napis z takiego atrybutu, złożony tak, jak złoży go kompilator.
 *
 * Dwie rzeczy do zdjęcia i obie zmieniają treść: `\` na końcu linii skleja ją z następną razem
 * z jej wcięciem, a `\"` w środku jest cudzysłowem, który człowiek naprawdę zobaczy.
 */
function rustText(attribute: string): string {
  const joined = attribute.replace(/\\\r?\n\s*/g, '').trim();
  const quoted = /^"((?:[^"\\]|\\.)*)"/.exec(joined);
  return (quoted?.[1] ?? '').replace(/\\"/g, '"');
}

const TEMPLATE = rustText(errorAttributeBefore(fileText(SKILLS), 'pub struct Missing'));
const WHY = rustText(errorAttributeBefore(fileText(SKILLS), 'NotInTheLibrary,'));

/** Nazwa kroku — ta z kafelka, bo to jej szuka człowiek na płótnie. */
const STEP = 'Only step';

/**
 * Umiejętność, której nie ma w bibliotece.
 *
 * Nazwa jest CELOWO nie do pomylenia z niczym innym na ekranie: kontrola niżej twierdzi, że po
 * przebiegu bez odmowy tego napisu NIE MA nigdzie w dokumencie, a zwykłe słowo dałoby tam
 * przypadkowe trafienie z cudzego wiersza i zamieniło kontrolę w zgadywankę.
 */
const SKILL = 'nowhere-in-your-library';

/** Zdanie, którym bieg odmawia — złożone z JEGO szablonu, nie napisane tutaj. */
const REFUSAL = TEMPLATE.replace('{step}', STEP).replace('{skill}', SKILL).replace('{why}', WHY);

/** Zakres, w którym pracujemy. Bez niego start odmawia zdaniem o folderze, a nie o skillu. */
const FOLDER = '/Users/somebody/Projects/loadout-skill-refusal-e2e';
const WORKSPACE = { id: FOLDER, name: 'Skill refusal fixture', folder: FOLDER };

/** Workflow z jednym krokiem — tym, który nosi nazwę stojącą w odmowie. */
const WORKFLOW = {
  path: 'ship.json',
  workflow: {
    format: 1,
    id: 'wf-skill-refusal',
    name: 'Ship it',
    steps: [{ id: 's_only', name: STEP }],
    links: [],
  },
};

/** Agent, którego umiejętności nie ma w bibliotece — ten, którego człowiek wskazuje w scenie. */
const WITHOUT_IT = { id: 'agent-without-the-skill', name: 'Piper', skills: [SKILL] };

/** Agent, który ma wszystko, czego chce jego krok. Stoi tu dla drugiego przebiegu. */
const WITH_EVERYTHING = { id: 'agent-with-everything', name: 'Quill', skills: [] };

/** Ekran pracy jest pierwszą sekcją okna (`src/ui/shell/section-store.ts`), więc nikt nie klika. */
const WORK = 'main[data-section="run"]';

/** Wybór lidera. Etykietę CZYTAMY z produkcji, żeby zmiana brzmienia nie minęła się z testem. */
const LEAD = `select[aria-label="${LEAD_LABEL}"]`;

/** Jedyna kontrolka tego ekranu, która zaczyna bieg (`src/sections/run/start.tsx`). */
const RUN = 'button[data-workflow-run="manual"]';

/**
 * Pole „co ma zbudować ten bieg" — sąsiad przycisku w tej samej grupie paska.
 *
 * Stoi tu jako ŚWIADEK, że kontrolki startu są na ekranie, i to dwa razy: przed kliknięciem
 * (scena jest prawdziwym ekranem pracy, a nie stroną w połowie wczytaną, na której klik i tak
 * nic by nie zrobił) i po nim (zdanie DOŁĄCZYŁO do ekranu, zamiast go zastąpić — ekran, który
 * po odmowie gubi to, czym się startuje, zostawia człowieka z powodem i bez drugiej próby).
 *
 * Napis CZYTAMY z produkcji, nie przepisujemy — po to jest wyeksportowany i mówi to jego własny
 * komentarz. Przepisany z palca byłby zielony także wtedy, gdyby kontrolka i test mówiły o dwóch
 * różnych rzeczach; wtedy „kontrolki startu stoją na ekranie" jest zdaniem o teście.
 */
const TASK = `[aria-label="${TASK_LABEL}"]`;

/** Ile czekamy na pierwsze pojawienie się elementu, który ma przyjść po zdarzeniu. */
const APPEARS = 4_000;

/** Ile czekamy, aż React dorysuje skutek kliknięcia. Render, nie sieć. */
const SETTLE = 500;

/**
 * Scena jednej karty: zakres, jeden workflow z krokiem, biblioteka agentów i jedna odpowiedź
 * granicy na `run_workflow`.
 *
 * Odpowiedzi list podane są po kilka razy: kolejka jest zużywalna (`shift()`), a karta, która
 * przeczytałaby katalog dwa razy, dostałaby za drugim razem domyślne `[]` i zgasiłaby sobie
 * ekran. Kolejka `run_workflow` ma DOKŁADNIE jedną pozycję — jedno kliknięcie to jedno przejście
 * granicy i właśnie to liczy asercja niżej.
 */
function scene(
  answer: TauriReply,
  agents: readonly unknown[],
): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: [{ value: [WORKSPACE] }, { value: [WORKSPACE] }],
    list_workflows: [{ value: [WORKFLOW] }, { value: [WORKFLOW] }],
    list_agents: [{ value: agents }, { value: agents }],
    run_workflow: [answer],
  };
}

/** Otwiera aplikację i czeka, aż ekran pracy naprawdę ma co uruchomić. */
async function openWork(answer: TauriReply, agents: readonly unknown[]): Promise<RunningApp> {
  const app = await openApp({ replies: scene(answer, agents) });
  await app.page.locator(WORK).waitFor({ state: 'attached', timeout: APPEARS });
  await app.page
    .locator(RUN)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  return app;
}

/** Czeka na skutek kliknięcia, nie przesądzając, w którym regionie ekranu ma stanąć. */
async function afterTheClick(app: RunningApp): Promise<string> {
  await app.page
    .getByText(SKILL, { exact: false })
    .first()
    .waitFor({ state: 'visible', timeout: APPEARS })
    .catch(() => undefined);
  await app.page.waitForTimeout(SETTLE);
  return app.page.locator('body').innerText();
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka pierwszy `it` płaci cały rozruch pod swoim
 * limitem. Ta sama para haków stoi w `copy-diagnostics-is-real.spec.ts`. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a real click on Run shows the refusal about the skill the step never got', () => {
  it('runs on the wording the refused run really carries', () => {
    expect(
      TEMPLATE,
      'nothing was read out of the refusal wording in src-tauri/src/skills/mod.rs, so every ' +
        'comparison below would run against an empty string and pass on nothing. Either that ' +
        'file moved, or the refusal stopped carrying the sentence it is made of.',
    ).not.toBe('');
    expect(
      TEMPLATE.includes('{step}') && TEMPLATE.includes('{skill}'),
      'the wording read out of the refusal names neither the step nor the skill, so what this ' +
        'file hands the boundary could not prove anything about either name. It reads: ' +
        TEMPLATE,
    ).toBe(true);
    expect(
      WHY,
      'nothing was read out of the reason given for a name the library never saw, so the ' +
        'sentence would carry an empty clause exactly where the cause belongs.',
    ).not.toBe('');
    expect(
      REFUSAL.includes(SKILL) && REFUSAL.includes(STEP),
      'the sentence has to name both the skill and the step: without the skill a refusal turns ' +
        'one tick box into a search through a list, and without the step a person does not know ' +
        'which tile to open. It says: ' +
        REFUSAL,
    ).toBe(true);
  });

  it('puts that sentence on the screen after the person presses Run', async () => {
    const app = await openWork({ error: REFUSAL }, [WITHOUT_IT, WITH_EVERYTHING]);
    try {
      /* Człowiek wskazuje agenta. Wybór jedzie do okna, a nie do wyroku o umiejętności — ten
       * zapada po drugiej stronie granicy (patrz nagłówek). Stoi tu, bo to jest ekran, na którym
       * człowiek naprawdę jest, i bo bez tej pozycji na liście scena byłaby pustym ekranem. */
      const lead = app.page.locator(LEAD);
      expect(
        await lead.locator(`option[value="${WITHOUT_IT.id}"]`).count(),
        'the work screen offers no agent to pick, so the whole scene below would be a click on ' +
          'a screen that never finished loading.',
      ).toBe(1);
      await lead.selectOption(WITHOUT_IT.id);

      expect(
        await app.page.locator(RUN).count(),
        'the work screen renders no control that starts a run at all, so there is nothing here ' +
          'for a person to press.',
      ).toBe(1);
      expect(
        await app.page.locator(TASK).count(),
        'the controls a run starts from are not on this screen, so the click below would land ' +
          'on a page that never finished mounting rather than on the product.',
      ).toBe(1);
      expect(
        await app.page.locator(RUN).isEnabled(),
        'the only control that starts a run is dimmed on a screen that has a workspace and a ' +
          'workflow with a step in it, so the refusal below could never be reached by pressing ' +
          'anything.',
      ).toBe(true);

      // ── KONTROLA PRZECIW EKRANOWI, KTÓRY MÓWI TO ZAWSZE ────────────────────────────────
      expect(
        await app.page.locator('body').innerText(),
        'the sentence stood on the screen before anything was started, so nothing below could ' +
          'tell a screen that answers a refused run from one that says it always.',
      ).not.toContain(SKILL);

      await app.page.locator(RUN).click();
      const screen = await afterTheClick(app);

      expect(
        (await app.calls()).filter((call) => call.cmd === 'run_workflow').length,
        'pressing the only control that starts a run sent nothing across the boundary. Whatever ' +
          'the screen says next, it did not come from a run that was refused — and a control ' +
          'that reaches nothing is a control without a handler (invariant 16).',
      ).toBe(1);

      expect(
        screen,
        'the run was refused because a skill it needed could not reach the step, and the ' +
          'sentence naming that skill and that step is nowhere on the screen. This is the whole ' +
          'request: a person reads a run that never started and no reason for it — and "the ' +
          'agent was never given that skill" looks from outside exactly like "the model did not ' +
          'reach for it". Nothing here calls a callback by hand: the only input was a click and ' +
          'the only output is what the window shows. The sentence that had to be there: ' +
          REFUSAL,
      ).toContain(REFUSAL);

      expect(
        await app.page.locator(TASK).count(),
        'the sentence arrived and the controls a run starts from left with it. A refusal is ' +
          'meant to join the screen, not replace it: a person who reads why nothing started ' +
          'and has nothing left to press has been told the reason and denied the retry.',
      ).toBe(1);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('says nothing of the kind when the run is not refused', async () => {
    /* Ta sama czynność, ta sama kontrolka, jedna rzecz inna: granica przyjmuje bieg zamiast go
     * odrzucić — tak, jak przyjmuje go dla kroku, którego umiejętności są w bibliotece. Bez tego
     * przebiegu „zdanie jest na ekranie" przechodziłoby też na ekranie, który pokazuje odmowę
     * przy każdym starcie, czyli mówi ją także o biegach, które ruszyły. */
    const app = await openWork({ value: null }, [WITH_EVERYTHING, WITHOUT_IT]);
    try {
      await app.page.locator(LEAD).selectOption(WITH_EVERYTHING.id);
      await app.page.locator(RUN).click();
      const screen = await afterTheClick(app);

      expect(
        (await app.calls()).filter((call) => call.cmd === 'run_workflow').length,
        'this pass has to press the same control and really start something, or "no refusal on ' +
          'the screen" would be a statement about a click that never happened.',
      ).toBe(1);

      expect(
        screen,
        'the run was not refused and the screen still names a skill nothing could deliver. A ' +
          'screen that says it either way tells a person nothing at all, and it makes the pass ' +
          'above true without a single line of the path it claims to measure.',
      ).not.toContain(SKILL);
    } finally {
      await app.close();
    }
  }, 90_000);
});
