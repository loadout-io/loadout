import type { FormEvent, ReactElement } from 'react';
import { useRef, useState } from 'react';
import { type FilledAgent, whatTheyGot } from '../../ipc/filled-agents';
import { why } from '../../ipc/why';
import type { Reviewed } from '../../state/skills';
import { activeWorkspace } from '../../state/workspaces';
import { Tick } from '../../ui/primitives/tick';
/* Lista znalezisk przyjeżdża z sekcji Umiejętności, bo to jest ta sama lista dla tego samego
 * człowieka — powód stoi w nagłówku tamtego pliku (niezmiennik 23). */
import { Findings } from '../skills/findings';
import {
  COULD_NOT_ASK,
  COULD_NOT_STOP,
  type Comparing,
  IDLE,
  answered,
  askFailed,
  asking,
  refused,
  stopFailed,
  stopped,
} from './comparing';
import * as Disk from './io';
import {
  BECOMES_INSTRUCTIONS,
  DROP_UNREAD,
  OPEN_IT,
  READ_ALREADY,
  READ_IT,
  STOPS_IT,
  compatibilityIn,
  mustBeRead,
  readingSays,
  readySays,
  stillUnread,
} from './skill-review';
import { type InventoryView, SHOW_ALL, hiddenSays, hidesEverything, keptBy } from './shown';
import { A_NEW_CONVERSATION, importedSays, landingSays } from './where-it-lands';

export type Compatibility = 'exact' | 'adjusted' | 'needs_choice' | 'unsupported';
export type SourceKind =
  'claude' | 'codex' | 'agent_skills' | 'rulesync' | 'open_standard' | 'unknown';
/** Lustro `import::ItemKind`: pięć rodzajów rzeczy, które Loadout umie u siebie postawić. */
export type ItemKind = 'agent' | 'skill' | 'connection' | 'workflow' | 'memory';

export interface SourceItem {
  id: string;
  source?: SourceKind;
  /* Rust wysyła rodzaj ZAWSZE (`import::SourceItem::kind` nie jest `Option`), a od 2026-08-29
   * jest ich pięć i każdy ma swoją nazwę. Pole opcjonalne kazało tu trzymać szóstą nazwę
   * („Other") na wypadek, który nie zachodzi — czyli słowo na ekranie bez rzeczy pod spodem. */
  kind: ItemKind;
  path: string;
  name: string;
  summary: string;
}

export interface Mapping {
  itemId: string;
  compatibility: Compatibility;
  message: string;
}

export type ImportSourceRole = 'definition' | 'behavior' | 'dependency';
export type ImportStatus = 'ready' | 'needs_choice' | 'unsupported' | 'missing_dependencies';

export interface ImportSource {
  provider: SourceKind;
  path: string;
  hash: string;
  role: ImportSourceRole;
}

/** Addytywny model T-78. Stare wektory zostają do czasu, aż będą z niego materializowane. */
export interface ImportItem {
  id: string;
  kind: ItemKind;
  sources: ImportSource[];
  target: string | null;
  dependencies: string[];
  status: ImportStatus;
  statusMessage: string;
  generatedHash: string | null;
  /**
   * Co przegląd znalazł w tej umiejętności — lustro `ImportItem::reviewed`
   * (`src-tauri/src/import/mod.rs`), tym samym kształtem, co karta przeglądu przy wklejonym
   * linku (`src/sections/skills/review-card.tsx`).
   *
   * OPCJONALNE PO OBU STRONACH, i to jest jedno pole, nie dwa: Rust ma tu
   * `skip_serializing_if = "Option::is_none"`, więc klucz **nie przyjeżdża wcale** dla pozycji,
   * która nie jest umiejętnością. Brak klucza znaczy „przegląd nie dotyczył tej pozycji"
   * i nigdy „przejrzano, nic nie ma" — te dwa zdania wolno pomylić dokładnie raz.
   */
  reviewed?: Reviewed;
  /**
   * Czy biblioteka ma już wszystkie pliki tej pozycji — lustro `ImportItem::already_here`.
   *
   * OPCJONALNE, bo siedem fikstur w `*.test.tsx` opisuje plan sprzed 2026-09-16 i brak klucza
   * ma tam znaczyć dokładnie to, co znaczył wtedy: „biblioteka tego nie miała". Pytamy więc
   * o `=== true`, nigdy o `!== false`.
   */
  alreadyHere?: boolean;
}

/** Czego agent ma dotknąć, żeby porównać kopie jednej pozycji.
 *
 * Klucze są nazwami parametrów `compare_import_copies` w `src-tauri/src/ipc.rs` — Tauri
 * dopasowuje argumenty PO NAZWIE, a rozjazd objawia się dopiero pod palcem człowieka.
 */
export interface CompareRequest {
  workspace: string;
  item: string;
  agent: string;
}

/** Druga opinia o kopiach jednej pozycji — lustro `import::compare::Comparison`.
 *
 * `compared` niesie ŚCIEŻKI, które agent naprawdę przeczytał, a nie ich liczbę: człowiek
 * ma rozstrzygnąć konkretne dwa pliki, więc odpowiedź, która ich nie nazywa, jest zdaniem
 * o czymś innym. `keep` to jedna z tych ścieżek albo `null`, kiedy z prozy agenta nie
 * wynika żadna — zgadywanie tutaj byłoby radą, której nikt nie udzielił.
 */
export interface Comparison {
  itemId: string;
  compared: string[];
  said: string;
  keep: string | null;
}

/** Skąd połączenie się wzięło — lustro `connections::Origin`.
 *
 * Brak pola znaczy „plik projektu": tak wygląda każde połączenie zapisane przed 2026-08-22
 * i tak samo rozstrzyga to serde po stronie Rusta. */
export type ConnectionOrigin = 'project' | 'yours-here' | 'yours-everywhere';

export interface ImportedConnection {
  id: string;
  name: string;
  enabled: boolean;
  origin?: ConnectionOrigin;
  /** Plik, w którym ten serwer jest zadeklarowany — lustro `connections::Connection::source`.
   *
   * Opcjonalne, bo połączenie złożone ręcznie w specyfikacji tego pola nie ma, a zdania
   * o pochodzeniu brak ścieżki nie psuje (`whereFrom`). */
  source?: string;
}

export interface ImportPreview {
  snapshot: { root: string; items: SourceItem[] };
  draft: {
    sourceHashes: Record<string, string>;
    items: ImportItem[];
    agents: Array<{ id: string; name: string }>;
    skills: Array<{ name: string }>;
    connections: ImportedConnection[];
    /** Pliki, które ten plan by zapisał, a biblioteka już je ma — lustro
     * `MigrationDraft::already_in_the_library`, ścieżki względem korzenia biblioteki.
     *
     * Ekran czyta z tego JEDNĄ rzecz, której nie niesie `alreadyHere` pozycji: połączenia
     * z `~/.claude.json` nie mają w planie ani jednego wiersza, a to właśnie one startują
     * zaznaczone. Opcjonalne z tego samego powodu, co `alreadyHere` wyżej. */
    alreadyInTheLibrary?: string[];
    workflows: Array<{
      id: string;
      name: string;
      description?: string;
      steps?: Array<{
        kind: 'agent' | 'check' | 'checkpoint';
        id: string;
        name: string;
        command?: string;
      }>;
    }>;
    report: { mappings: Mapping[] };
  };
}

export interface ApplyRequest {
  workspace: string;
  expectedSourceHashes: Record<string, string>;
  enableConnections: string[];
  /** Pole przejściowe dla starego ekranu; T-78 zastępuje je dwiema decyzjami poniżej. */
  leaveOut?: string[];
  excludedItems?: string[];
  withoutBehavior?: string[];
}

export interface ImportReceipt {
  id: string;
  written: string[];
  enabledConnections: string[];
  /** Agenci, którym ten import dopisał połączenia — lustro `ImportReceipt::filled_agents`. */
  filledAgents: FilledAgent[];
}

export interface ImportIo {
  scanSetup(workspace: string): Promise<ImportPreview>;
  applySetup(request: ApplyRequest): Promise<ImportReceipt>;
  /** `null` znaczy „człowiek to zatrzymał" i jest WARTOŚCIĄ, nie odmową (niezmiennik 7):
   *  wiersz wraca wtedy do swojego pytania, bez odpowiedzi i bez zdania o awarii. */
  compareCopies(ask: CompareRequest): Promise<Comparison | null>;
  stopComparing(): Promise<void>;
}

export interface ImportSetupProps {
  onClose: () => void;
  onImported: () => void;
  io?: ImportIo;
  initialPreview?: ImportPreview;
  /** Zapisani agenci — to jeden z nich porówna kopie. Pusta lista znaczy „nie ma kogo
   *  o to poprosić", i wtedy zamiast przycisku stoi zdanie (niezmiennik 16). */
  agents?: Array<{ id: string; name: string }>;
}

