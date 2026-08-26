/* Szew ekranu agenta: skąd bierze się jego treść i kiedy ten ekran w ogóle istnieje.
 *
 * TU MIESZKA CAŁE „SKĄD", a `session.tsx` nie wie o magazynach nic. Podział jest ten sam, co
 * między `feed/model.ts` i `feed/feed.tsx`: sekcje liczy funkcja czysta, którą da się osądzić
 * bez okna (niezmiennik 15), a ten plik tylko podaje jej to, co okno naprawdę wie.
 *
 * DLACZEGO CZYTAMY MAGAZYNY SAMI, A NIE PROPSEM Z EKRANU PRACY. Ekran pracy (`../index.tsx`)
 * nie należy do tego zadania, a każdy nowy props na jego granicy jest zmianą w cudzym pliku.
 * Ważniejsze: „co jest w strumieniu" i „co jest w planie biegu" mają po jednym właścicielu
 * (`runFeed`, `useRun`), a komponent, który dostawałby je propsem od rodzica czytającego to samo,
 * byłby drugą drogą do tej samej odpowiedzi (niezmiennik 13). Jedyne, co przychodzi propsem, to
 * KAFELKI — bo ich policzenie należy do listy agentów i tam ma zostać.
 *
 * DLACZEGO `useSyncExternalStore` Z BIEŻĄCYM STANEM JAKO MIGAWKĄ SERWEROWĄ. `renderToStaticMarkup`
 * jest rendererem serwerowym, a zustand 5 podaje mu `getInitialState` — więc ekran czytany hakiem
 * pokazywałby stan Z CHWILI UTWORZENIA magazynu i nigdy tego, co do niego weszło. Ta aplikacja
 * nigdy nie hydratuje serwerowego HTML-a, więc powód, dla którego React chce tam stanu
 * początkowego, tutaj nie istnieje. Ten sam zapis stoi w `../index.tsx` i w `../start.tsx`.
 *
 * CZEGO OKNO NIE MA, więc czego ten ekran nie pokazuje (niezmiennik 17). Do 2026-08-23 stały tu
 * trzy braki i blok „co dostał" jechał przez to na trzech pustych listach — czyli mówił
 * „Nothing was given to this agent." w biegu, w którym ten agent dostał i pliki, i notatki.
 * Zostały dwa, oba nazwane, oba zgłoszone:
 *   brief kroku      `AgentStep.instructions` jest czytany z dysku i gubiony w `../choices.ts`
 *                    (`planOf` przepisuje `id`, `name`, `state` i nic więcej),
 *   który to bieg    okno NIGDY nie poznaje katalogu biegu: `run_workflow` oddaje `()`,
 *                    a `RunState` nie ma pola na identyfikator. Zakresem przekazań jest więc
 *                    FOLDER, nie bieg — plik zaadresowany do kroku o tej samej nazwie
 *                    w poprzednim biegu tego workflow stanie na tym ekranie. Zawężenie
 *                    wymaga pola na drucie, czyli pliku spoza tego zadania.
 * Wiersz zgadnięty wyglądałby dokładnie jak wiersz z danymi, więc żadnego z tych dwóch braków
 * nie zastępujemy domysłem.
 */
import { useEffect, useSyncExternalStore } from 'react';
import type { ReactElement } from 'react';

import { useRun } from '../../../state/run';
import { runStepAgain } from '../rail/again';
import type { Step } from '../../../state/run';
import type { Handoff as HandedOver, Note } from '../../../state/memory';
/* JEDYNA KRAWĘDŹ do tych dwóch komend w całej aplikacji (niezmiennik 23) — sekcja Pamięć
 * czyta z niej te same pliki i te same notatki. Drugie `invoke` z tej samej nazwy byłoby
 * drugim miejscem, które wie, jak nazywa się komenda. */
import { listHandoffs, listNotes } from '../../memory/io';
import { runFeed } from '../feed/live';
import type { FeedView } from '../feed/model';
import type { RailCard } from '../rail/card';
import { changesOf } from './changes';
import type { Handoff, StepBrief, UsedNote } from './layout';
import { sessionSections } from './layout';
import { closeAgent, openedAgent, subscribeToOpenAgent } from './open';
import { Session } from './session';

