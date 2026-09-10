/* Jedyne miejsce w sekcji Agenci, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, krawędź po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` rozsiany po magazynie i po formularzu. Magazyn
 * (`src/state/agents.ts`) dostaje ten moduł WSTRZYKNIĘTY jako `AgentsIo` i dzięki temu jego
 * testy podstawiają atrapę bez dotykania transportu. Zdanie „edycja kroku nie zapisuje agenta"
 * ma sens tylko wtedy, kiedy jest jedna krawędź, przez którą da się zapisać cokolwiek — dwie
 * drogi do Rusta znaczą, że asercja pilnuje jednej z nich, a zapis jedzie drugą.
 *
 * Kształt jest lustrem `AgentsIo` z `src/state/agents.ts` i tak ma zostać: funkcja dopisana
 * tutaj bez pozycji tam jest funkcją, której magazyn nie umie zawołać.
 *
 * 2026-08-16 — ciała wypełnia T-27. Nazwy komend są dosłownie te z `src-tauri/commands.golden.txt`
 * i muszą takie zostać: ten sam plik czyta po drugiej stronie granicy `ipc_commands_registered.rs`,
 * więc nazwa sklejona tutaj ze zmiennej albo przepisana z pamięci rozjeżdża się w ciszy —
 * `invoke` na nieistniejącą komendę odmawia dopiero pod palcem użytkownika.
 */
import { invoke } from '@tauri-apps/api/core';
import { activeWorkspace } from '../../state/workspaces';
import type { AgentsIo } from '../../state/agents';

import type { Agent } from '../../state/agents';
import type { Definition } from '../../state/library';
import { definitionsOf, healthyOnly } from '../../state/library';

/**
 * Czym uzupelniamy agenta, ktoremu za granica zabraklo klucza — 2026-08-31 wieczorem.
 *
 * KAZDA z tych wartosci jest ta sama, ktora `blankAgent` w `./index.tsx` daje nowej roli, i to
 * nie jest przypadek: to sa te same odpowiedzi na te same pytania. Najwezszy dostep do plikow,
 * bo prawo do zmieniania plikow ma dawac czlowiek; `look-only` jest tu wiec wartoscia bezpieczna,
 * a nie wygodna.
 *
 * NIE SA TO DANE — sa to wartosci, ktorych brak przewraca ekran. Agent z pustym `model` zapisze
 * sie na dysk z pustym `model`, i to jest widoczne w polu, ktore czlowiek moze wypelnic. Agent
 * bez `model` w OGOLE wywraca render.
 */
const FILLED = {
  schema: 1,
  name: '',
  summary: '',
  color: 'slate',
  instructions: '',
  runsWith: 'claude-code',
  model: '',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 30,
  tools: 'everything',
  reachesTheWeb: true,
  skills: [],
  connections: [],
  writeResultsTo: '',
} as const satisfies Omit<Agent, 'id'>;

/**
 * Agent, ktoremu za granica zabraklo klucza, dostaje wartosc zamiast `undefined`.
 *
 * WADA, ZMIERZONA 2026-08-31 rano: `roleWords(agent.instructions)` przewracalo CALY ekran
 * Agents (`TypeError: Cannot read properties of undefined (reading 'replace')`), a granica
 * bledu zamieniala go w pusty prostokat — czyli ekran, ktory dla czlowieka wyglada na „nic tu
 * nie ma", a naprawde sie wywrocil.
 *
 * TA SAMA WADA WROCILA TEGO SAMEGO DNIA WIECZOREM, i to jest powod, dla ktorego ta funkcja
 * uzupelnia dzis CALY ksztalt, a nie jedno pole. Do wieczora ekran wstawal jako sciana kafelkow,
 * wiec `AgentForm` montowal sie WYLACZNIE po kliknieciu; od zmiany ukladu rola stoi w ciele
 * ekranu od pierwszej klatki, wiec formularz czyta pierwszego agenta ZAWSZE. Zmierzone
 * w prawdziwym chromium podczas biegu e2e:
 *
 *   TypeError: Cannot read properties of undefined (reading 'trim')
 *     at AgentForm (src/sections/agents/agent-form.tsx) — `value.model.trim()`
 *
 * Naprawa jest TUTAJ, a nie u czytelnikow pol: gdyby kazdy bronil sie sam, nastepny by
 * zapomnial i ta sama wada wrocilaby tym samym wejsciem po raz trzeci (niezmiennik 13).
 * Jedna droga, ktora zapisani agenci wchodza do aplikacji, jest jednym miejscem na jej prostowanie.
 *
 * ZLACZENIE JEST W TE STRONE, w ktora jest: `{ ...FILLED, ...one }` zostawia KAZDA wartosc,
 * ktora naprawde przyszla — takze `''` i `false` — a uzupelnia wylacznie brakujace klucze.
 * Odwrotna kolejnosc nadpisywalaby dysk naszymi domyslnymi i byla trzecia wersja tej wady.
 */
function whole(one: Definition<Agent> | Agent): Definition<Agent> | Agent {
  /* DWA KSZTALTY, NIE JEDEN. `DefinitionListing` dopuszcza obok opakowanej definicji takze
   * GOLA wartosc — tak odpowiadaja wstrzykiwane atrapy i tak odpowiada granica e2e. Wersja
   * pytajaca wylacznie o `kind === 'healthy'` przepuszczala gola wartosc nietknieta, wiec
   * naprawa dzialala w vitest i NIE dzialala w przegladarce: ekran Agents dalej padal. */
  if (!('kind' in one)) return { ...FILLED, ...one };
  if (one.kind !== 'healthy') return one;
  return { ...one, value: { ...FILLED, ...one.value } };
}