/* OKNO MODALA JEST NIEPRZEJRZYSTE, i to jest naprawa, nie preferencja (2026-08-22).
 *
 * Do tego dnia stało tu `bg-panel`, a `--color-panel` to `rgba(255,255,255,0.045)` — bielą-alfa
 * podnosi się powierzchnię LEŻĄCĄ NA TLE APLIKACJI, nie zasłania nią cudzej treści. Nad ekranem
 * Agentów, przy tle za modalem na 72%, ekran spod spodu przebijał przez okno: nazwy agentów
 * i ich opisy nachodziły na wiersze importu i nie dało się tego czytać (zrzut właściciela).
 *
 * `--overlay` (`#1b1b24`) jest tokenem, który DESIGN §6 wymienia przy `modal` wprost, ze słowem
 * „nieprzejrzysty" w nawiasie — więc to nie jest nowy kolor ani odstępstwo od makiety, tylko
 * użycie tego, co makieta na to miejsce przewidziała. Ta sama pomyłka stała w czterech modalach
 * naraz i wszystkie cztery są poprawione razem: jedna klasa, jedna przyczyna. */
/* WEJSCIE JEST SAMA PRZEZROCZYSTOSCIA (`.fade-in`), bo DESIGN §6 mowi o modalu wprost:
 * bez rozmycia i bez animacji wjazdu poza `opacity`. Sprezyna nalezy do powierzchni, ktore
 * wchodza NAD widok, a to okno zaslania go w calosci. Jeden region na jedno zdarzenie. */
const BACKDROP = 'fade-in fixed inset-0 z-20 flex items-center justify-center bg-bg/72 p-6';
/* BEZ DOLNEGO WYPEŁNIENIA, i to jest część paska przy dole, a nie oszczędność (2026-08-29).
 *
 * Okno jest własnym pasem przewijania, więc treść przewija się TAKŻE przez jego wypełnienie.
 * Dopóki `p-6` zostawiało 24 px pod przyklejonym paskiem akcji, spod paska wyzierała kolejna
 * ścieżka z listy „Proposed files" — zależnie od tego, którą krawędź `position: sticky` weźmie
 * za swoją. Dolne 24 px oddaje więc sam pasek (`pb-6`), a ekran bez planu — akapit pod spodem. */
const WINDOW =
  'flex max-h-full w-full max-w-240 flex-col gap-4 overflow-auto rounded-lg border border-line-strong bg-overlay px-6 pt-6';
/* 2026-08-31: trzy stałe z listami klas zeszły do warstwy prymitywów (`theme.css`):
 * `BUTTON` -> `.btn-quiet`, `PRIMARY` -> `.btn-primary`, `ORIGIN` -> `.label`. */

/** Zdanie o pochodzeniu połączenia: KTO JE WIDZI, a zaraz potem — z którego pliku przyjechało.
 *
 * Pierwsza połowa odpowiada na pytanie, które człowiek stojący nad tą listą zadaje najpierw:
 * „to ustawienie zespołu czy moje własne?". Sama ścieżka odpowiadałaby na nie okrężnie i tylko
 * komuś, kto zna trzy zakresy Claude Code na pamięć, więc zostaje tam, gdzie była.
 *
 * DRUGA POŁOWA JEST NOWA (2026-09-16) i ma swój incydent. Serwer bywa zadeklarowany WYŁĄCZNIE
 * w nagłówku agenta, więc u właściciela `figma` wisiała na `.claude/agents/figma-extractor.md`,
 * a `playwright` na `design-qa.md` — i oba znikały razem z tymi plikami, kiedy zostawiał je poza
 * importem. Zaznaczenie trzyma je dziś w planie (`translate::keep_selected_outputs`), ale
 * człowiek nadal nie miał jak się dowiedzieć, na czym one wiszą.
 *
 * TRZECIE ZDANIE JEST Z 2026-09-16 i ma ten sam incydent, co `alreadyHere` przy pozycji: serwer,
 * którego plik biblioteka dostała przy pierwszym imporcie, nie jedzie drugi raz. Bez tego napisu
 * człowiek widzi wyłącznie pusty ptaszek przy własnym połączeniu, które „przecież startuje
 * zaznaczone" — i nie ma jak się dowiedzieć, że nic nie zginęło.
 *
 * JEDEN NAPIS, NIE DWA: drugi region na ten sam fakt podniósłby gęstość listy, która ma po
 * jednym wierszu na połączenie (niezmiennik 18). */
function whereFrom(connection: ImportedConnection, alreadyHere: boolean): string {
  let who = 'in the project';
  if (connection.origin === 'yours-here') who = 'just you, in this project';
  if (connection.origin === 'yours-everywhere') who = 'just you, everywhere';
  const file = connection.source ?? '';
  const from = file === '' ? who : `${who}, from ${file}`;
  return alreadyHere ? `${from} — already in your library, and this import leaves it alone` : from;
}

/** Gdzie w bibliotece leżałby plik tego połączenia.
 *
 * Jedna odpowiedź na cały ekran i jedyne miejsce, w którym ta strona granicy zna tę nazwę;
 * po tamtej liczy ją `import::apply::would_write`. Połączenia z `~/.claude.json` nie mają
 * w planie ani jednego wiersza, więc `alreadyHere` pozycji nie umie o nich nic powiedzieć. */
function connectionFile(connection: ImportedConnection): string {
  return `connections/${connection.id}.json`;
}

/** Czy biblioteka ma już plik tego połączenia. */
function libraryHasConnection(preview: ImportPreview, connection: ImportedConnection): boolean {
  return (preview.draft.alreadyInTheLibrary ?? []).includes(connectionFile(connection));
}

const STATUS: Readonly<Record<Compatibility, string>> = {
  exact: 'Exact',
  adjusted: 'Adjusted',
  needs_choice: 'Needs a choice',
  unsupported: "Can't be reproduced",
};

const SOURCES: Readonly<Record<SourceKind, string>> = {
  claude: 'Claude',
  codex: 'Codex',
  agent_skills: 'Agent Skills',
  rulesync: 'Rulesync',
  open_standard: 'Project file',
  unknown: 'Project file',
};

/* JEDNO SŁOWO NA JEDNĄ RZECZ, i tym słowem jest „Workflow" (2026-08-29).
 *
 * Do tego dnia stało tu `workflow: 'Routine'` — nazwa, która w całym produkcie nie istniała
 * nigdzie indziej: sekcja w pasku nazywa się Workflows, licznik nad tabelą nazywa się
 * Workflows, pliki lądują w `workflows/`. Kolumna „Type" mówiła więc „Routine" trzy cale pod
 * licznikiem mówiącym „Workflows", o tej samej rzeczy (niezmiennik 13). */
const KINDS: Readonly<Record<ItemKind, string>> = {
  agent: 'Agent',
  skill: 'Skill',
  connection: 'Connection',
  workflow: 'Workflow',
  memory: 'Note',
};

const ITEM_STATUS: Readonly<Record<ImportStatus, string>> = {
  ready: 'Ready',
  needs_choice: 'Needs a choice',
  unsupported: "Can't be reproduced",
  missing_dependencies: 'Missing dependencies',
};

/* CO WIDZI CZŁOWIEK, KTÓRY IMPORTUJE TEN SAM PROJEKT DRUGI RAZ (2026-09-16).
 *
 * ZGŁOSZENIE WŁAŚCICIELA, Loadout 0.6.2: drugi import urc-monorepo oddał jedno czerwone zdanie
 * („…already exists. Nothing was imported.") i ani jednego znaku przy wierszach, które przyjechały
 * przy pierwszym imporcie. Człowiek miał je znaleźć okiem wśród kilkudziesięciu pozycji.
 *
 * Status, a nie sam brak ptaszka: pusty ptaszek odpowiada na pytanie „co się stanie", a nie na
 * pytanie „dlaczego". Bez tego słowa wiersz stał na ekranie jako `Ready` i odznaczony naraz. */
const ALREADY_HERE = 'Already in your library';

/** Dlaczego tej jednej pozycji nie da się wnieść jeszcze raz — i co z tym zrobić.
 *
 * Nadpisania tu nie ma i to jest zmierzone, nie ostrożność: `library::agents::write_agent_file`
 * bez oczekiwanej rewizji odmawia istniejącemu plikowi, a `apply::commit` przenosi pliki przez
 * `fs::rename`. Zdanie mówi więc jedyną prawdziwą drogę wyjścia zamiast obiecywać podmianę. */
const WILL_NOT_REPLACE =
  'Loadout will not replace the file you already have. Take that file out of your library first ' +
  'if you want this copy instead.';

function sourceLabel(item: SourceItem): string {
  if (item.source !== undefined) return SOURCES[item.source];
  if (item.path.startsWith('.claude/')) return SOURCES.claude;
  if (item.path.startsWith('.codex/')) return SOURCES.codex;
  if (item.path.startsWith('.agents/')) return SOURCES.agent_skills;
  if (item.path.startsWith('.rulesync/')) return SOURCES.rulesync;
  return SOURCES.unknown;
}

function kindLabel(item: SourceItem): string {
  return KINDS[item.kind];
}

/** Czy ta pozycja czeka na rozstrzygnięcie człowieka — i tylko taka daje się oddać agentowi.
 *
 * `unsupported` jest tu razem z `needs_choice` z rozmysłem: `Can't be reproduced` zostaje
 * widoczne DOPÓKI takiej analizy nie było (docs/PLAN.md §6d), więc to jest dokładnie ten
 * wiersz, przy którym druga opinia ma co powiedzieć. Sam status nie rusza się od niej ani
 * o jotę — porównanie doradza, nie rozstrzyga. */
function awaitsAChoice(item: ImportItem): boolean {
  return item.status === 'needs_choice' || item.status === 'unsupported';
}

/** „a", „a and b", „a, b and c" — ścieżki wyliczone tak, jak wylicza się je w zdaniu.
 *
 * Wprost, a nie `join(', ')`: człowiek czyta tu, ILE plików agent przeczytał, i przecinek
 * przed ostatnim z dwóch wygląda jak lista ucięta w połowie. */