/* ─── CO TEN AGENT DOSTAŁ ──────────────────────────────────────────────────────────────────
 *
 * Fakty spoza strumienia leżą na dysku i mają po jednej istniejącej krawędzi do Rusta
 * (`sections/memory/io.ts`). Trzymamy je w magazynie NA POZIOMIE MODUŁU, nie w `useState`
 * ekranu, i powód jest ten sam, co przy `./open.ts`: to repo nie ma jsdom, więc żaden efekt
 * Reacta nie odpali się w kryterium. Odczyt, którego kryterium nie umie zawołać, jest kodem,
 * którego nikt nie osądzi — czyli tą samą rodziną, z której wzięły się kontrolki bez skutku
 * (niezmiennik 16). Tutaj kryterium woła to, co woła wejście w ekran agenta.
 */

/** Fakty spoza strumienia, z których powstaje blok „co dostał". */
export interface WhatWasGiven {
  /** Pliki, które kroki tego folderu zostawiły sobie nawzajem. */
  readonly passed: readonly HandedOver[];
  /** Notatki, które leżą na dysku — także te, których nikt nie wziął do użytku. */
  readonly known: readonly Note[];
}

/** Stan przed pierwszym odczytem. Jeden obiekt, żeby migawka miała stałą tożsamość. */
const NOTHING_YET: WhatWasGiven = { passed: [], known: [] };

let given: WhatWasGiven = NOTHING_YET;
const watchers = new Set<() => void>();

/** Ta sama migawka dla okna i dla renderu serwerowego. */
export function whatWasGiven(): WhatWasGiven {
  return given;
}

/** Powiadomienie o zmianie; kształt, którego chce `useSyncExternalStore`. */
export function subscribeToGiven(watch: () => void): () => void {
  watchers.add(watch);
  return () => {
    watchers.delete(watch);
  };
}

/**
 * Czyta z dysku to, co ten folder przekazywał, i notatki tej maszyny.
 *
 * DWA ODCZYTY, DWA OSOBNE `catch`, i to nie jest ostrożność na zapas — ten sam podział, co
 * w `state/memory.ts`: przekazania leżą w katalogach biegów, a notatki w `~/.loadout/memory`.
 * Jeden `try` na oba znaczy, że nieczytelny katalog notatek zabiera z ekranu także pliki, które
 * są w porządku. Odmowa NIE leci w górę: wołającym jest wejście w ekran agenta, a wyjątek
 * stamtąd wywraca ekran zamiast zostawić blok takim, jaki był.
 *
 * `folder` jest jedynym zakresem — ta sama umowa, co w `listRuns` i `listHandoffs`. `null`
 * jedzie jawnie, żeby Rust wziął swoją domyślną, zamiast żeby okno podstawiało drugą.
 */
export async function readWhatWasGiven(folder: string | null): Promise<void> {
  let passed = given.passed;
  let known = given.known;

  try {
    passed = await listHandoffs(folder);
  } catch {
    /* Lista pustoszeje z rozmysłem: przekazania sprzed odmowy są tym, co okno PAMIĘTA, a nie
     * tym, co leży w plikach (niezmiennik 4). */
    passed = [];
  }
  try {
    known = await listNotes(folder);
  } catch {
    known = [];
  }

  given = { passed, known };
  for (const watch of [...watchers]) watch();
}

/**
 * Ile ten plik waży, gotowe do przeczytania.
 *
 * Liczba, nigdy `—`: pomiar jest jedyną odpowiedzią na pytanie „czy poprzednik zostawił
 * research, czy dwa zdania", a wiersz zastępczy w tej samej siatce czyta się jak wiersz
 * z wartością (niezmiennik 17). Jedno miejsce po przecinku poniżej dziesięciu i żadnego wyżej:
 * `1.2 KB` niesie różnicę, `348.7 KB` niesie szum.
 */