/**
 * Wszyscy zapisani agenci, po jednym na plik w bibliotece — i kazdy w PELNYM ksztalcie.
 *
 * Powod, dwie wady i kierunek zlaczenia stoja przy [`whole`] wyzej.
 */
export async function listDefinitions(
  folder = activeWorkspace()?.folder ?? null,
): Promise<Definition<Agent>[]> {
  const listed = await invoke<(Definition<Agent> | Agent)[]>('list_agents', { folder });
  return listed.map(whole) as Definition<Agent>[];
}

/** Callery poza ekranem Agents potrzebują tylko zdrowych zapisanych agentów. */
export async function list(folder = activeWorkspace()?.folder ?? null): Promise<Agent[]> {
  return healthyOnly(definitionsOf(await listDefinitions(folder)));
}

/**
 * Świeży identyfikator, wybity po stronie Rusta.
 *
 * Nie `crypto.randomUUID()`: tamto daje v4, czyli liczbę losową, a tutaj musi być v7 —
 * sortowalne po czasie [T4 §5.1]. Mennica stoi tam, gdzie v7 już jest.
 */
export function newId(): Promise<string> {
  return invoke<string>('new_id');
}

/**
 * Zapisuje definicję agenta i oddaje rewizję, którą ma teraz jego plik. Duplikat to nowy PLIK,
 * nie wiersz żyjący na ekranie.
 *
 * `expectedRevision` to rewizja, którą okno CZYTAŁO dla tego agenta; `null` znaczy „tego pliku
 * ma jeszcze nie być". Klucz jedzie zawsze, nawet z `null` w środku — Tauri dopasowuje
 * argumenty po nazwie, więc klucz zdjęty przez `JSON.stringify` byłby wywołaniem ODRZUCONYM.
 */
export function save(
  agent: Agent,
  expectedRevision: string | null,
  folder = activeWorkspace()?.folder ?? null,
): Promise<string> {
  return invoke<string>('save_agent', { agent, expectedRevision, folder });
}

/**
 * Rewizja pliku tego agenta, prosto z biblioteki — albo `null`, kiedy jej tam nie ma.
 *
 * PO CO TO ISTNIEJE. Edytor workflow też zapisuje agenta (naprawa „Save to the agent"
 * w `state/workflows.ts`), a rewizji pliku nie trzyma i trzymać nie powinien: to nie jest fakt
 * o otwartym workflow. Zamiast przewlekać ją przez płótno i trzy panele, czytamy bibliotekę
 * tuż przed zapisem — dokładnie tak, jak magazyn listy czyta katalog tuż przed wyborem wolnej
 * nazwy pliku. Okno między odczytem a zapisem zamyka i tak Rust: to on porównuje bajty.
 */
export async function revisionOf(
  id: string,
  folder = activeWorkspace()?.folder ?? null,
): Promise<string | null> {
  const found = definitionsOf(await listDefinitions(folder)).find(
    (definition) => definition.kind === 'healthy' && definition.value.id === id,
  );
  return found?.kind === 'healthy' ? (found.revision ?? null) : null;
}

/** Usuwa agenta po identyfikatorze — stabilnym przez zmianę nazwy, w odróżnieniu od pliku. */
export function remove(id: string, folder = activeWorkspace()?.folder ?? null): Promise<void> {
  return invoke<void>('delete_agent', { id, folder });
}

/** Co wrócilo z generowania: szkic i wszystko, czego w nim nie ma. */
export interface GeneratedDraft {
  readonly operation: string;
  readonly agent: Agent;
  /** Czego generator musial sie domyslic z opisu. */
  readonly assumptions: readonly string[];
  /** Krotkie uzasadnienia istotnych ustawien — po co, nie jak myslal. */
  readonly because: readonly string[];
  /** Mozliwosci, o ktore poprosil, a ktorych tu nie ma. Widoczne PRZED zapisem. */
  readonly missing: readonly string[];
  /** Co odrzucono przy dopasowaniu, po jednym zdaniu. */
  readonly refused: readonly string[];
}

/**
 * Prosi wybranego vendora o napisanie szkicu agenta.
 *
 * `operation` jest identyfikatorem TEJ proby: po nim idzie anulowanie i po nim poznaje sie
 * spozniona odpowiedz, ktora nie ma prawa nadpisac nowszego szkicu.
 */
export function generate(
  operation: string,
  described: string,
  runsWith: Agent['runsWith'],
  folder = activeWorkspace()?.folder ?? null,
): Promise<GeneratedDraft> {
  return invoke<GeneratedDraft>('generate_agent', { operation, described, runsWith, folder });
}

/** Zatrzymuje JEDNO generowanie. Bieg workflow sie o tym nie dowiaduje. */
export function stopGenerating(operation: string): Promise<void> {
  return invoke<void>('stop_generating_agent', { operation });
}

/** Każdy zapis wraca do projektu, w którym otwarto edytor, także po przełączeniu. */
export function forProject(folder: string | null): AgentsIo {
  return {
    list: () => listDefinitions(folder),
    newId,
    save: (agent, revision) => save(agent, revision, folder),
    remove: (id) => remove(id, folder),
  };
}