function andThen(paths: readonly string[]): string {
  if (paths.length < 2) return paths.join('');
  return `${paths.slice(0, -1).join(', ')} and ${paths[paths.length - 1] ?? ''}`;
}

/** Zdania agenta o kopiach — przy tej pozycji, której dotyczyły.
 *
 * Cztery rzeczy, w tej kolejności: co przeczytał, co powiedział, co proponuje i KTO decyduje.
 * Ostatnie zdanie jest stałe i nie ma prawa zniknąć: agent doradza, człowiek importuje — ta
 * sama granica, którą trzyma weryfikator (AGENTS.md §2). Ptaszek `Import` i status wiersza
 * zostają tam, gdzie były; ten blok wyłącznie dopisuje tekst. */
function SecondOpinion({ said }: { said: Comparison }): ReactElement {
  return (
    <span className="mt-2 block border-l border-line pl-2">
      <span className="lead block">{`An agent read ${andThen(said.compared)}.`}</span>
      <span className="lead block" data-tone="body">
        {said.said}
      </span>
      {said.keep === null ? null : (
        <span className="lead block" data-tone="body">
          {`It suggests keeping the copy from ${said.keep}.`}
        </span>
      )}
      <span className="lead block">This is advice. You choose what to import.</span>
    </span>
  );
}

/* TĄ SAMĄ DROGĄ, CO POZYCJE NIEGOTOWE (2026-09-16). Wiersz, którego pliki biblioteka już ma,
 * wraca ze skanu odznaczony — bo zaznaczony jest prośbą o zapisanie pliku, którego Loadout nie
 * podmieni, a to kończy się odmową CAŁEGO importu (`apply::preflight`). Powód stoi w wierszu
 * (`ALREADY_HERE`, `WILL_NOT_REPLACE`), więc odznaczenie niczego nie przemilcza. */
function typedExcludedIn(preview: ImportPreview): string[] {
  return preview.draft.items
    .filter((item) => item.status !== 'ready' || item.alreadyHere === true)
    .map((item) => item.id);
}

/** Pozycje pominięte DLATEGO, że już je masz — osobno od tych, które człowiek odznaczył sam. */
function alreadyHereAmong(preview: ImportPreview, excludedItems: readonly string[]): string[] {
  return preview.draft.items
    .filter((item) => item.alreadyHere === true && excludedItems.includes(item.id))
    .map((item) => item.id);
}

/* ODZNACZONE, BO JUŻ JE MASZ, TEŻ DOMYKA ZALEŻNOŚĆ (2026-09-16). Trzy gałęzie niżej pytają
 * „czy to przyjedzie razem z tym" — a agent nie widzi planu, tylko bibliotekę: umiejętność,
 * która leży tam od pierwszego importu, jest dla niego obecna. Bez tego wyjątku drugi import
 * odznaczał umiejętność (bo już ją ma) i zaraz potem liczył jej agenta jako zablokowanego,
 * czyli wygaszał przycisk Import nad planem, który da się wnieść w całości. Ta sama zasada stoi
 * po drugiej stronie granicy (`translate::the_library_already_has`). */
function hasDependency(
  preview: ImportPreview,
  dependency: string,
  excludedItems: readonly string[],
  enabledConnections: readonly string[],
): boolean {
  const divider = dependency.indexOf(':');
  if (divider < 1) return false;
  const kind = dependency.slice(0, divider);
  const name = dependency.slice(divider + 1).toLocaleLowerCase();
  if (kind === 'connection') {
    return preview.draft.connections.some(
      (connection) =>
        (enabledConnections.includes(connection.id) || libraryHasConnection(preview, connection)) &&
        (connection.id.toLocaleLowerCase() === name ||
          connection.name.toLocaleLowerCase() === name),
    );
  }
  if (kind === 'skill') {
    const exists = preview.draft.skills.some((skill) => skill.name.toLocaleLowerCase() === name);
    const selected = preview.draft.items
      .filter((item) => item.kind === 'skill')
      .some(
        (item) =>
          (!excludedItems.includes(item.id) || item.alreadyHere === true) &&
          (item.target?.toLocaleLowerCase().includes(`/skills/${name}/`) === true ||
            item.target?.toLocaleLowerCase().startsWith(`skills/${name}/`) === true),
      );
    return exists && selected;
  }
  if (kind === 'agent') {
    const exists = preview.draft.agents.some(
      (agent) => agent.id.toLocaleLowerCase() === name || agent.name.toLocaleLowerCase() === name,
    );
    const selected = preview.draft.items
      .filter((item) => item.kind === 'agent')
      .some(
        (item) =>
          (!excludedItems.includes(item.id) || item.alreadyHere === true) &&
          item.target?.toLocaleLowerCase().startsWith(`agents/${name}`) === true,
      );
    return exists && selected;
  }
  return false;
}

function typedBlockers(
  preview: ImportPreview,
  excludedItems: readonly string[],
  withoutBehavior: readonly string[],
  enabledConnections: readonly string[],
  read: readonly string[],
): number {
  const compatibility = compatibilityIn(preview);
  return preview.draft.items.filter((item) => {
    if (excludedItems.includes(item.id)) return false;
    if (item.status === 'unsupported') return true;
    if (item.status === 'needs_choice' && !withoutBehavior.includes(item.id)) return true;
    /* CUDZA UMIEJĘTNOŚĆ NIE WCHODZI, ZANIM KTOŚ JĄ PRZECZYTA (2026-08-31, `skill-review.ts`).
       Ta sama zgoda, co pod blokującym znaleziskiem w karcie przeglądu: przycisk kończący
       robotę jest WYŁĄCZONY, dopóki każda taka pozycja nie jest odklikana. Wyłączony przycisk
       jest tu jedyną drogą, bo `apply` i tak sprawdza `blocked > 0` przed wywołaniem. */
    if (mustBeRead(item, compatibility.get(item.id)) && !read.includes(item.id)) return true;
    return item.dependencies.some(
      (dependency) => !hasDependency(preview, dependency, excludedItems, enabledConnections),
    );
  }).length;
}

function blockers(preview: ImportPreview, leaveOut: readonly string[]): number {
  return preview.draft.report.mappings.filter(
    (mapping) =>
      (mapping.compatibility === 'unsupported' || mapping.compatibility === 'needs_choice') &&
      !leaveOut.includes(mapping.itemId),
  ).length;
}

/** Połączenia z TWOJEJ własnej konfiguracji — te startują zaznaczone.
 *
 * 2026-09-14 — DWIE GRUPY, DWIE ODPOWIEDZI, i to nie jest niekonsekwencja. `yours-here`
 * i `yours-everywhere` to `~/.claude.json` tego człowieka: jego własne ustawienie, którego
 * i tak używa codziennie, więc żądanie ptaszka jest żądaniem powtórzenia decyzji, którą już
 * podjął. `project` to plik ze sklonowanego repo — `.mcp.json` z `"command": "npx"` jest
 * uruchomieniem cudzego kodu, i dokładnie przed tym stoi `Connection::imported` z `enabled:
 * false`. Zaznaczone domyślnie zostaje więc to, co i tak jest twoje.
 *
 * Eksport, bo tę samą listę czyta pierwsze otwarcie okna i każdy kolejny „Scan" — dwie kopie
 * tej reguły rozjechałyby się przy pierwszym dopisanym zakresie (niezmiennik 23). */
export function yoursAmong(preview: ImportPreview): string[] {
  return preview.draft.connections
    .filter(
      (connection) =>
        (connection.origin === 'yours-here' || connection.origin === 'yours-everywhere') &&
        /* CHYBA ŻE BIBLIOTEKA JUŻ TO MA (2026-09-16). Ptaszek postawiony za człowieka przy
         * połączeniu, którego plik już leży na dysku, nie wnosi niczego — a do tego dnia wnosił
         * kolizję: serwery z `~/.claude.json` zostają w planie ZAWSZE, więc `linear-server`
         * i `murmur` wywracały drugi import właściciela, zanim spojrzał na tę listę. Ptaszek
         * postawiony RĘCZNIE dalej działa i dalej znaczy „daj to agentom". */
        !libraryHasConnection(preview, connection),
    )
    .map((connection) => connection.id);
}

/** Pozycje, których Loadout nie umie wnieść: nieobsługiwane i te, które wymagają wyboru. */
function unresolvedIn(preview: ImportPreview): string[] {
  return preview.draft.report.mappings
    .filter(
      (mapping) =>
        mapping.compatibility === 'unsupported' || mapping.compatibility === 'needs_choice',
    )
    .map((mapping) => mapping.itemId);
}