function readableSize(bytes: number): string {
  if (bytes < 1024) return String(bytes) + ' B';
  const kb = bytes / 1024;
  if (kb < 1024) return (kb < 10 ? kb.toFixed(1) : String(Math.round(kb))) + ' KB';
  return (kb / 1024).toFixed(1) + ' MB';
}

/** Sama nazwa pliku. Ścieżka od korzenia projektu jest w wierszu szumem [makieta 511]. */
function fileName(path: string): string {
  return path.split('/').at(-1) ?? path;
}

/**
 * Przekazania widziane oczami TEGO agenta.
 *
 * Jedno przekazanie z drutu daje najwyżej jeden wiersz, bo `to` jest listą, a blok pyta
 * o jednego adresata: tego, którego ekran stoi otworem. Przekazanie, które ani do niego nie
 * przyszło, ani od niego nie wyszło, nie ma na tym ekranie czego robić.
 */
function handoffsFor(agent: string, passed: readonly HandedOver[]): readonly Handoff[] {
  const rows: Handoff[] = [];
  for (const one of passed) {
    const mine = one.to.includes(agent);
    if (!mine && one.from !== agent) continue;
    rows.push({
      from: one.from,
      to: mine ? agent : (one.to[0] ?? ''),
      file: fileName(one.path),
      summary: one.title,
      size: readableSize(one.bytes),
      /* Panel szczegółów jest osobną powierzchnią, której to repo nie ma — numer, którego nikt
       * nie odbiera, byłby wierszem o kształcie odnośnika prowadzącego donikąd. */
      detailId: null,
    });
  }
  return rows;
}

/**
 * Notatki, które naprawdę jadą w promptach tego biegu.
 *
 * `in-use` i tylko `in-use`: kandydatka, której człowiek nie wziął do użytku, nie wchodzi
 * w żaden prompt (`memory::notes::what_you_know`), więc w bloku faktów o tym, co model dostał,
 * byłaby zdaniem, którego model nigdy nie przeczytał.
 *
 * WŁAŚCICIEL JEST TYLKO PRZY ZAKRESIE `this-agent`. Notatka `everywhere` i `this-project` jedzie
 * w promptach wszystkich kroków, a pole `agent` bywa przy niej wypełnione nazwą autora — użyte
 * jako filtr, schowałoby ją przed każdym agentem poza jednym. Nazwę zakresu jednego agenta
 * normalizujemy tak samo jak Rust, bo plik pisze człowiek (`Forge`), a krok może nazywać się
 * `forge`; różnica pisowni nie zmienia odbiorcy promptu.
 *
 * `leftOut` jest bieżącym rachunkiem tego samego `what_you_know`: wiersz zostaje w katalogu,
 * lecz postawienie go tutaj twierdziłoby, że agent przeczytał zdanie odłożone przez limit.
 *
 * TO NIE JEST `run.json`, I TO JEST ZAPISANY DŁUG. Rachunek z pamięci JEDNEGO biegu
 * (`commands::run::MemoryRecord`) istnieje na dysku i nie ma nośnika na drucie: `read_run`
 * oddaje `PastRunWire` bez pola `memory`, a jego dołożenie jest zmianą w Ruście. Dopóki go nie
 * ma, jedyną prawdą o tym, co jedzie w promptach, jest zbiór notatek w użyciu — dla biegu,
 * który trwa, ten sam zbiór, który zamrożono na jego starcie.
 */
function agentKey(name: string): string {
  const key = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return key === '' ? 'agent' : key;
}

function notesFor(agent: string, known: readonly Note[]): readonly UsedNote[] {
  const recipient = agentKey(agent);
  return known
    .filter(
      (note) =>
        note.status === 'in-use' &&
        note.leftOut !== true &&
        (note.scope !== 'this-agent' ||
          (note.agent !== null && note.agent !== undefined && agentKey(note.agent) === recipient)),
    )
    .map((note) => ({
      // Lista jest już policzona dla tego ekranu. Podajemy jego identyfikator, żeby czysty
      // layout zachował swój filtr także wtedy, gdy plik i krok różnią się pisownią nazwy.
      agent: note.scope === 'this-agent' ? agent : '',
      /* `rule` jest jedyną częścią notatki, która jedzie do modelu — tytuł i uzasadnienie
       * zostają w pliku. Wiersz ma mówić to, co przeczytał agent. */
      text: note.rule,
      detailId: null,
    }));
}

