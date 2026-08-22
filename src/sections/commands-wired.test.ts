/* AC-2 dla T-27: każda krawędź sekcji naprawdę woła komendę, po nazwie z jednej listy.
 *
 * `@tauri-apps/api/core` jest podmieniony atrapą — żadnego żywego Tauri, żadnej przeglądarki.
 * Kryterium, które ich wymaga, nie umie być czerwone z właściwego powodu: `Failed to launch`
 * stoi na liście `NOT_A_REAL_RED` w `harness/gate.py`, i to jest dokładnie ten powód, dla
 * którego szew między warstwami przeżył dwadzieścia sześć zielonych zadań bez ani jednego
 * dowodu.
 *
 * DLACZEGO KAŻDA FUNKCJA JEST WYKONYWANA, A NIE OGLĄDANA.
 * Słaba wersja tego kryterium to grep po `invoke(` w źródłach. Przechodzi na krawędzi, która
 * woła `invoke` w martwej gałęzi, i na takiej, która skleja nazwę komendy ze zmiennej — czyli
 * na dwóch defektach, których nikt nigdy nie zobaczy, dopóki nie kliknie. Rozróżnia je
 * wykonanie: każda funkcja jest wołana naprawdę, nazwa komendy jest odczytana z atrapy, a to,
 * co funkcja dostała, ma się znaleźć w tym, co pojechało do Rusta.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI. Początkowo krawędzie rzucały `not implemented`, więc każdy
 * test, który tylko LICZY wystąpienia `invoke` w plikach, przechodził na nich bez
 * zmiany ani jednej linii. Stąd asercja jawna: żadna z nich nie ma prawa tak odmówić.
 *
 * TABELA MUSI POKRYWAĆ CAŁY EKSPORT. Wiersze niżej nie są listą przykładów — pierwszy test
 * porównuje je z tym, co moduły faktycznie eksportują, więc funkcja dopisana jutro bez wiersza
 * tutaj jest czerwona. Ręczna lista przykładów milczy dokładnie o tej funkcji, o której autor
 * zapomniał.
 */
import { readFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import * as workspaces from '../state/workspaces-io';
import * as agents from './agents/io';
import { ipcSource, windowSideArguments } from './ipc-signature';
import * as memory from './memory/io';
import * as run from './run/io';
import * as skills from './skills/io';
import * as triggers from './triggers/io';
import * as workflows from './workflows/io';

import type { Agent } from '../state/agents';
import type { Authored, Import, Landing } from '../state/skills';
import type { WorkflowFile } from '../state/workflows';

/* Atrapa jest podniesiona razem z `vi.mock`, żeby moduły sekcji dostały JĄ, a nie prawdziwy
 * transport. Rozwiązuje się zawsze i zawsze tą samą wartością: ta droga nie mierzy odpowiedzi
 * Rusta, tylko to, co w jego stronę pojechało. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) => Promise.resolve(undefined)),
}));

/* `Channel` jest w atrapie, bo krawędź Biegu zakłada go w oknie i podaje `run_workflow`
 * argumentem. Atrapa bez niego mierzyłaby brak atrapy zamiast braku argumentu — dokładnie ten
 * błąd trzymał wiersz Startu poza tym plikiem przez całe T-30. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

/** Ta sama lista, którą po drugiej stronie granicy czyta `ipc_commands_registered.rs`. */
const GOLDEN = new URL('../../src-tauri/commands.golden.txt', import.meta.url);

const known = new Set(
  readFileSync(GOLDEN, 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

/** `ipc.rs` w całości. Czytany raz: to jedyne miejsce, w którym stoją nazwy argumentów. */
const rust = ipcSource();

/** Gdzie leży dana krawędź. W komunikacie ma stać prawdziwa ścieżka, nie zgadnięta. */
const WHERE_PATH: Readonly<Record<string, string>> = {
  agents: 'src/sections/agents/io.ts',
  memory: 'src/sections/memory/io.ts',
  run: 'src/sections/run/io.ts',
  skills: 'src/sections/skills/io.ts',
  triggers: 'src/sections/triggers/io.ts',
  workflows: 'src/sections/workflows/io.ts',
  workspaces: 'src/state/workspaces-io.ts',
};

const AGENT_ID = '0198a1f2-3b4c-7d5e-8f60-112233445566';
const FILE_NAME = 'ship-a-feature.json';
const IMAGE = { mime: 'image/png' as const, base64: 'iVBORw0KGgoAAAANSUhEUg==' };

const AGENT: Agent = {
  schema: 1,
  id: AGENT_ID,
  name: 'Forge',
  summary: 'Writes code',
  color: 'clay',
  instructions: 'Write the smallest change that makes the checks pass.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/build.md',
};

const WORKFLOW: WorkflowFile = {
  format: 1,
  id: '0198a1f2-3b4c-7d5e-8f60-99887766aabb',
  name: 'Ship a feature',
  steps: [
    { kind: 'checkpoint', id: 'look', name: 'Does the plan look right?', at: { x: 0, y: 0 } },
  ],
  links: [],
};

const SKILL: Import = {
  name: 'pdf',
  summary: 'Pulls tables out of PDF files.',
  reviewed: { body: '---\nname: pdf\n---\n', findings: [], verdict: 'clean' },
  scripts: 1,
  fromTheInternet: true,
};

/* 2026-08-19 (T-44) — WYBÓR MIEJSCA I FOLDER, którymi wołane są trzy krawędzie umiejętności.
 *
 * `LANDING` jest typowane z rozmysłu: dzień, w którym ta unia zmieni kształt, ma zaczerwienić
 * ten plik na kompilacji, a nie po cichu wysłać napis, którego Rust już nie rozpoznaje.
 * „ten projekt", a nie „wszędzie", bo to jest wartość, której ta droga nigdy wcześniej nie
 * niosła — wiersz wołany domyślną przechodziłby także wtedy, gdyby wybór był ignorowany.
 *
 * `FOLDER` jest ścieżką BEZWZGLĘDNĄ i nigdy nie dotyka dysku: granica jest atrapą, więc jedyne,
 * co się z nią dzieje, to porównanie z tym, co pojechało. Napis, nie `null` — `insides()` niżej
 * odrzuca `null`, więc wiersz wołany `null`em nie sprawdzałby, czy folder w ogóle dojechał. */
const LANDING: Landing = 'this-project';
const FOLDER = '/Users/somebody/Projects/Loadout';

/** Trzy odpowiedzi z formularza „write one yourself" [T5 §8.3]. */
const AUTHORED: Authored = {
  name: 'Review pull requests',
  whenToUse: 'Use this when somebody asks for a second look at a pull request.',
  whatToDo: 'Read the change first, then say in one paragraph what to fix.',
};

const LINEAR_KEY = 'lin_api_1234567890123456789012345678901234567890';
const TRIGGER_DRAFT: triggers.TriggerDraft = {
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  workspace: FOLDER,
  pollEveryMinutes: 5,
  apiKey: LINEAR_KEY,
};
const TRIGGER_EXPECTED: triggers.TriggerSnapshot = {
  slug: 'linear-0198ca82-ded0-7000-8000-000000000074',
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'analysis.json',
  workspace: FOLDER,
  enabled: true,
  pollEveryMinutes: 5,
  hasApiKey: true,
};

/** Jedna krawędź: skąd, co, którą komendę woła, z czym ją wołamy. */
interface Wire {
  readonly where: string;
  readonly what: string;
  readonly command: string;
  /** To, co funkcja dostaje. Każda wartość stąd ma się znaleźć w tym, co pojechało dalej. */
  readonly given: readonly unknown[];
  readonly call: () => unknown;
}

const WIRES: readonly Wire[] = [
  { where: 'agents', what: 'list', command: 'list_agents', given: [], call: () => agents.list() },
  { where: 'agents', what: 'newId', command: 'new_id', given: [], call: () => agents.newId() },
  {
    where: 'agents',
    what: 'save',
    command: 'save_agent',
    given: [AGENT],
    call: () => agents.save(AGENT),
  },
  {
    where: 'agents',
    what: 'remove',
    command: 'delete_agent',
    given: [AGENT_ID],
    call: () => agents.remove(AGENT_ID),
  },
  {
    where: 'workflows',
    what: 'list',
    command: 'list_workflows',
    given: [],
    call: () => workflows.list(),
  },
  {
    where: 'workflows',
    what: 'newId',
    command: 'new_id',
    given: [],
    call: () => workflows.newId(),
  },
  {
    where: 'workflows',
    what: 'load',
    command: 'load_workflow',
    given: [FILE_NAME],
    call: () => workflows.load(FILE_NAME),
  },
  {
    where: 'workflows',
    what: 'write',
    command: 'save_workflow',
    given: [FILE_NAME, WORKFLOW],
    call: () => workflows.write(FILE_NAME, WORKFLOW),
  },
  {
    where: 'workflows',
    what: 'remove',
    command: 'delete_workflow',
    given: [FILE_NAME],
    call: () => workflows.remove(FILE_NAME),
  },
  {
    where: 'workflows',
    what: 'check',
    command: 'check_workflow',
    given: [WORKFLOW],
    call: () => workflows.check(WORKFLOW),
  },
  {
    where: 'skills',
    what: 'readLink',
    command: 'review_skill',
    given: ['https://example.invalid/skills/pdf/SKILL.md'],
    call: () => skills.readLink('https://example.invalid/skills/pdf/SKILL.md'),
  },
  /* 2026-08-19 (T-44) — TE TRZY KRAWĘDZIE NIOSĄ TERAZ WYBÓR MIEJSCA I FOLDER. Nic tu nie
   * usunięto: zmieniło się `given` i `call` w trzech istniejących wierszach, bo `install_skill`,
   * `list_skills` i `delete_skill` przyjmują po tamtej stronie granicy o dwa (i o jeden)
   * argumenty więcej. Wiersz, który wołałby je po staremu, wysyłałby klucze, których Rust już
   * nie ma — a Tauri dopasowuje argumenty PO NAZWIE, więc byłoby to wywołanie ODRZUCONE, nie
   * mniejsze. */
  {
    where: 'skills',
    what: 'install',
    command: 'install_skill',
    given: [SKILL, LANDING, FOLDER],
    call: () => skills.install(SKILL, LANDING, FOLDER),
  },
  {
    where: 'memory',
    what: 'putToUse',
    command: 'put_note_to_use',
    given: [{ id: 'note-to-use' }],
    call: () => memory.putToUse({ id: 'note-to-use' }),
  },
  {
    where: 'memory',
    what: 'stopUsing',
    command: 'stop_using_note',
    given: [{ id: 'note-to-drop' }],
    call: () => memory.stopUsing({ id: 'note-to-drop' }),
  },
  /* 2026-08-18 (T-38 AC-5/AC-6) — DWIE NOWE KRAWĘDZIE ODCZYTU, DOPISANE, NIC NIE USUNIĘTE.
   * Ten plik złapał je, zanim zdążyły wjechać niezauważone, i to jest dokładnie jego zadanie:
   * `listNotes` i `listSkills` powstały po to, żeby Pamięć i Umiejętności czytały z dysku —
   * bez tych dwóch wierszy byłyby funkcjami, których nikt nigdy nie zobaczył docierających
   * do Rusta, a zapisana umiejętność ginęła po restarcie (niezmiennik 4). */
  {
    where: 'memory',
    what: 'listNotes',
    command: 'list_notes',
    given: [],
    call: () => memory.listNotes(),
  },
  {
    where: 'skills',
    what: 'listSkills',
    command: 'list_skills',
    given: [FOLDER],
    call: () => skills.listSkills(FOLDER),
  },
  /* 2026-08-18 (fala pieciu sekcji) — DWIE KOLEJNE KRAWEDZIE, znowu dopisane, znowu nic
   * nie usuniete. Ten plik zlapal je w tej samej godzinie, w ktorej powstaly, i to jest jego
   * cala wartosc: `remove` w Umiejetnosciach i `listHandoffs` w Pamieci sa krawedziami do
   * komend, ktore powstaly w tej samej fali po drugiej stronie granicy, a krawedz bez wiersza
   * tutaj jest funkcja, ktorej nikt nigdy nie zobaczyl docierajacej do Rusta.
   *
   * `remove` w Umiejetnosciach wazy wiecej niz zwykla krawedz: ta sekcja PISZE do zywej
   * konfiguracji narzedzi czlowieka (`skills::mod::DESTINATION_DIRS`), wiec dopoki nie bylo
   * usuwania, jedno bledne „Add" zostawalo tam na stale. */
  {
    where: 'skills',
    what: 'remove',
    command: 'delete_skill',
    given: ['a-skill-to-take-away', LANDING, FOLDER],
    call: () => skills.remove('a-skill-to-take-away', LANDING, FOLDER),
  },
  {
    where: 'memory',
    what: 'listHandoffs',
    command: 'list_handoffs',
    given: [],
    call: () => memory.listHandoffs(),
  },
  /* 2026-08-19 (T-42) — DRUGA DROGA WEJŚCIA DO UMIEJĘTNOŚCI, dopisana, nic nie usunięte.
   * `authorSkill` jest krawędzią do komendy, która przyjmuje TREŚĆ umiejętności, a nie adres —
   * czyli do jedynej rzeczy, której `commands.golden.txt` nie miał, choć pusty ekran obiecywał
   * ją zdaniem „Paste a link, or write one yourself". Bez wiersza tutaj byłaby funkcją, której
   * nikt nigdy nie zobaczył docierającej do Rusta. */
  {
    where: 'skills',
    what: 'authorSkill',
    command: 'author_skill',
    given: [AUTHORED],
    call: () => skills.authorSkill(AUTHORED),
  },
  /* 2026-08-19 (T-43) — TRZECIA DROGA WEJŚCIA DO UMIEJĘTNOŚCI, dopisana, nic nie usunięte.
   * Adres i formularz przyjmują wyłącznie GOTOWY tekst; te dwie krawędzie są jedyną drogą,
   * którą zdanie człowieka zamienia się w tekst od modelu, i jedyną, która potem tego modela
   * zatrzymuje. Bez wiersza tutaj `stopWriting` byłaby funkcją, której nikt nigdy nie zobaczył
   * docierającej do Rusta — a to jest ta jedna krawędź w tej sekcji, której cisza kosztuje
   * pieniądze: proces vendora pisze dalej i dalej pali limit dostawcy (niezmienniki 6 i 10). */
  {
    where: 'skills',
    what: 'askAnAgent',
    command: 'draft_skill',
    given: ['Something that reads a change and says what to fix first.', AGENT_ID],
    call: () =>
      skills.askAnAgent('Something that reads a change and says what to fix first.', AGENT_ID),
  },
  {
    where: 'skills',
    what: 'stopWriting',
    command: 'stop_draft',
    given: [],
    call: () => skills.stopWriting(),
  },
  /* 2026-08-18 — SIEDEM KRAWĘDZI BIEGU I ZAKRESU, i to jest domknięcie luki, którą ten plik
   * sam nazywał: `EDGES` obejmowało cztery sekcje, a Bieg — jedyna sekcja, która URUCHAMIA
   * agentów — nie miał tu ani jednego wiersza. Cztery jego krawędzie (`start`, `stop`,
   * `continueRun`, `sayToAgent`) wołają cztery komendy, których nazwy i nazwy argumentów nie
   * były po tej stronie granicy sądzone niczym. Trzy z nich naprawdę pojechały zepsute:
   * `run_workflow` bez `lines` (Start odbijał się przy każdym kliknięciu, 2026-08-17),
   * `continue_run` bez `answer`, i pisanie do agenta, którego nie było w ogóle.
   *
   * Zakresy dochodzą tą samą falą: `src/state/workspaces-io.ts` powstał 2026-08-18 i jest
   * krawędzią jak każda inna — to, że leży w `state/`, a nie w `sections/`, zmienia katalog,
   * nie ryzyko. */
  {
    where: 'run',
    what: 'start',
    command: 'run_workflow',
    given: [FILE_NAME, 3],
    call: () => run.start(FILE_NAME, 3, { name: 'Ship a feature', steps: [] }, null),
  },
  { where: 'run', what: 'stop', command: 'stop_run', given: [], call: () => run.stop() },
  {
    where: 'run',
    what: 'continueRun',
    command: 'continue_run',
    given: ['ship it'],
    call: () => run.continueRun('ship it'),
  },
  {
    where: 'run',
    what: 'sayToAgent',
    command: 'say_to_agent',
    given: ['also add a dark mode toggle'],
    call: () => run.sayToAgent('also add a dark mode toggle'),
  },
  /* 2026-08-19 — DWIE KRAWĘDZIE ROZMOWY Z AGENTEM WIODĄCYM. Rozstrzygnięcie właściciela: górny
   * wiersz jest rozmową, a sztywny przebieg zaczyna wyłącznie komenda. Rozmowa ma więc własne
   * komendy — i dokładnie te same ryzyka, co Start: `open_chat` bez `lines` byłoby wywołaniem
   * odrzuconym przed wejściem w ciało, a `say_to_orchestrator` bez `folder` rozmawiałoby o innym
   * katalogu niż ten, w którym człowiek stoi. */
  {
    where: 'run',
    what: 'openChat',
    command: 'open_chat',
    given: [],
    call: () => run.openChat(null),
  },
  {
    where: 'run',
    what: 'sayToOrchestrator',
    command: 'say_to_orchestrator',
    given: ['what should the checker look at?', AGENT_ID, IMAGE],
    call: () =>
      run.sayToOrchestrator('what should the checker look at?', null, null, AGENT_ID, [IMAGE]),
  },
  {
    where: 'run',
    what: 'copyDiagnostics',
    command: 'copy_diagnostics',
    given: [FOLDER],
    call: () => run.copyDiagnostics(FOLDER),
  },
  /* 2026-08-20 (T-71) — JEDNA NOWA KRAWĘDŹ BIEGU: koniec rozmowy zamykanej karty. Dopisana,
   * nic nie usunięte i żaden istniejący wiersz nie przepisany — bez niej pierwszy test wyżej
   * jest czerwony, bo `run/io.ts` eksportuje `closeTerminal`, a krawędź bez wiersza jest
   * krawędzią, której nikt nie zobaczył docierającej do Rusta. To nie jest ostrożność na
   * przyszłość: `close_terminal` po tamtej stronie granicy stało otestowane i bez ani jednego
   * wołającego z produkcji, więc każdy terminal zamknięty `×` zostawiał lidera żywego
   * i płacącego (niezmiennik 6).
   *
   * `given` NIESIE IDENTYFIKATOR TERMINALU, bo on JEST tu całym wywołaniem: `close_terminal`
   * bierze jeden argument i to nim rejestr rozmów wybiera, którą kończy. Wiersz wołany bez
   * niego przechodziłby także dla krawędzi, która kończy rozmowę pierwszą z listy — czyli
   * cudzą. Wartością jest folder, bo karta biegu nazywa się folderem po obu stronach granicy
   * (`run/tabs/store.ts`, `endLeadOf`). */
  {
    where: 'run',
    what: 'closeTerminal',
    command: 'close_terminal',
    given: [FOLDER],
    call: () => run.closeTerminal(FOLDER),
  },
  {
    where: 'workspaces',
    what: 'listWorkspaces',
    command: 'list_workspaces',
    given: [],
    call: () => workspaces.listWorkspaces(),
  },
  {
    where: 'workspaces',
    what: 'saveWorkspace',
    command: 'save_workspace',
    given: [{ name: 'Loadout', folder: '/Users/somebody/Projects/Loadout' }],
    call: () =>
      workspaces.saveWorkspace({ name: 'Loadout', folder: '/Users/somebody/Projects/Loadout' }),
  },
  {
    where: 'workspaces',
    what: 'deleteWorkspace',
    command: 'delete_workspace',
    given: [{ id: '/Users/somebody/Projects/Loadout' }],
    call: () => workspaces.deleteWorkspace({ id: '/Users/somebody/Projects/Loadout' }),
  },
  /* 2026-08-20 (T-62) — JEDNA NOWA KRAWĘDŹ BIEGU: `/ask`, jeden agent z jednym zdaniem.
   * Dopisana, nic nie usunięte i żaden istniejący wiersz nie przepisany — mandat tego zadania
   * na ten plik pozwala dokładnie na tyle (TASK.md, „lustro komend"), a wiersz jest tu dlatego,
   * że bez niego pierwszy test wyżej jest czerwony: `run/io.ts` eksportuje `ask`, więc krawędź
   * bez wiersza jest krawędzią, której nikt nie zobaczył docierającej do Rusta.
   *
   * `given` NIE ZAWIERA NAZWY AGENTA, i to nie jest przeoczenie: na drut jedzie identyfikator,
   * bo przeżywa zmianę nazwy i bo `run_agent` szuka nim agenta w bibliotece — nazwa zostaje
   * w oknie, na pasku loadoutu (`run/io.ts`, `interface Asked`). Wiersz wymagający jej po tamtej
   * stronie żądałby klucza, którego Rust nie ma, a klucz, który się nie zgadza, nie daje
   * mniejszego wywołania — daje odrzucone.
   *
   * `howManyAtOnce` jedzie tu z tego samego powodu, z którego jedzie przy Starcie: bieg
   * jednokrokowy bierze miejsce z TEJ SAMEJ puli (niezmiennik 11). Wiersz, który by go nie
   * wysłał, przechodziłby także dla krawędzi wołającej `run_agent` ze stałą `1` po tamtej
   * stronie — czyli dla `/ask`, które omija limiter. */
  {
    where: 'run',
    what: 'ask',
    command: 'run_agent',
    given: [AGENT_ID, 'read the change and say what to fix first', 3],
    call: () =>
      run.ask({ id: AGENT_ID, name: AGENT.name }, 'read the change and say what to fix first', 3),
  },
  /* 2026-08-20 (T-72) — TRZY KRAWĘDZIE RZECZY URUCHOMIONYCH KOMENDĄ, dopisane, nic nie usunięte
   * i żaden istniejący wiersz nie przepisany.
   *
   * Ten plik jest dla nich JEDYNYM sądem po tej stronie granicy i to nie jest przesada: `/start`
   * nie idzie żadną drogą, którą widzi kryterium e2e — tam granica Rusta jest atrapą i odpowiada
   * kształtem — a po tamtej stronie nie ma czego wołać bez okna. Krawędź z podmienionym kluczem
   * dawałaby wywołanie ODRZUCONE, nie mniejsze, i człowiek zobaczyłby dokładnie to samo, co przy
   * Starcie zepsutym 2026-08-17: kafelek, który się nie pojawia, i ani jednego zdania dlaczego.
   *
   * `startProcess` niesie FOLDER, choć po tamtej stronie jest `Option<String>`: wiersz wołany
   * `null`em przechodziłby także dla krawędzi, która folder gubi — `insides()` niżej odrzuca
   * `null`, więc nie byłoby czego zgubić. To jest ta sama pułapka, którą ten plik nazywa przy
   * `FOLDER` wyżej.
   *
   * `listProcesses` jedzie z `null`em i to jest jedyna wartość, jaką ta krawędź w kryterium może
   * ponieść: `opened` jest `pgid` OTWARTEGO panelu, a w teście krawędzi nie ma otwartego niczego.
   * Sam KLUCZ jest tu tym, co się sądzi — bez niego wywołanie byłoby odrzucone. */
  {
    where: 'run',
    what: 'startProcess',
    command: 'start_process',
    given: ['npm run dev', FOLDER],
    call: () => run.startProcess('npm run dev', FOLDER),
  },
  {
    where: 'run',
    what: 'stopProcess',
    command: 'stop_process',
    given: [4213],
    call: () => run.stopProcess(4213),
  },
  {
    where: 'run',
    what: 'listProcesses',
    command: 'list_processes',
    given: [],
    call: () => run.listProcesses(null),
  },
  /* 2026-08-21 (T-65) — TRZY KRAWEDZIE TRIGGEROW, razem z cala zredagowana biblioteka.
   * Listowanie nie bierze argumentow, zapis niesie obie wartosci kontrolki, a odpytanie slug.
   * Kazdy eksport jest tu wykonany, wiec dopisanie przycisku bez drogi do Rusta albo klucza
   * argumentu zapali to samo lustro, ktore pilnuje pozostalych sekcji. */
  {
    where: 'triggers',
    what: 'listTriggers',
    command: 'list_triggers',
    given: [],
    call: () => triggers.listTriggers(),
  },
  {
    where: 'triggers',
    what: 'setTriggerEnabled',
    command: 'set_trigger_enabled',
    given: ['assigned-to-me', false],
    call: () => triggers.setTriggerEnabled('assigned-to-me', false),
  },
  {
    where: 'triggers',
    what: 'checkTrigger',
    command: 'check_trigger',
    given: ['assigned-to-me'],
    call: () => triggers.checkTrigger('assigned-to-me'),
  },
  {
    where: 'triggers',
    what: 'retryTrigger',
    command: 'retry_trigger',
    given: ['assigned-to-me'],
    call: () => triggers.retryTrigger('assigned-to-me'),
  },
  {
    where: 'triggers',
    what: 'createTrigger',
    command: 'create_trigger',
    given: [TRIGGER_DRAFT],
    call: () => triggers.createTrigger(TRIGGER_DRAFT),
  },
  {
    where: 'triggers',
    what: 'updateTrigger',
    command: 'update_trigger',
    given: [TRIGGER_EXPECTED.slug, TRIGGER_EXPECTED, TRIGGER_DRAFT],
    call: () => triggers.updateTrigger(TRIGGER_EXPECTED.slug, TRIGGER_EXPECTED, TRIGGER_DRAFT),
  },
  {
    where: 'triggers',
    what: 'deleteTrigger',
    command: 'delete_trigger',
    given: [TRIGGER_EXPECTED.slug, TRIGGER_EXPECTED],
    call: () => triggers.deleteTrigger(TRIGGER_EXPECTED.slug, TRIGGER_EXPECTED),
  },
  {
    where: 'triggers',
    what: 'testLinearConnection',
    command: 'test_linear_connection',
    given: [LINEAR_KEY],
    call: () => triggers.testLinearConnection(null, LINEAR_KEY),
  },
];

const EDGES: ReadonlyArray<readonly [string, object]> = [
  ['agents', agents],
  ['memory', memory],
  ['run', run],
  ['skills', skills],
  ['triggers', triggers],
  ['workflows', workflows],
  ['workspaces', workspaces],
];

function exportedFunctions(edge: object): string[] {
  return Object.entries(edge)
    .filter(([, value]) => typeof value === 'function')
    .map(([name]) => name)
    .sort();
}

/**
 * Każda wartość prosta w środku, na dowolnym poziomie zagnieżdżenia.
 *
 * Pyta o WARTOŚCI i tylko o nie: wywołanie, które doszło bez tego, co dostało, jest tą samą
 * ciszą, co brak wywołania.
 *
 * 2026-08-18 — POPRAWIONY KOMENTARZ, BO POPRZEDNI BYŁ NIEPRAWDZIWY. Stało tu, że „krawędź wolno
 * napisać jako `{ id }` albo `{ agentId }`, bo Tauri i tak przepisuje nazwy argumentów przy
 * przejściu". Tauri **nie** przepisuje nazw: dopasowuje je PO NAZWIE (z jedną, mechaniczną
 * zamianą `snake_case` na `camelCase`) i deserializuje argumenty PRZED wejściem w ciało komendy.
 * Podmieniony klucz nie daje więc mniejszego wywołania, daje ODRZUCONE — i przez ten jeden
 * nieprawdziwy akapit ta droga przechodziła tu na zielono. Dlatego nazwy są od dziś sądzone
 * osobno, niżej, przeciwko `src-tauri/src/ipc.rs`.
 */
function insides(value: unknown, into: unknown[]): unknown[] {
  if (Array.isArray(value)) {
    for (const item of value as unknown[]) insides(item, into);
  } else if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value as Record<string, unknown>)) insides(item, into);
  } else if (value !== undefined && value !== null) {
    into.push(value);
  }
  return into;
}