export function ImportSetup({
  onClose,
  onImported,
  io: suppliedIo,
  initialPreview,
  agents = [],
}: ImportSetupProps): ReactElement {
  /* DOKĄD ten import zapisze — jedna wartość, z której powstaje ZARÓWNO zdanie na ekranie, jak
   * i argument `folder` lecący do `apply_setup` (2026-09-15, `./where-it-lands.ts`). Dwa osobne
   * odczyty `activeWorkspace()` dałyby się rozjechać przy przełączeniu projektu w bocznym menu,
   * a wtedy okno nazywałoby jedno miejsce i pisało do drugiego — czyli gorzej niż dziś, kiedy
   * nie nazywa żadnego. Czytane RAZ, przy otwarciu: to samo domknięcie, które ma `forProject`. */
  const [target] = useState(() => activeWorkspace());
  const [disk] = useState(() => Disk.forProject(target?.folder ?? null));
  const io = suppliedIo ?? disk;
  /* Import dotyczy projektu otwartego w bocznym menu, więc zaczyna od tego samego, jedynego
   * źródła prawdy co Run i Skills. Puste pole zostaje wyłącznie wtedy, gdy człowiek nie wybrał
   * jeszcze żadnego workspace; wpisanie ścieżki ręcznie nadal pozwala zeskanować inny projekt. */
  const [workspace, setWorkspace] = useState(
    initialPreview?.snapshot.root ?? activeWorkspace()?.folder ?? '',
  );
  const [preview, setPreview] = useState<ImportPreview | null>(initialPreview ?? null);
  const [enabled, setEnabled] = useState<string[]>(
    initialPreview === undefined ? [] : yoursAmong(initialPreview),
  );
  /* POZYCJE NIE DO WNIESIENIA SĄ POMINIĘTE OD RAZU, a nie po 68 kliknięciach.
   *
   * 2026-08-22 — ZGŁOSZENIE WŁAŚCICIELA: „trzeba kliknąć każdy element po kolei, żeby
   * zaimportować, bo inne, których nie da się zaimportować, wtedy nas blokują". Miał rację i to
   * nie było drobiazgiem: jedynym rozstrzygnięciem, jakie ten ekran w ogóle oferuje dla pozycji
   * `needs_choice`, jest „Skip" — więc żądanie wyboru było żądaniem kliknięcia jedynej możliwej
   * odpowiedzi, raz na pozycję. Przy 68 pozycjach import po prostu nie odbywał się nigdy.
   *
   * Domyślne pominięcie NIE UKRYWA niczego: każda taka pozycja stoi w tabeli z powodem i ze
   * swoim zaznaczonym „Skip", a stopka mówi, ile ich jest. Człowiek, który chce jedną z nich
   * wnieść mimo wszystko, odznacza jej pole — i wtedy dopiero import czeka na rozstrzygnięcie,
   * bo o to właśnie poprosił. */
  const [leaveOut, setLeaveOut] = useState<string[]>(
    initialPreview === undefined ? [] : unresolvedIn(initialPreview),
  );
  const [excludedItems, setExcludedItems] = useState<string[]>(
    initialPreview === undefined ? [] : typedExcludedIn(initialPreview),
  );
  const [withoutBehavior, setWithoutBehavior] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [saved, setSaved] = useState<ImportReceipt | null>(null);
  const [inventoryView, setInventoryView] = useState<InventoryView>('all');
  /* 2026-09-09 — uchwyt do listy, zeby filtr mogl ja cofnac na gore. */
  const listRef = useRef<HTMLDivElement>(null);
  /* Kogo pytamy o kopie. Pusty napis znaczy „człowiek jeszcze nie wybrał", a nie „nikogo":
   * lista agentów przyjeżdża propsem i bywa pusta w chwili pierwszego renderu, więc wybór
   * domyślny liczy się przy każdym renderze ([`whoCompares`] niżej), a nie raz w `useState`. */
  const [chosenAgent, setChosenAgent] = useState('');
  /* Kto jest porównywany TERAZ, co po tym zostaje na ekranie i które odpowiedzi są już
   * nieaktualne — jeden obiekt, bo domknięcie obietnicy musi rozstrzygnąć to wszystko naraz
   * (`./comparing.ts`). Jeden naraz, tak jak po tamtej stronie granicy
   * (`commands::import::Comparing`) — drugie pytanie jest tam odmową, więc dwa wiersze mówiące
   * „porównuje teraz" byłyby zdaniem o czymś, co się nie dzieje. */
  const [asked, setAsked] = useState<Comparing>(IDLE);
  /* Umiejętności, o których człowiek powiedział, że je przeczytał. Nie przeżywa skanu: świeży
   * skan czyta pliki na nowo, więc zgoda sprzed niego dotyczyła innego tekstu. */
  const [read, setRead] = useState<string[]>([]);
  /* Które pozycje wyszły poza import HURTEM, jednym kliknięciem `DROP_UNREAD` — podzbiór
   * `excludedItems`, nie druga lista pominięć. Stoi osobno wyłącznie po to, żeby zdanie
   * gotowości umiało powiedzieć, że chodzi o umiejętności, których nikt nie przeczytał:
   * z samego `excludedItems` nie da się tego odróżnić od ptaszka odznaczonego ręcznie. Nie
   * przeżywa skanu, tak samo jak `read` — plan po skanie jest inny. */
  const [dropped, setDropped] = useState<string[]>([]);
  const whoCompares = chosenAgent === '' ? (agents[0]?.id ?? '') : chosenAgent;
  /* Zdanie o tym, komu ten import dał połączenia — `null`, kiedy nie dał nikomu. */
  const gave = saved === null ? null : whatTheyGot(saved.filledAgents);
  const hasTypedItems = preview !== null && preview.draft.items.length > 0;
  const selectedTypedItems =
    preview?.draft.items.filter((item) => !excludedItems.includes(item.id)) ?? [];
  const mappings = new Map(
    preview?.draft.report.mappings.map((mapping) => [mapping.itemId, mapping]) ?? [],
  );
  const blocked =
    preview === null
      ? 0
      : hasTypedItems
        ? typedBlockers(preview, excludedItems, withoutBehavior, enabled, read)
        : blockers(preview, leaveOut);
  /* Ile umiejętności czeka na przeczytanie. Osobno od `blocked`, bo stopka ma nazwać TĘ
     przyczynę po imieniu: „N item(s) still need attention" nie mówi, że wystarczy przeczytać. */
  const unread =
    preview === null || !hasTypedItems ? [] : stillUnread(preview, excludedItems, read);
  /* Ile pozycji zostaje poza importem DLATEGO, że biblioteka już je ma. Osobno od `dropped`
     i od ptaszka odznaczonego ręcznie: to jedyna z trzech przyczyn, której człowiek nie wybrał. */
  const alreadyHere =
    preview === null || !hasTypedItems ? [] : alreadyHereAmong(preview, excludedItems);
  const hasItems = preview !== null && preview.snapshot.items.length > 0;
  const unresolved = preview === null ? [] : unresolvedIn(preview);
  /* Ile pozycji naprawdę zostanie poza importem — liczone z listy pominięć, nie z `unresolved`:
   * człowiek mógł którąś odznaczyć i wtedy ona już nie „zostaje poza", tylko blokuje. */
  const leftOut = unresolved.filter((id) => leaveOut.includes(id)).length;
  const sourceItems = new Map(preview?.snapshot.items.map((item) => [item.id, item]) ?? []);
  const visibleTypedItems = keptBy(
    preview?.draft.items ?? [],
    inventoryView,
    (item) => item.status === 'ready',
  );
  const visibleItems = hasTypedItems
    ? []
    : keptBy(preview?.snapshot.items ?? [], inventoryView, (item) => {
        const compatibility = mappings.get(item.id)?.compatibility ?? 'unsupported';
        return compatibility === 'exact' || compatibility === 'adjusted';
      });
  /* Ile wierszy tabela pokazuje, a ile ich w ogóle jest — z tej pary bierze się odpowiedź
     na pytanie „pusto, bo skan nic nie znalazł, czy pusto, bo filtr to schował". */
  const rowsShown = hasTypedItems ? visibleTypedItems.length : visibleItems.length;
  const rowsAtAll = hasTypedItems
    ? (preview?.draft.items.length ?? 0)
    : (preview?.snapshot.items.length ?? 0);
  /* Wybór agenta stoi na ekranie tylko wtedy, kiedy jest co porównywać: pole nad tabelą,
   * w której każda pozycja jest gotowa, byłoby pytaniem bez skutku (niezmiennik 16). */
  const anyAwaitsAChoice = preview?.draft.items.some(awaitsAChoice) ?? false;

  const scan = (event: FormEvent): void => {
    event.preventDefault();
    if (workspace.trim() === '') return;
    setBusy(true);
    setRefusal(null);
    setSaved(null);
    void io
      .scanSetup(workspace.trim())
      .then((next) => {
        setPreview(next);
        /* Ta sama reguła, co przy pierwszym otwarciu okna: twoje własne połączenia wracają
         * zaznaczone, połączenia z plików projektu wracają wyłączone. */
        setEnabled(yoursAmong(next));
        /* Świeży skan wraca z domyślnym pominięciem, tak samo jak pierwsze otwarcie: inaczej
         * jedno kliknięcie „Scan" cofałoby ekran do stanu, w którym import jest zablokowany. */
        setLeaveOut(unresolvedIn(next));
        setExcludedItems(typedExcludedIn(next));
        setWithoutBehavior([]);
        /* Odpowiedzi z poprzedniego skanu odchodzą razem z nim: zdanie o dwóch kopiach
         * jest zdaniem o KONKRETNYCH plikach, a te po ponownym skanie mogą być inne. Tak samo
         * zgoda na cudzą umiejętność: dotyczyła tekstu sprzed tego skanu. */
        setAsked(IDLE);
        setRead([]);
        setDropped([]);
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not inspect that folder.'));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  /* Zamknięcie okna — i dopiero TU ekran za nim dowiaduje się o imporcie.
   *
   * 2026-09-14 — DO TEGO DNIA `onImported()` szło zaraz po zapisie, a ten handler u jedynego
   * wołającego (`sections/agents/index.tsx`) ZDEJMUJE to okno. Zdanie „N files imported."
   * powstawało więc w komponencie, który w tej samej chwili znikał: linia wyniku nie miała jak
   * dojść do człowieka ani razu, a nikt tego nie zauważył, bo żadne kryterium nie klikało
   * Importu naprawdę (niezmiennik 29 — kryterium zielone, funkcja martwa). Lista za oknem
   * odświeża się teraz przy zamknięciu, czyli wtedy, kiedy w ogóle da się ją zobaczyć.
   *
   * Ten sam kształt, co w drugim oknie importu (`ui/project-setup/modal.tsx`): tam po zapisie
   * zostaje sekcja sukcesu i przycisk „Done", a nie zniknięcie bez słowa. */
  const finish = (): void => {
    if (saved !== null) onImported();
    onClose();
  };

  const apply = (): void => {
    if (preview === null || !hasItems || blocked > 0 || saved !== null) return;
    setBusy(true);
    setRefusal(null);
    void io
      .applySetup({
        workspace: preview.snapshot.root,
        expectedSourceHashes: preview.draft.sourceHashes,
        enableConnections: enabled,
        leaveOut: hasTypedItems ? [] : leaveOut,
        excludedItems: hasTypedItems ? excludedItems : [],
        withoutBehavior: hasTypedItems ? withoutBehavior : [],
      })
      .then((saved) => {
        setSaved(saved);
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not save that setup.'));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  /* Jedno pytanie do agenta o kopie JEDNEJ pozycji.
   *
   * `setBusy` NIE JEST tu wołane z rozmysłem: `busy` blokuje Scan i Import, a porównanie nie
   * zmienia planu ani o wiersz — człowiek ma w jego trakcie nadal móc odznaczyć ptaszek albo
   * wnieść resztę. Jedyne, co ta droga zmienia, to tekst przy pozycji. */
  const compare = (item: ImportItem): void => {
    if (preview === null || whoCompares === '' || asked.at !== null) return;
    /* Numer, z którym to pytanie wyrusza. Domknięcie obietnicy przedstawia się nim przy
       powrocie, więc odpowiedź na pytanie, którego już nie ma, nie ma jak wylądować na
       ekranie (niezmiennik 7: monotoniczna generacja, nigdy flaga). */
    const mine = asked.ask + 1;
    setAsked(asking(asked, item.id));
    void io
      .compareCopies({ workspace: preview.snapshot.root, item: item.id, agent: whoCompares })
      .then((said) => {
        /* `null` znaczy „człowiek nacisnął Stop" i jest WARTOŚCIĄ, nie odmową (niezmiennik 7):
         * wiersz wraca wtedy do swojego pytania, bez odpowiedzi i bez zdania o awarii. */
        setAsked((now) => answered(now, mine, item.id, said));
      })
      .catch((error: unknown) => {
        /* Zdanie ląduje PRZY TEJ POZYCJI, nie w pasku nad tabelą: pytanie dotyczyło jednego
           wiersza, a pasek `role="alert"` odpowiada za Scan i Import. */
        setAsked((now) => refused(now, mine, item.id, askFailed(why(error, COULD_NOT_ASK))));
      });
  };

  /* „Stop": zwalnia wiersz OD RAZU i dopiero potem prosi Rusta, żeby agent zszedł.
   *
   * 2026-08-31 — DO TEGO DNIA BYŁO ODWROTNIE i to nie był drobiazg. Lokalne „porównuje teraz"
   * czyściła wyłącznie odpowiedź `compareCopies`; kiedy ta nie wracała — agent zawieszony,
   * kanał zerwany, `stop_comparing_copies` odrzucone — wiersz mówił „An agent is comparing the
   * copies now." BEZ KOŃCA, a każdy inny wiersz miał wyłączone pytanie, bo warunek patrzy na
   * jedno pole. Limitu czasu nie ma nigdzie, więc jedynym wyjściem było zamknięcie okna razem
   * z całym planem. Dowód, że agent naprawdę zszedł, dalej wraca tamtą drogą i dalej jest
   * jedynym miejscem, w którym ten fakt mieszka (niezmiennik 13) — ale ekran przestaje być
   * jego zakładnikiem. */
  const stopComparing = (): void => {
    const at = asked.at;
    if (at === null) return;
    const free = stopped(asked);
    setAsked(free);
    void io.stopComparing().catch((error: unknown) => {
      setAsked((now) => refused(now, free.ask, at, stopFailed(why(error, COULD_NOT_STOP))));
    });
  };

  return (
    <div className={BACKDROP}>
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-setup-title"
        className={WINDOW}
      >
        <div className="flex items-center gap-3">
          <div>
            <h2 id="import-setup-title" className="text-heading text-ink">
              Import setup
            </h2>
            <p className="lead">
              Turn project agents, skills, connections, workflows, and notes into Loadout files.
            </p>
            {/* CO ZOSTAJE W PROJEKCIE, powiedziane wprost (2026-08-29).
                Do tego dnia hooki, reguły i ustawienia cudzej aplikacji stały na tej liście
                jako wiersze z pytaniem bez odpowiedzi. Zniknęły, bo Loadout nie ma dla nich
                ani sekcji, ani wykonawcy — ale zniknęły też BEZ SŁOWA, a milczenie po takiej
                zmianie czyta się jak przeoczenie. Drugie zdanie mówi też, dlaczego to nie jest
                strata: krok startuje z `current_dir` w folderze projektu, więc `CLAUDE.md`
                i `.claude/rules/` czyta sam agent. Zmierzone sondą tego samego dnia — także
                pod `--setting-sources ""`, wbrew temu, co mówi dokumentacja vendora. */}
            <p className="lead">
              Hooks, project rules, and app settings stay in the project — agents read them from the
              project folder.
            </p>
          </div>
          <button type="button" className="btn-quiet ml-auto" onClick={finish}>
            Close
          </button>
        </div>

        <form className="flex items-end gap-2" onSubmit={scan}>
          <label className="stack label min-w-0 flex-1">
            Project folder
            <input
              value={workspace}
              onChange={(event) => {
                setWorkspace(event.target.value);
              }}
              placeholder="/Users/you/project"
              className="field"
            />
          </label>
          <button type="submit" disabled={busy || workspace.trim() === ''} className="btn-primary">
            Scan
          </button>
        </form>

        {/* DOKĄD TE PLIKI TRAFIĄ, powiedziane PRZED Skanem i PRZED Importem (2026-09-15).
            Pole nad tym zdaniem odpowiada na pytanie „co czytamy" i przy tym znaczeniu zostaje;
            to zdanie odpowiada na drugie, którego ekran nie zadawał: `apply_setup` bierze
            OSOBNY argument `folder`, czyli bibliotekę projektu aktywnego w oknie. Powód, dla
            którego te dwa miejsca rozjeżdżają się bez niczyjego błędu, stoi w nagłówku
            `./where-it-lands.ts`.

            JEDEN ŻYWY REGION NA TEN FAKT (niezmiennik 13): po zapisie ten sam projekt nazywa
            zdanie wyniku, więc to znika dokładnie wtedy, kiedy tamto się pojawia. */}
        {saved === null ? (
          <p className="lead">{landingSays(preview?.snapshot.root ?? workspace, target)}</p>
        ) : null}

        {/* WSKAŹNIK TRWANIA NA GRANICĘ IPC (DESIGN §7). Skan cudzego projektu i zapis planu idą
            przez Rusta i przez dysk, a do 2026-08-31 nie zmieniały tu ani jednego piksela:
            oba przyciski dostawały wyłącznie `disabled`, więc kliknięcie kończyło się ciszą,
            a cisza czyta się jak kliknięcie, które nie doszło.

            JEDEN ŻYWY REGION NA TEN FAKT (niezmiennik 13) i dlatego pasek stoi TYLKO tutaj:
            żaden przycisk tego okna nie przepisuje swojej etykiety na „Scanning…", więc pasek
            nie powtarza niczego, co już jest na ekranie. Nieokreślony, bo przejścia przez
            granicę nie da się wyrazić w procentach. */}
        {busy ? <span aria-hidden className="working shrink-0" /> : null}

        {refusal === null ? null : (
          /* Pasek błędu: wypełnienie i krawędź bez promienia — trzecia z trzech rzeczy,
             które niosą barwę stanu (DESIGN §6). WEJŚCIE, bo to zdanie PRZYCHODZI i jest
             jedyną odpowiedzią na Scan albo Import, który się nie udał. */
          <p
            role="alert"
            className="enter border border-fail-edge bg-fail-soft p-3 text-body text-fail"
          >
            {refusal}
          </p>
        )}
        {saved === null ? null : (
          /* KTO CO DOSTAŁ, po zatwierdzeniu i z tego, co Rust NAPRAWDĘ zapisał
             (`ImportReceipt::filled_agents`). Zdanie zbudowane tu z zaznaczeń mówiłoby
             o zamiarze, a nie o tym, co się stało — a różnica między jednym a drugim jest
             całą treścią tego produktu (`docs/FOUNDATIONS.md` §2.1). */
          <p role="status" className="enter text-ink">
            {importedSays(saved.written.length, target)}
            {gave === null ? '' : ` ${gave}`}
            {/* CO Z ROZMOWĄ, KTÓRA JUŻ TRWA. Zmierzone: sterownik lidera dostaje połączenia
                raz, przy starcie rozmowy — więc człowiek, który zaimportuje je i napisze do
                otwartego okna, dalej nie będzie miał narzędzi. Zdanie stoi w TYM samym
                regionie co wynik, bo to jest jedna wiadomość o jednym imporcie. */}
            <span className="lead block">{A_NEW_CONVERSATION}</span>
          </p>
        )}

        {preview === null ? (
          <p className="lead pb-6">
            Scan reads setup files only. It does not run hooks, skills, agents, or connections.
          </p>
        ) : (
          <>
            <div className="grid grid-cols-5 gap-2 border-y border-line py-3 text-center">
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'agent').length
                    : preview.draft.agents.length}
                </b>
                <small className="text-muted">Agents</small>
              </span>
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'skill').length
                    : preview.draft.skills.length}
                </b>
                <small className="text-muted">Skills</small>
              </span>
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'connection').length
                    : preview.draft.connections.length}
                </b>
                <small className="text-muted">Connections</small>
              </span>
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'workflow').length
                    : preview.draft.workflows.length}
                </b>
                <small className="text-muted">Workflows</small>
              </span>
              {/* PIĄTY LICZNIK, bo notatki też się importują (2026-08-29).
                  Na zrzucie właściciela plan wnosił 16 notatek i nie było ich w tym wierszu
                  ani razu: cztery liczniki nad tabelą pokrywały 28 z 49 pozycji. Wektor
                  sprzed T-78 nie ma notatek w ogóle, więc dla niego uczciwą liczbą jest 0 —
                  ten kształt planu powstał, zanim import notatek istniał. */}
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'memory').length
                    : 0}
                </b>
                <small className="text-muted">Notes</small>
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="label">Show</span>
              {/* FILTR TO TRZY PIGUŁKI, NIE PRZYCISK PODSTAWOWY OBOK DWÓCH DRUGOPLANOWYCH.
                  Do 2026-08-31 wybrany filtr brał wypełniony akcent, a dwa pozostałe obrys —
                  czyli miejsce najgłośniejszej rzeczy na ekranie zajmował filtr, a nie akcja,
                  po którą człowiek to okno otworzył (DESIGN §6). Po zejściu do prymitywów
                  różnica byłaby też różnicą WYSOKOŚCI (36 px kontra 28), więc trzy sąsiadujące
                  kontrolki skakałyby przy każdym przełączeniu. Chip ma jedną wysokość dla
                  wszystkich tonów, a `data-tone` mówi, który jest wybrany. */}
              {(
                [
                  ['all', 'All'],
                  ['ready', 'Ready'],
                  ['attention', 'Needs attention'],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  className="chip"
                  data-tone={inventoryView === value ? 'accent' : undefined}
                  onClick={() => {
                    setInventoryView(value);
                    /* 2026-09-09 — FILTR WRACA NA GÓRĘ LISTY. Bez tego lista zachowuje
                       przewinięcie sprzed przełączenia, a po zawężeniu do kilku pozycji stoi
                       przewinięta POZA pierwszą z nich: człowiek pyta „co wymaga uwagi"
                       i dostaje pustą górę listy. Zmierzone w chromium — po kliknięciu
                       w ptaszek lista stała na `scrollTop: 40`, a pierwszy wiersz lądował
                       8 px nad własnym pojemnikiem. Wcześniej ukrywał to rozmiar pola
                       zaznaczenia: mniejsze nie wyzwalało przewinięcia i wada spała. */
                    listRef.current?.scrollTo({ top: 0 });
                  }}
                >
                  {label}
                </button>
              ))}
              {/* KTO PORÓWNA KOPIE — jedno pole na całą tabelę, nie jedno na wiersz.
                  Wybór agenta jest tą samą odpowiedzią dla każdej pozycji, która o niego
                  prosi, a siedemnaście takich wierszy to siedemnaście identycznych list
                  do rozwinięcia. Znika razem z powodem: bez ani jednej pozycji czekającej
                  na rozstrzygnięcie nie ma czego porównywać. */}
              {!anyAwaitsAChoice || agents.length === 0 ? null : (
                <label className="label ml-auto flex items-center gap-2">
                  Who should compare them?
                  <select
                    value={whoCompares}
                    onChange={(event) => {
                      setChosenAgent(event.target.value);
                    }}
                    className="field w-auto"
                  >
                    {agents.map((agent) => (
                      <option key={agent.id} value={agent.id}>
                        {agent.name}
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </div>
            {/* TABELA DOSTAJE WYSOKOŚĆ JAKO PIERWSZA, nie jako ostatnia (2026-08-29).

                ZGŁOSZENIE WŁAŚCICIELA, ze zrzutu ekranu: skan projektu `meetnotes` znalazł
                86 pozycji i cała tabela zniknęła — została po niej kreska. Stało za tym samo
                `min-h-0`, które tu wcześniej było: okno jest kolumną fleksa z `max-h-full`,
                więc kiedy treść przerasta okno, flexbox ścieśnia dzieci — a ścisnąć się
                poniżej własnej treści pozwalało TYLKO to jedno. Cały nadmiar rosnącej listy
                „Proposed files" szedł więc z tabeli, aż do zera pikseli. Filtr sterował wtedy
                listą, której nie widać, a 37 pominiętych pozycji nie dało się ani obejrzeć,
                ani odznaczyć — czyli jedyna rzecz, po którą się to okno otwiera.

                `flex-1` oddaje jej wolne miejsce jako pierwszej, `min-h-40` stawia podłogę,
                poniżej której już nikt jej nie ścieśni. Dowodzi tego
                `e2e/tests/import-list-stays-visible.spec.ts`, w prawdziwym chromium: układu
                nie liczy ani czysty moduł, ani `renderToStaticMarkup`, więc obie te drogi
                dałyby zielone na kodzie, na którym właściciel widział pusty ekran. */}
            {/* WEJŚCIE SPRĘŻYNĄ, bo ta tabela POJAWIA SIĘ po Scanie nad pustym oknem —
                a to jest dokładnie ta chwila, w której człowiek dowiaduje się, że skan coś
                znalazł. Jeden region na jedno zdarzenie: liczniki i pasek akcji przybywają
                z nią razem, ale nie ruszają się (sufit z ARCHITECTURE §7 wynosi dwa). */}
            <div
              ref={listRef}
              data-import-items
              className="enter min-h-40 flex-1 overflow-auto rounded-md border border-line"
            >
              <table className="w-full table-fixed border-collapse text-left">
                <thead className="sticky top-0 bg-panel text-label text-muted">
                  <tr>
                    <th className="w-2/5 px-3 py-2 font-normal">Item</th>
                    <th className="w-24 px-3 py-2 font-normal">Type</th>
                    <th className="w-28 px-3 py-2 font-normal">Source</th>
                    <th className="w-36 px-3 py-2 font-normal">Status</th>
                    <th className="w-24 px-3 py-2 font-normal">Include</th>
                  </tr>
                </thead>
                <tbody>
                  {hasTypedItems
                    ? visibleTypedItems.map((item) => {
                        const sourceItem = sourceItems.get(item.id);
                        /* KAŻDA definicja, nie pierwsza z brzegu (2026-08-29). Odkąd jeden
                           wiersz scala kopie z kilku aplikacji, pokazanie jednej ścieżki
                           przemilczałoby to, że decyzja dotyczy ich obu. */
                        const definitions = item.sources.filter(
                          (source) => source.role === 'definition',
                        );
                        const definition = definitions[0] ?? item.sources[0];
                        /* Do lokalnej stałej, nie w JSX: pod `noUncheckedIndexedAccess` odczyt
                           z mapy daje `Comparison | undefined`, a zawężenie po `item.id`
                           (odczyt pola, nie stała) nie przeżywa granicy wyrażenia. */
                        const compared = asked.answers[item.id];
                        /* Zdanie o przerwanym albo nieudanym pytaniu — przy TEJ pozycji,
                           której dotyczyło. Do stałej, bo `asked.said?.item === item.id`
                           nie zawęża `asked.said` poza granicę wyrażenia. */
                        const aboutThis =
                          asked.said !== null && asked.said.item === item.id
                            ? asked.said.sentence
                            : null;
                        /* Czy tę cudzą umiejętność trzeba przeczytać, zanim wolno ją wnieść
                           (`./skill-review.ts`). */
                        const waitsToBeRead = mustBeRead(
                          item,
                          mappings.get(item.id)?.compatibility,
                        );
                        /* Co przegląd w tym pliku znalazł. Pusta lista przy pozycji, która
                           umiejętnością nie jest, ORAZ przy umiejętności, w której nic nie
                           znaleziono — te dwa zdania rozróżnia `item.reviewed`, a nie długość
                           listy, i tylko dlatego blok niżej nie mówi „nic nie ma" nad czymś,
                           czego nikt nie czytał.

                           `kind` w warunku nie jest ozdobą: przegląd dotyczy WYŁĄCZNIE
                           umiejętności (Rust wpisuje go tylko dla nich), więc klucz przy
                           notatce albo połączeniu byłby usterką po tamtej stronie — a blok
                           niżej otwiera się zdaniem „A skill becomes instructions…", czyli
                           usterka wyszłaby na ekran jako zdanie o czymś, czym ta pozycja nie
                           jest (niezmiennik 5: nieznane pole porzucamy, nie ufamy mu). */
                        const findings =
                          item.kind === 'skill' ? (item.reviewed?.findings ?? []) : [];
                        return (
                          <tr key={item.id} className="border-t border-line align-top">
                            <td className="px-3 py-2">
                              <b className="block truncate text-body text-ink">
                                {sourceItem?.name ?? item.target ?? item.id}
                              </b>
                              {(definitions.length > 0 ? definitions : item.sources).map(
                                (source) => (
                                  <span
                                    key={source.path}
                                    className="block truncate font-mono text-meta text-muted"
                                  >
                                    {source.path}
                                  </span>
                                ),
                              )}
                              <span className="lead block" data-tone="body">
                                {item.statusMessage}
                              </span>
                              {/* DLACZEGO TEN WIERSZ NIE WCHODZI, przy nim i po ludzku
                                  (2026-09-16). Kolumna Status mówi JAKI to stan; to zdanie mówi,
                                  co się z nim stanie i jaka jest droga wyjścia. Sam pusty ptaszek
                                  nie mówi ani jednego, ani drugiego — a przez to wyglądał jak
                                  ekran, który o tym wierszu zapomniał. */}
                              {item.alreadyHere !== true ? null : (
                                <span className="lead block">{WILL_NOT_REPLACE}</span>
                              )}
                              {item.target === null ? null : (
                                <span className="block font-mono text-meta text-muted">
                                  Target: {item.target}
                                </span>
                              )}
                              {item.dependencies.length === 0 ? null : (
                                <span className="lead block">
                                  Requires: {item.dependencies.join(', ')}
                                </span>
                              )}
                              {/* CUDZA UMIEJĘTNOŚĆ, TYMI SAMYMI SŁOWAMI, CO PRZY WKLEJONYM
                                  LINKU (2026-08-31). Powód i granica tej drogi stoją w nagłówku
                                  `./skill-review.ts`, a lista znalezisk — w
                                  `../skills/findings.tsx`; tutaj są wyłącznie zdania o tym
                                  ekranie. Blok wisi pod ścieżkami plików, bo to o nich mówi
                                  „Open the file above" — kreska z lewej wiąże go z tym wierszem
                                  tak samo, jak wiąże drugą opinię.

                                  DWA POWODY, DLA KTÓRYCH TEN BLOK STOI, i nie są tym samym
                                  powodem. `waitsToBeRead` znaczy „ta pozycja czeka na twoją
                                  zgodę"; znaleziska znaczą „przegląd coś w niej znalazł".
                                  Umiejętność ZATRZYMANA przeglądem ma to drugie bez pierwszego:
                                  jest `Unsupported`, zgody od nikogo nie potrzebuje, bo nie
                                  wejdzie — i to właśnie przy niej człowiek najbardziej
                                  potrzebuje wiedzieć, co w niej stało.

                                  Kontenerem jest `<div>`, nie `<span>`: lista znalezisk jest
                                  `<ul>`, a lista w treści śródliniowej to znacznik, którego
                                  przeglądarka nie ma prawa dostać. */}
                              {!waitsToBeRead && findings.length === 0 ? null : (
                                <div className="mt-2 border-l border-line pl-2">
                                  <span className="lead block" data-tone="body">
                                    {BECOMES_INSTRUCTIONS}
                                  </span>
                                  {/* Zgody per znalezisko ten ekran NIE MA i dlatego jej tu nie
                                      udaje: `blockingSays` zamiast `onAcknowledge`. Odklikanie
                                      niczego by nie zmieniło — `stage_skills` odmawia zawsze,
                                      kiedy przegląd zatrzymał umiejętność. */}
                                  <Findings findings={findings} blockingSays={STOPS_IT} />
                                  {/* Pozycja WYJĘTA z importu dostaje same fakty i ani jednego
                                      żądania: „przeczytaj to" nad czymś, co i tak nie wchodzi,
                                      jest prośbą bez skutku. Zdania o tym, że zostaje poza,
                                      tu nie ma — mówi to jej odznaczony ptaszek `Import`
                                      (niezmiennik 13). */}
                                  {!waitsToBeRead ||
                                  excludedItems.includes(item.id) ? null : read.includes(
                                      item.id,
                                    ) ? (
                                    <span className="lead block">{READ_ALREADY}</span>
                                  ) : (
                                    <>
                                      <span className="lead block">{OPEN_IT}</span>
                                      <button
                                        type="button"
                                        data-read-skill={item.id}
                                        className="btn-quiet mt-1"
                                        onClick={() => {
                                          setRead((now) =>
                                            now.includes(item.id) ? now : [...now, item.id],
                                          );
                                        }}
                                      >
                                        {READ_IT}
                                      </button>
                                    </>
                                  )}
                                </div>
                              )}
                              {/* DRUGA OPINIA STOI PRZY POZYCJI, KTÓREJ DOTYCZY (2026-08-29).
                                  Adapter sam prosi o agenta („Let an agent compare them before
                                  import.") i do dziś nikt go nie wołał: jedyną odpowiedzią tego
                                  ekranu dla siedemnastu takich wierszy było pominięcie. Wiersz
                                  niesie już OBIE ścieżki — pytanie jest o to, czym te kopie się
                                  różnią, więc odpowiedź nie ma prawa wylądować nigdzie indziej
                                  niż w tej samej komórce. Statusu, ptaszka `Import` ani liczników
                                  nad tabelą ta droga nie dotyka ani o jotę. */}
                              {!awaitsAChoice(item) ? null : (
                                <span className="mt-2 block">
                                  {agents.length === 0 ? (
                                    /* Nie ma kogo poprosić, więc nie ma tu przycisku: kontrolka
                                       bez skutku jest gorsza od zdania (niezmiennik 16). */
                                    <span className="lead block">
                                      Save an agent first, then it can compare copies for you.
                                    </span>
                                  ) : asked.at === item.id ? (
                                    <>
                                      {/* WSKAŹNIK TRWANIA, bo to trwa: agent czyta dwa pliki
                                          i pisze o nich zdanie, a do 2026-08-31 przez cały ten
                                          czas nie zmieniał się ani jeden piksel. Kropki są
                                          DZIEĆMI, nie pseudoelementami — `aria-hidden` na nich
                                          jest czytelne dla czytnika ekranu, a zdanie obok niesie
                                          treść (DESIGN §7). Jeden na całą tabelę, bo po tamtej
                                          stronie granicy porównanie jest też jedno. */}
                                      <span className="lead block" data-tone="body">
                                        An agent is comparing the copies now.
                                        <span className="thinking ml-1">
                                          <span aria-hidden />
                                          <span aria-hidden />
                                          <span aria-hidden />
                                        </span>
                                      </span>
                                      <button
                                        type="button"
                                        data-stop-comparing
                                        className="btn-quiet mt-1"
                                        onClick={stopComparing}
                                      >
                                        Stop
                                      </button>
                                    </>
                                  ) : (
                                    <button
                                      type="button"
                                      data-compare-copies={item.id}
                                      disabled={asked.at !== null}
                                      className="btn-quiet"
                                      onClick={() => {
                                        compare(item);
                                      }}
                                    >
                                      {/* Jedna kopia to nie jest wybór między kopiami, więc
                                          i pytanie brzmi inaczej: to są te dwa skille nie do
                                          odtworzenia i dwie ceremonie bez wykonalnej treści. */}
                                      {definitions.length > 1
                                        ? 'Compare the copies'
                                        : 'Explain this'}
                                    </button>
                                  )}
                                  {aboutThis === null ? null : (
                                    <span className="lead mt-1 block" data-tone="body">
                                      {aboutThis}
                                    </span>
                                  )}
                                  {compared === undefined ? null : (
                                    <SecondOpinion said={compared} />
                                  )}
                                </span>
                              )}
                            </td>
                            <td className="px-3 py-2 text-body text-ink">{KINDS[item.kind]}</td>
                            <td className="px-3 py-2 text-body text-muted">
                              {definition === undefined
                                ? SOURCES.unknown
                                : SOURCES[definition.provider]}
                            </td>
                            <td className="px-3 py-2 text-body text-muted">
                              {item.alreadyHere === true ? ALREADY_HERE : ITEM_STATUS[item.status]}
                            </td>
                            <td className="px-3 py-2">
                              <Tick
                                className="flex items-center gap-2 text-body text-ink"
                                label="Import"
                                name="Import this item"
                                checked={!excludedItems.includes(item.id)}
                                onChange={(event) => {
                                  const { checked } = event.target;
                                  setExcludedItems((now) =>
                                    checked
                                      ? now.filter((id) => id !== item.id)
                                      : [...now, item.id],
                                  );
                                  /* Ptaszek postawiony z powrotem cofa TĘ pozycję z hurtowego
                                     wyrzucenia (2026-09-15). Bez tego stopka dalej liczyłaby
                                     ją wśród nieprzeczytanych umiejętności, których poza
                                     importem już nie ma — a to jest zdanie o czymś, co się
                                     nie dzieje. */
                                  if (checked) {
                                    setDropped((now) => now.filter((id) => id !== item.id));
                                  }
                                }}
                              />
                              {item.status !== 'needs_choice' ? null : (
                                <Tick
                                  className="mt-2 flex items-start gap-2 text-note text-ink"
                                  label="Without behavior"
                                  name="Import without this behavior"
                                  checked={withoutBehavior.includes(item.id)}
                                  onChange={(event) => {
                                    setWithoutBehavior((now) =>
                                      event.target.checked
                                        ? [...now, item.id]
                                        : now.filter((id) => id !== item.id),
                                    );
                                  }}
                                />
                              )}
                            </td>
                          </tr>
                        );
                      })
                    : visibleItems.map((item) => {
                        const mapping = mappings.get(item.id);
                        const unresolvedItem =
                          mapping?.compatibility === 'needs_choice' ||
                          mapping?.compatibility === 'unsupported';
                        return (
                          <tr key={item.id} className="border-t border-line align-top">
                            <td className="px-3 py-2">
                              <b className="block truncate text-body text-ink">{item.name}</b>
                              <span className="block truncate font-mono text-meta text-muted">
                                {item.path}
                              </span>
                              <span className="lead block" data-tone="body">
                                {mapping?.message ?? item.summary}
                              </span>
                            </td>
                            <td className="px-3 py-2 text-body text-ink">{kindLabel(item)}</td>
                            <td className="px-3 py-2 text-body text-muted">{sourceLabel(item)}</td>
                            <td className="px-3 py-2 text-body text-muted">
                              {mapping === undefined
                                ? "Can't be reproduced"
                                : STATUS[mapping.compatibility]}
                            </td>
                            <td className="px-3 py-2">
                              {unresolvedItem ? (
                                <Tick
                                  className="flex items-center gap-2 text-body text-ink"
                                  label="Skip"
                                  name={
                                    mapping.compatibility === 'unsupported'
                                      ? 'Leave this item out of the import'
                                      : 'Import without this behavior'
                                  }
                                  checked={leaveOut.includes(item.id)}
                                  onChange={(event) => {
                                    setLeaveOut((now) =>
                                      event.target.checked
                                        ? [...now, item.id]
                                        : now.filter((id) => id !== item.id),
                                    );
                                  }}
                                />
                              ) : (
                                <span className="text-body text-muted">Yes</span>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                  {/* TABELA, KTÓREJ NIE WIDAĆ, MÓWI DLACZEGO (2026-08-31).
                      Skan puszczony przy filtrze „Needs attention" nad projektem z samymi
                      gotowymi pozycjami renderował `<tbody>` PUSTY, bez ani jednego zdania —
                      a liczniki nad nim mówiły wtedy „17 Skills". Jedyne, co się z tego czyta,
                      to że skan się zepsuł. Reguła i zdanie mieszkają w `./shown.ts`, bo ten
                      stan powstaje po dwóch kliknięciach, a `renderToStaticMarkup` klika nie
                      umie (niezmiennik 29, droga „czysty moduł"). */}
                  {!hidesEverything(rowsShown, rowsAtAll, inventoryView) ? null : (
                    <tr className="border-t border-line">
                      <td colSpan={5} className="px-3 py-2">
                        <span className="lead block">{hiddenSays(rowsAtAll, inventoryView)}</span>
                        <button
                          type="button"
                          data-show-all
                          className="btn-quiet mt-2"
                          onClick={() => {
                            setInventoryView('all');
                          }}
                        >
                          {SHOW_ALL}
                        </button>
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            {preview.draft.connections.length === 0 ? null : (
              <fieldset className="flex flex-col gap-2 border-t border-line pt-3">
                <legend className="flex w-full items-center gap-3 text-subhead text-ink">
                  {/* ZDANIE MUSI BYĆ PRAWDZIWE DLA OBU GRUP (2026-09-14). Stało tu
                      „Connections stay off unless you enable them" — od dnia, w którym twoje
                      własne połączenia startują zaznaczone, to jest zdanie o połowie listy,
                      a człowiek czyta je nad ptaszkami, które mówią co innego. */}
                  Your own connections start on, the project’s stay off
                  {/* JEDNO KLIKNIĘCIE ZAMIAST N, i to jest cała treść tego przycisku (2026-08-22).
                      Projekt z siedmioma serwerami to dziś siedem ptaszków, a każdy z nich jest tą
                      samą decyzją. Przycisk NIE zmienia reguły — nadal to człowiek włącza, nadal
                      widzi każdą pozycję z osobna i nadal może odznaczyć. */}
                  <button
                    type="button"
                    data-enable-all
                    className="btn-quiet ml-auto"
                    disabled={enabled.length === preview.draft.connections.length}
                    onClick={() => {
                      setEnabled(preview.draft.connections.map((one) => one.id));
                    }}
                  >
                    Turn them all on
                  </button>
                </legend>
                {preview.draft.connections.map((connection) => (
                  <Tick
                    key={connection.id}
                    className="flex items-center gap-2 text-body text-ink"
                    label={connection.name}
                    /* Nazwa połączenia na samym polu: bez niej nie da się powiedzieć, KTÓRY
                       ptaszek jest zaznaczony, ani z testu, ani z e2e. */
                    field={connection.id}
                    checked={enabled.includes(connection.id)}
                    onChange={(event) => {
                      setEnabled((now) =>
                        event.target.checked
                          ? [...now, connection.id]
                          : now.filter((id) => id !== connection.id),
                      );
                    }}
                  >
                    {/* SKĄD TO JEST, przy nazwie i po cichu. Człowiek stojący nad tą listą pyta
                        o jedno: czy to ustawienie zespołu, czy moje własne — a od 2026-08-22 na
                        liście stoją obie rodzaje naraz. Bez tego zdania `linear-server` z twojej
                        prywatnej konfiguracji wygląda identycznie jak `context7` z repo.
                        Od 2026-09-16 ten sam napis niesie też plik, w którym serwer jest
                        zadeklarowany — powód w całości stoi przy `whereFrom`. */}
                    <span className="label">
                      {whereFrom(connection, libraryHasConnection(preview, connection))}
                    </span>
                  </Tick>
                ))}
                {/* CO TE PTASZKI ZROBIĄ AGENTOM, powiedziane PRZED zatwierdzeniem
                    (niezmiennik 29). Do 2026-09-14 agent przyjeżdżał z pustą listą połączeń
                    i nikt tego nie mówił: człowiek włączał pięć serwerów i dostawał agentów,
                    którzy nie widzieli ani jednego. */}
                <p className="lead">
                  Agents that name no connection get the ones you turn on here. Agents that already
                  name their own keep them.
                </p>
              </fieldset>
            )}
            {/* SEKCJA „PROPOSED FILES" ZNIKŁA, bo mówiła to samo dwa razy (2026-08-29).
                Jej lista ścieżek powtarzała wiersz `Target:` stojący przy każdej pozycji,
                a jej podsumowanie powtarzało liczniki nad tabelą — jeden fakt w trzech żywych
                miejscach przy limicie 1 (niezmiennik 13). To ona rosła bez sufitu i to ona
                zabierała tabeli całą wysokość okna, aż tabela zniknęła z ekranu. */}
            <div className="sticky bottom-0 z-10 -mx-6 flex items-center gap-3 border-t border-line bg-overlay px-6 pt-3 pb-6">
              {hasTypedItems || unresolved.length === 0 ? null : (
                <button
                  type="button"
                  className="btn-quiet"
                  disabled={unresolved.every((id) => leaveOut.includes(id))}
                  onClick={() => {
                    setLeaveOut(unresolved);
                  }}
                >
                  Leave out all unresolved items
                </button>
              )}
              {/* DRUGA DROGA ZE ZDANIA OBOK, jednym kliknięciem (2026-09-15).
                  Stopka obiecuje „Read each one, or take it out of the import", a do tego dnia
                  drugie dało się zrobić wyłącznie po jednej pozycji — przy trzynastu cudzych
                  umiejętnościach import nie odbywał się nigdy, bo wyłączony przycisk Import nie
                  wysyła nawet zdarzenia kliknięcia. Kontrolka stoi PRZY tym zdaniu i znika
                  razem z powodem: bez ani jednej nieprzeczytanej umiejętności nie miałaby co
                  wyrzucić (niezmiennik 16). Czego tu nie ma: przycisku „przeczytałem
                  wszystkie" — powód stoi przy `DROP_UNREAD`. */}
              {!hasTypedItems || unread.length === 0 ? null : (
                <button
                  type="button"
                  data-drop-unread
                  className="btn-quiet"
                  onClick={() => {
                    setExcludedItems((now) => [...now, ...unread]);
                    setDropped((now) => [...now, ...unread]);
                  }}
                >
                  {DROP_UNREAD}
                </button>
              )}
              <p className="lead">
                {saved !== null
                  ? 'Close this window when you have read what came over.'
                  : !hasItems
                    ? 'No setup files were found in this project.'
                    : blocked > 0
                      ? hasTypedItems
                        ? unread.length > 0
                          ? readingSays(unread.length)
                          : `${String(blocked)} item(s) still need attention.`
                        : `${String(blocked)} item(s) need a choice. Tick Skip to leave them out.`
                      : hasTypedItems
                        ? readySays(excludedItems.length, dropped.length, alreadyHere.length)
                        : leftOut === 0
                          ? 'Ready to import.'
                          : `Ready to import. ${String(leftOut)} item(s) will be left out.`}
              </p>
              <button
                type="button"
                data-import-now
                /* Plan wniesiony raz nie daje się wnieść drugi raz: `apply::preflight` odmawia
                   każdemu pliku, który już leży w bibliotece, więc żywy przycisk nad zapisanym
                   planem obiecuje robotę, która skończy się samą odmową. */
                disabled={busy || !hasItems || blocked > 0 || saved !== null}
                className="btn-primary ml-auto"
                onClick={apply}
              >
                Import
              </button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