export interface AgentScreenProps {
  /** Kafelki listy agentów — jedno źródło nazwy, roli, koloru i stanu (niezmiennik 13). */
  readonly cards: readonly RailCard[];
  /** Zdanie dla czlowieka po powtorzeniu kroku. Brak propsu = ten ekran nie umie go pokazac. */
  readonly onSaid?: (text: string) => void;
}

/* Ta sama migawka dla okna i dla renderu serwerowego. Model nie ma stanu „po stronie serwera":
 * `renderToStaticMarkup` widzi po prostu bieg, którego jeszcze nie ma. */
function currentView(): FeedView {
  return runFeed.view;
}

/**
 * Kroki, na których stoi TEN agent.
 *
 * PODPIS AGENTA W STRUMIENIU TO NAZWA KROKU i nie jest to domysł: pompa zdarzeń startuje jako
 * `forward(…, self.plan.steps[id].name.clone())` (`src-tauri/src/commands/run.rs`), więc pole
 * `agent` każdej linii niesie nazwę kroku. To samo dopasowanie robi lista agentów
 * (`../index.tsx`, `factsOf`) — jedna reguła, dwa miejsca odczytu, żadnej drugiej definicji.
 */
function stepsOf(steps: readonly Step[], agent: string): readonly StepBrief[] {
  return steps
    .filter((step) => step.name === agent)
    .map((step) => ({ agent: step.name, name: step.name, brief: '', files: [] }));
}

/**
 * Ekran otwartego agenta — albo `null`, kiedy żaden nie jest otwarty.
 *
 * `null` także wtedy, gdy otwarty podpis nie ma kafelka w tym zakresie: kafelek istnieje wtedy
 * i tylko wtedy, gdy agent pojawił się w strumieniu TEGO workspace'a, więc po przełączeniu
 * zakresu ekran cudzego agenta gaśnie sam. Identyfikator zostaje zapamiętany, żeby powrót do
 * tamtego folderu wrócił do tego samego agenta — sesji się nie traci.
 */
export function AgentScreen({ cards, onSaid }: AgentScreenProps): ReactElement | null {
  const opened = useSyncExternalStore(subscribeToOpenAgent, openedAgent, openedAgent);
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);
  const run = useSyncExternalStore(useRun.subscribe, useRun.getState, useRun.getState);
  const given = useSyncExternalStore(subscribeToGiven, whatWasGiven, whatWasGiven);

  /* PYTAMY DYSK DOPIERO PRZY OTWARTYM AGENCIE. Ten komponent stoi zamontowany przez cały czas
   * życia widoku pracy i przez większość tego czasu rysuje `null` — odczyt bezwarunkowy byłby
   * dwoma przejściami granicy na każde wejście w sekcję Bieg, po nic. Folder jest w zależnościach,
   * bo przełączenie zakresu zmienia odpowiedź na oba pytania. */
  const folder = run.folder;
  useEffect(() => {
    if (opened === null) return;
    void readWhatWasGiven(folder);
  }, [opened, folder]);

  const card = cards.find((one) => one.id === opened);
  if (card === undefined) return null;

  const sections = sessionSections(
    { id: card.id, name: card.name },
    {
      view,
      steps: stepsOf(run.steps, card.id),
      handoffs: handoffsFor(card.id, given.passed),
      changes: changesOf(run.lines, card.id),
      notes: notesFor(card.id, given.known),
    },
  );

  /* Powtórzenie dostaje wyłącznie krok, który JEST w grafie: pod-agent rozpuszczony w trakcie
   * biegu nie ma czego powtórzyć, więc jego ekran nie dostaje przycisku. */
  const step = card.stepId;

  return (
    <Session
      card={card}
      sections={sections}
      onBack={closeAgent}
      onToggle={runFeed.toggle}
      {...(step === null || step === undefined || onSaid === undefined
        ? {}
        : {
            onRunAgain: () => {
              runStepAgain(step, onSaid);
            },
          })}
    />
  );
}