describe('the six section edges, the workspace edge and the one list of command names', () => {
  beforeEach(() => {
    invoked.mockClear();
  });

  it('has a row below for every function the seven edges export', () => {
    for (const [where, edge] of EDGES) {
      const exported = exportedFunctions(edge);
      const covered = WIRES.filter((wire) => wire.where === where)
        .map((wire) => wire.what)
        .sort();
      expect(
        covered,
        (WHERE_PATH[where] ?? where) +
          ' exports something this file never runs, or this file names something it does ' +
          'not export. A function with no row here is a function nobody ever watched reach Rust.',
      ).toEqual(exported);
    }
  });

  /* KONTROLA PRZECIW PUSTEMU ZBIOROWI OCZEKIWAŃ. Każda asercja o nazwach argumentów niżej
   * porównuje się z tym, co parser wyciągnie z `ipc.rs`. Parser, który nie znalazł pliku albo
   * nie zrozumiał ani jednego podpisu, oddaje puste listy — a `[] equals []` przechodzi dla
   * KAŻDEJ krawędzi, także dla takiej, która nie wysyła nic. To jest ten sam kształt fałszywej
   * zieleni, przez który ten szew przeżył dwadzieścia sześć zielonych zadań. */
  it('really read the argument names out of ipc.rs', () => {
    expect(
      rust,
      'src-tauri/src/ipc.rs could not be read, so every expected set below would come from ' +
        'nowhere and each comparison would pass on two empty lists.',
    ).not.toBe('');
    const parsed = WIRES.filter(
      (wire) => windowSideArguments(rust, wire.command).length > 0,
    ).length;
    expect(
      parsed,
      'not one command signature was parsed out of ipc.rs, so the argument-name check below is ' +
        'comparing nothing against nothing.',
    ).toBeGreaterThan(WIRES.length / 2);
  });

  it('names only commands that are on commands.golden.txt', () => {
    const strangers = [...new Set(WIRES.map((wire) => wire.command))].filter(
      (command) => !known.has(command),
    );
    expect(
      strangers,
      'these command names are expected below and are not on src-tauri/commands.golden.txt: ' +
        strangers.join(', ') +
        '. The list is the one place where both sides of the seam agree on a name.',
    ).toEqual([]);
  });

  for (const wire of WIRES) {
    it(
      wire.where + '/' + wire.what + ' calls ' + wire.command + ' once, carrying what it got',
      async () => {
        let refusal: unknown = null;
        try {
          await wire.call();
        } catch (error) {
          refusal = error;
        }

        const said = refusal instanceof Error ? refusal.message : String(refusal ?? '');
        expect(
          said.includes('not implemented'),
          (WHERE_PATH[wire.where] ?? wire.where) +
            ' turned down ' +
            wire.what +
            ' with "not implemented". That is the state this file exists to end, and it is why ' +
            'this test runs the edges instead of reading them: ' +
            said,
        ).toBe(false);

        expect(
          invoked.mock.calls.length,
          wire.where +
            '/' +
            wire.what +
            ' has to reach Rust exactly once. Zero is the state this task exists to end; more ' +
            'than one means the same button does the same work twice.',
        ).toBe(1);

        const sent = invoked.mock.calls.at(0);
        if (sent === undefined) {
          throw new Error(wire.where + '/' + wire.what + ' never reached Rust at all');
        }

        const name = sent.at(0);
        expect(
          typeof name === 'string' && known.has(name),
          wire.where +
            '/' +
            wire.what +
            ' asked Rust for ' +
            String(name) +
            ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side is ' +
            'keeping that name alive, and the day it is renamed this call goes quiet.',
        ).toBe(true);
        expect(name, 'and the command it asks for is the one this section is meant to use').toBe(
          wire.command,
        );

        /* NAZWY ARGUMENTÓW, PRZECIWKO `ipc.rs`. To jest ta połowa kontraktu, której ten plik
         * do dziś nie sądził — i jedyna, która naprawdę decyduje, czy kliknięcie coś zrobi.
         * Porównanie idzie w OBIE strony: klucz, którego Rust nie ma, wywala deserializację
         * całego wywołania, a brakujący klucz robi to samo, bo argumenty nie są opcjonalne
         * przez to, że okno ich nie wysłało. */
        const wanted = [...windowSideArguments(rust, wire.command)].sort();
        const keys = Object.keys((sent.at(1) ?? {}) as Record<string, unknown>).sort();
        expect(
          keys,
          wire.where +
            '/' +
            wire.what +
            ' sends ' +
            JSON.stringify(keys) +
            ' to ' +
            wire.command +
            ', which takes ' +
            JSON.stringify(wanted) +
            ' (read out of src-tauri/src/ipc.rs). Tauri matches invoke arguments BY NAME and ' +
            'deserializes them before the command body runs, so a key that does not line up is ' +
            'not a smaller call — it is a rejected one, and the refusal arrives as a raw string ' +
            'nobody sees.',
        ).toEqual(wanted);

        const carried = insides(sent.at(1), []);
        const lost = insides(wire.given, []).filter((value) => !carried.includes(value));
        expect(
          lost,
          wire.where +
            '/' +
            wire.what +
            ' called the right command and left some of what it was given behind: ' +
            JSON.stringify(lost) +
            '. A call that reaches Rust without its values is the same silence as no call at all.',
        ).toEqual([]);
      },
    );
  }
});
