/* PIERWSZE OTWARCIE: droga, powitanie, trzej gotowi agenci i wiersz klawiszy.
 *
 * ── CO BYŁO I DLACZEGO WŁAŚCICIEL ODRZUCIŁ TO DWA RAZY ─────────────────────────────────────
 *
 * ZMIERZONE 2026-08-31 na tej gałęzi. Ten plik rysował trzy wiersze tekstu w `<ol>` i nic poza
 * nimi: bez tytułu ekranu, bez zdania o tym, czym ta aplikacja jest, bez ani jednego gotowego
 * agenta, bez licznika drogi, bez wyjścia dla kogoś, kto zna drogę, i bez jednego klawisza.
 * Największy stopień na całym ekranie miał 13 px (`--text-ui`, na przycisku pierwszego kroku),
 * bo drabinka typograficzna kończyła się wtedy na 20 px. Właściciel powiedział o tym dwa razy
 * to samo: „nudne", „UX totalnie nieoczywisty".
 *
 * PRZYCZYNA JEST ZMIERZONA, NIE ESTETYCZNA. Przy suficie 20 px żaden ekran NIE MOŻE mieć
 * bohatera — da się zrobić rzecz grubszą albo szerszą, nigdy większą — więc każda próba
 * hierarchii kończy się szarym prostokątem obok szarego prostokąta. Drabinka sięga od
 * 2026-08-31 do 40 px (`--text-display`) i ten ekran jest jej pierwszym i jedynym wołającym.
 *
 * ── CZTERY RZECZY, KAŻDA ODPOWIADA NA INNE PYTANIE ─────────────────────────────────────────
 *
 *   droga        gdzie jestem i ile zostało      liczona Z DYSKU, nie z licznika w oknie
 *   powitanie    czym to jest i co nacisnąć      jeden tytuł, jedno zdanie, JEDEN akcent
 *   galeria      czy muszę pisać od zera         nie: trzy pliki, jedno kliknięcie
 *   klawisze     czy da się szybciej             tak, i wymienione są tylko te, które działają
 *
 * DROGA MA TRZY PRZYSTANKI, BO TYLE ICH NAPRAWDĘ JEST: bieg potrzebuje kogoś, kto pracuje
 * (agent), kolejności, w jakiej pracują (workflow), i miejsca, w którym pracują (folder).
 * Każdy z nich ma odpowiedź Z DYSKU — liczby wchodzą do [`firstRunSteps`] z tych samych trzech
 * list, które czytają sekcje. Stan przystanku jest WYLICZANY, nigdy zapamiętywany: zapamiętane
 * „już to zrobiłeś" rozjeżdża się z rzeczywistością przy pierwszym skasowanym pliku i wtedy
 * przewodnik mówi o świecie, którego nie ma.
 *
 * TRZECI PRZYSTANEK NOSI TYTUŁ Z MAKIETY („Run it and watch") i ODPOWIADA ZA FOLDER — jego
 * druga linia mówi to wprost („in one folder on this Mac"). Makieta nie rysuje folderu wcale,
 * bo jej trzeci przystanek odhacza się dopiero po biegu; u nas nie może, i to jest zmierzone:
 * `./index.tsx` przestaje rysować przewodnik dokładnie wtedy, gdy nic nie zostało, więc
 * przystanek odhaczany po biegu trzymałby cały ten ekran NAD strumieniem pierwszego biegu
 * i zabierałby `./ready.tsx` jego jedyną scenę. Folder jest ostatnią rzeczą, której brakuje do
 * naciśnięcia Run, więc stoi na trzecim miejscu i mówi o sobie prawdę.
 *
 * ── DLACZEGO ZAPROSZENIE DO FOLDERU MA DWA MIEJSCA, A ZNACZNIK JEDNO ───────────────────────
 *
 * Bez folderu nie ma gdzie pracować, więc zaproszenie musi stać na ekranie ZAWSZE, dopóki
 * folderu nie ma — także wtedy, gdy bieżącym krokiem jest agent. Stoi więc w tym przystanku,
 * którego dotyczy, a kiedy TEN przystanek jest bieżący, przenosi się do powitania, czyli tam,
 * gdzie i tak stoi jedyna głośna kontrolka ekranu. Znacznik `data-add-workspace` jedzie razem
 * z nim i jest DOKŁADNIE JEDEN: liczy go `e2e/tests/plus-opens-a-terminal.spec.ts` i kolektor
 * gęstości (`scripts/density-collect.mjs`, `inviteIsUp`), a dwa zaproszenia do tej samej
 * rzeczy każą człowiekowi zgadywać, które jest prawdziwe.
 *
 * ── CO TEN PLIK ŚWIADOMIE ROBI INACZEJ NIŻ MAKIETA, I DLACZEGO ─────────────────────────────
 *
 * `⌘N` z makiety w tej aplikacji NIE ISTNIEJE i nie jest tu rysowany. `⌘K` i `⌘1`–`⌘7`
 * istnieją i mieszkają w jednym miejscu (`src/ui/palette/keys.ts`, `moveFor`/`jumpForNumber`),
 * więc ekran pyta o nie tamtą funkcję, zamiast wpisywać numer z palca — druga klawiatura
 * dopisana tutaj byłaby drugim miejscem, w którym rozstrzyga się skok do sekcji
 * (niezmiennik 13). Klawisz narysowany obok przycisku jest obietnicą, a obietnica, której nikt
 * nie dotrzymuje, jest tą samą wadą co przycisk bez handlera (niezmiennik 16): kryterium
 * `first-open-is-a-door.test.tsx` naciska KAŻDY klawisz z tego ekranu w `moveFor` i żąda, żeby
 * okno na niego odpowiedziało.
 */
import type { ReactElement, ReactNode } from 'react';
import { Fragment, useEffect, useSyncExternalStore } from 'react';

import { useSectionStore } from '../../ui/shell/section-store';
/* NAPIS ZAPROSZENIA DO FOLDERU JEDZIE ZE STAŁEJ PRZEŁĄCZNIKA, nie z literału tutaj: „dodaj
 * zakres" ma w całej aplikacji jedno brzmienie, a dwie kopie tego samego zdania rozjeżdżają się
 * przy pierwszej zmianie i wtedy odmowa odsyła do przycisku o innej nazwie (niezmiennik 13). */
import { FIRST_INVITE } from '../../ui/shell/workspace-switcher';
import { guidanceIsWanted, stepAside, subscribeToGuidance } from './guidance';
import {
  STARTERS,
  runsOn,
  subscribeToTaking,
  takingAnAgent,
  whatItMayDo,
  type Starter,
} from './starters';
import { welcomeFor } from './welcome';

/** Krok zrobiony, krok do zrobienia teraz, krok, który poczeka. */
export type FirstRunState = 'done' | 'now' | 'later';

export interface FirstRunStep {
  /** Czego ten krok dotyczy — po tym poznaje go kryterium i po tym wybiera się czynność. */
  readonly id: 'agent' | 'workflow' | 'workspace';
  /** Zdanie na ekranie: tryb rozkazujący, bez kropki (DESIGN §6). */
  readonly title: string;
  /** Druga linia przystanku — co ten krok naprawdę znaczy, w pięciu słowach. */
  readonly note: string;
  readonly state: FirstRunState;
}

/** Co naprawdę leży na dysku — trzy liczby, każda z listy, którą czyta jakaś sekcja. */
export interface WhatIsThere {
  readonly workspaces: number;
  readonly agents: number;
  readonly workflows: number;
}

/** Kolejność jest częścią odpowiedzi: bez agenta nie ma kogo ustawić w rząd. */
const ORDER = [
  { id: 'agent', title: 'Make an agent', note: 'one job, one instruction' },
  { id: 'workflow', title: 'Put agents in a row', note: 'that row is a workflow' },
  { id: 'workspace', title: 'Run it and watch', note: 'in one folder on this Mac' },
] as const satisfies readonly { id: FirstRunStep['id']; title: string; note: string }[];

/**
 * Trzy kroki ze stanami — pierwszy niezrobiony jest bieżący, reszta czeka.
 *
 * DOKŁADNIE JEDEN krok jest bieżący, dopóki cokolwiek zostało: dwa akcenty naraz znaczą, że
 * człowiek ma wybrać, od czego zacząć, a to jest pytanie, którego pierwsze uruchomienie zadawać
 * nie ma prawa. Kiedy wszystko już leży, żaden nie jest bieżący i wołający tej listy nie rysuje
 * — przewodnik nad kompletem to trzy odhaczone wiersze zajmujące strefę pracy.
 */
export function firstRunSteps(there: WhatIsThere): readonly FirstRunStep[] {
  const done: Record<FirstRunStep['id'], boolean> = {
    agent: there.agents > 0,
    workflow: there.workflows > 0,
    workspace: there.workspaces > 0,
  };
  let lit = false;
  return ORDER.map((step) => {
    if (done[step.id]) return { ...step, state: 'done' as const };
    if (lit) return { ...step, state: 'later' as const };
    lit = true;
    return { ...step, state: 'now' as const };
  });
}

/** Czy z tej listy zostało cokolwiek do zrobienia — czyli czy przewodnik ma się w ogóle pokazać. */
export function somethingIsLeft(steps: readonly FirstRunStep[]): boolean {
  return steps.some((step) => step.state === 'now');
}

/**
 * Co ekran biegu ma do narysowania w obszarze pracy poza powitaniem — cztery liczby, każda
 * z innego źródła i każda z osobna wystarczająca, żeby kolumna kroków miała po co stać.
 */
export interface WhatTheWorkAreaHas {
  /** Kroki obrazu: z magazynu biegu, a kiedy ten milczy — z pliku workflow, który ruszy. */
  readonly steps: number;
  /** Rzeczy uruchomione komendą. Ich kafelki stoją w kolumnie kroków, nie w strumieniu. */
  readonly started: number;
  /** Wiersze, które już padły w strumieniu. */
  readonly lines: number;
  /** Ilu agentów mówi TERAZ. */
  readonly live: number;
}

/**
 * Czy powitanie JEST tym ekranem — czyli czy obszar pracy nie ma nic poza nim.
 *
 * ZGŁOSZENIE, Z KTÓREGO TO POWSTAŁO (właściciel, 2026-08-31, zrzut okna 1512×950). Powitanie
 * rysowało się WEWNĄTRZ kolumny strumienia, obok pustej kolumny kroków szerokiej na 376 px:
 * potrzebowało 1118 px, dostawało 802 i przelewało nadmiar aż do `main`, który stał wtedy
 * przewinięty w prawo. Pierwsza kontrolka pierwszego ekranu czytała się „…agents saved yet".
 *
 * PRZYCZYNA NIE JEST SZEROKOŚCIĄ POWITANIA, tylko tym, że stało ono w torze `1fr` siatki
 * dwukolumnowej, której druga kolumna była pusta. Zaklejenie tego przez `overflow` schowałoby
 * przelanie, zostawiając uciętą kontrolkę — więc rozstrzyga to UKŁAD: dopóki nie ma ani kroku,
 * ani rzeczy uruchomionej komendą, ani jednego wiersza w strumieniu, ekran nie buduje siatki
 * pracy w ogóle i oddaje powitaniu całą taflę.
 *
 * WARUNEK JEST SZERSZY NIŻ „ZERO AGENTÓW I ZERO WORKFLOW", i to jest konieczne: człowiek
 * z jednym zapisanym agentem i bez workflow widzi DOKŁADNIE to samo powitanie, tej samej
 * szerokości, w tej samej kolumnie. Pyta więc o to, czy przewodnik jest tym, co widać
 * ([`somethingIsLeft`]), a nie o którąś z liczb, z których się liczy.
 *
 * KAŻDA Z CZTERECH LICZB ODDZIELNIE PRZYWRACA SIATKĘ, bo każda z nich znaczy „w kolumnie
 * kroków albo w strumieniu jest już co pokazać": pierwsza rzecz uruchomiona komendą wraca
 * z kafelkiem do kolumny kroków, a pierwszy wiersz strumienia zdejmuje powitanie z ekranu
 * (`./feed/feed.tsx`, `nothingYet`) — układ bez tych warunków zostawiłby po nim jedną kolumnę.
 */
export function welcomeIsTheWholeScreen(
  steps: readonly FirstRunStep[],
  has: WhatTheWorkAreaHas,
): boolean {
  return (
    somethingIsLeft(steps) &&
    has.steps === 0 &&
    has.started === 0 &&
    has.lines === 0 &&
    has.live === 0
  );
}

/* DWIE CZYNNOŚCI STOJĄ W MODULE, NIE W KOMPONENCIE, i to jest ta sama decyzja, co
 * w `workspace-switcher.tsx`: repo nie ma jsdom, więc kliknięcia nie da się odpalić w teście,
 * a `renderToStaticMarkup` nigdy nie woła `onClick`. Handler zamknięty w komponencie byłby
 * kodem, którego żadne kryterium nie umie dotknąć — czyli tą samą martwą kontrolką, przed którą
 * stoi niezmiennik 16. Tutaj kryterium woła dokładnie to, co woła przycisk. */

/** Zabiera na ekran Agents — tam, gdzie agenta się pisze. */
export function openAgents(): void {
  useSectionStore.getState().go('agents');
}

/** Zabiera na ekran Workflows — tam, gdzie składa się kolejność pracy. */
export function openWorkflows(): void {
  useSectionStore.getState().go('workflows');
}

/**
 * Co znaczy naciśnięcie klawisza, kiedy na ekranie stoi przewodnik.
 *
 * FUNKCJA CZYSTA, poza komponentem, i to jest ten sam powód, co wyżej: nasłuch klawiatury
 * mieszka w efekcie, którego `renderToStaticMarkup` nie uruchamia, więc polityka zamknięta
 * w jego wnętrzu byłaby nietykalna dla wyroczni. To jest ten sam kształt, co `moveFor`
 * w `src/ui/palette/keys.ts`.
 *
 * `Escape` I NIC POZA NIM. Skrót z modyfikatorem byłby drugą klawiaturą obok tamtej
 * (niezmiennik 13); `Escape` nie należy do żadnej, bo znaczy „cofnij to, co jest na wierzchu",
 * a przewodnik jest na wierzchu strefy pracy. Naciśnięcie już przez kogoś obsłużone
 * (`defaultPrevented`) nie należy do nas — zamknięta paleta oddaje `Escape` i przewodnik nie
 * ma prawa znikać przy okazji.
 */
export function guidanceHears(key: string, alreadyTaken: boolean): 'step aside' | 'nothing' {
  if (alreadyTaken) return 'nothing';
  return key === 'Escape' ? 'step aside' : 'nothing';
}

export interface FirstRunProps {
  readonly steps: readonly FirstRunStep[];
  /**
   * Pytanie o pierwszy folder. Wchodzi propsem, bo droga do dysku i zdanie odmowy należą do
   * ekranu Run (`index.tsx`, `openFolder`), a nie do tego bloku — druga kopia tego wywołania
   * byłaby drugim miejscem, z którego bierze się zakres (niezmiennik 23).
   */
  readonly onAddWorkspace: () => void;
}

/* KLAWISZ NA EKRANIE. Trzeci zapis tego samego kształtu w drzewie (`entry/entry.tsx:906`,
 * `ui/shell/titlebar.tsx:345`) i zapisany dług: `kbd` należy się warstwie `components`
 * w `theme.css`, obok `.chip` i `.btn`. Nie robię tego tutaj, bo tamten plik jest w tej fali
 * w rękach innego zadania, a trzecia kopia znika jedną linią, kiedy klasa powstanie. */
const KEY_CAP =
  'rounded-sm border border-line bg-hover px-[5px] py-[3px] font-mono text-meta leading-none text-muted';

/* TEN SAM KLAWISZ NA WYPEŁNIENIU AKCENTEM. Zmierzone na zrzucie prawdziwego okna 1512×950:
 * `--color-muted` na `--color-hover` położonym NA akcencie daje pigułkę, w której nie widać
 * znaku — czyli klawisz, który wygląda na zepsuty, zamiast mówić, co nacisnąć. Makieta
 * rozstrzyga to tak samo (`.btn.pri kbd`): przyciemnienie zamiast rozjaśnienia i tekst prawie
 * biały. Barwy idą przez `color-mix` na tokenach, nie literałem. */
const KEY_CAP_ON_ACCENT = 'rounded-sm border px-[5px] py-[3px] font-mono text-meta leading-none';

const ON_ACCENT = {
  background: 'color-mix(in srgb, var(--color-bg) 26%, transparent)',
  borderColor: 'color-mix(in srgb, var(--color-ink) 28%, transparent)',
  color: 'color-mix(in srgb, var(--color-ink) 88%, transparent)',
};

/* BARWA MÓWI, CO WOLNO — jedna mapa na chip i na twarz agenta, obie w jednej karcie, więc to
 * jest jeden fakt narysowany dwa razy w jednym miejscu, a nie dwa fakty (niezmiennik 13).
 *
 * DLACZEGO STYLEM, A NIE `data-tone`. `.chip[data-tone=…]` w `theme.css` zna cztery stany
 * i akcent — a to są STANY („teraz", „twoja kolej", „zepsute", „zrobił to człowiek"). To, co
 * agentowi wolno, stanem nie jest ani przez chwilę, więc nie ma prawa wziąć nazwy stanu.
 * `--sky` jest w tokenach od 2026-08-31 dokładnie po to (theme.css, „tozsamosc, czlon
 * nasycony"), a rozdziela je FORMA, nie nasycenie: stan maluje wiersz i jego lewą krawędź,
 * tożsamość wyłącznie pigułkę i kwadrat z glifem (DESIGN §3). */
const MAY: Readonly<Record<string, string>> = {
  'reads only': 'var(--color-sky)',
  'edits files': 'var(--color-accent)',
  'runs commands': 'var(--color-live)',
};

function tint(what: string): string {
  return MAY[what] ?? 'var(--color-muted)';
}

/**
 * Glif twarzy agenta. Obrys, `currentColor`, ta sama gramatyka co glify nawigacji.
 *
 * RYSOWANY Z TEGO, CO AGENTOWI WOLNO, a nie z jego nazwy — czyli z tego samego faktu, co barwa
 * i pigułka obok. Glif wybierany po nazwie cicho zmienia się w zły rysunek w dniu, w którym
 * ktoś przemianuje agenta, i nic tego nie zgłasza: `Scout` przemianowany na `Reader` dostawał
 * wtedy ptaszek Needle'a. Lupa znaczy „czyta", strzałki „pisze kod", ptaszek „uruchamia
 * sprawdzenia" — i każde z tych trzech wynika z pól definicji (niezmiennik 17).
 */
function faceOf(may: string): ReactNode {
  if (may === 'reads only') {
    return (
      <>
        <circle cx="9.5" cy="9.5" r="5.4" />
        <path d="M13.6 13.6 L19 19" />
      </>
    );
  }
  if (may === 'edits files') {
    return <path d="M8.6 6.4 L4 11.5 L8.6 16.6 M14.4 6.4 L19 11.5 L14.4 16.6" />;
  }
  return (
    <>
      <rect x="4.2" y="4.2" width="14.6" height="14.6" rx="3.6" />
      <path d="M8 11.6 L10.8 14.4 L15.4 8.8" />
    </>
  );
}

/** Karta gotowego agenta — cztery fakty z definicji i jedno zdanie o tym, co przycisk zrobi. */
function ReadyMade({ one, busy }: { readonly one: Starter; readonly busy: boolean }): ReactElement {
  const may = whatItMayDo(one.agent);
  const colour = tint(may);
  return (
    <button
      type="button"
      data-starter={one.agent.name}
      disabled={busy}
      onClick={one.take}
      className="group flex flex-col rounded-md border border-line bg-raised p-[15px] text-left transition-colors hover:border-line-strong hover:bg-hover disabled:cursor-not-allowed disabled:opacity-40"
    >
      <span
        aria-hidden
        className="grid size-10 place-items-center rounded-md"
        style={{
          color: colour,
          background: `color-mix(in srgb, ${colour} 14%, transparent)`,
          border: `1px solid color-mix(in srgb, ${colour} 34%, transparent)`,
        }}
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="size-[22px]"
        >
          {faceOf(may)}
        </svg>
      </span>
      <span className="mt-3 block text-subhead text-ink">{one.agent.name}</span>
      {/* Zdanie o roli jest tym samym `summary`, które pojedzie do pliku — karta nie opowiada
          o agencie własnymi słowami (niezmiennik 13). */}
      <span className="mt-1 block min-h-[35px] text-note text-body">{one.agent.summary}</span>
      <span className="mt-3 mb-3 flex flex-wrap items-center gap-2">
        <span
          className="chip"
          style={{
            color: 'var(--color-ink)',
            background: `color-mix(in srgb, ${colour} 13%, transparent)`,
            borderColor: `color-mix(in srgb, ${colour} 32%, transparent)`,
          }}
        >
          <span
            aria-hidden
            className="size-1.5 rounded-pill"
            style={{ background: colour, boxShadow: `0 0 9px ${colour}` }}
          />
          {may}
        </span>
        <span className="chip font-mono text-meta">{runsOn(one.agent)}</span>
      </span>
      {/* `mt-auto` DOSUWA TEN WIERSZ DO DOŁU KARTY, i to nie jest kosmetyka. Zmierzone na
          zrzucie prawdziwego okna 1512×950: pigułka z nazwą aplikacji łamie się na drugą linię
          w dwóch kartach z trzech, więc trzy zdania „Use this agent" stały na trzech różnych
          wysokościach — a rząd kart, w którym to samo zdanie skacze, czyta się jak trzy różne
          rzeczy. Karty są równej wysokości z siatki, więc wystarczy oddać im resztę miejsca. */}
      <span className="mt-auto flex items-center justify-between border-t border-line pt-3 text-note text-muted transition-colors group-hover:text-ink">
        Use this agent
        <span aria-hidden>→</span>
      </span>
    </button>
  );
}

export function FirstRun({ steps, onAddWorkspace }: FirstRunProps): ReactElement {
  const wanted = useSyncExternalStore(subscribeToGuidance, guidanceIsWanted, guidanceIsWanted);
  const taking = useSyncExternalStore(subscribeToTaking, takingAnAgent, takingAnAgent);

  /* `Escape` odkłada przewodnik na bok — DRUGA droga do tej samej czynności, nie druga
     czynność: `stepAside` jest tą samą funkcją, którą woła przycisk obok (niezmiennik 13).
     Ten sam kształt, co zamknięcie szuflady kroku (`./graph/drawer.tsx`). */
  useEffect(() => {
    function heard(event: KeyboardEvent): void {
      if (guidanceHears(event.key, event.defaultPrevented) === 'step aside') stepAside();
    }
    window.addEventListener('keydown', heard);
    return () => {
      window.removeEventListener('keydown', heard);
    };
  }, []);

  const now = steps.find((step) => step.state === 'now');
  const hello = welcomeFor(steps, taking.landed);
  const act: Record<FirstRunStep['id'], () => void> = {
    agent: openAgents,
    workflow: openWorkflows,
    workspace: onAddWorkspace,
  };

  /* Zaproszenie do folderu należy do powitania dokładnie wtedy, gdy folder JEST bieżącym
     krokiem; w każdej innej chwili stoi przy swoim przystanku. Powód w nagłówku pliku. */
  const askingHere = now?.id === 'workspace';
  const folderIsMissing = steps.some((step) => step.id === 'workspace' && step.state !== 'done');

  /* ODŁOŻONY NA BOK — i to nie znaczy „pusty ekran". Zostaje jedno zdanie i jedna kontrolka,
     czyli dokładnie to, czego DESIGN §6 żąda od pustego ekranu; znika droga, powitanie w pełnej
     skali, galeria i wiersz klawiszy. Znacznik `data-empty` zostaje na zdaniu, bo ekran Run ma
     nieść dokładnie jeden (`src/sections/empty-screen-invites.test.tsx`). */
  if (!wanted || hello === null || now === undefined) {
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-[18px]">
        <p data-empty className="max-w-[44ch] text-center text-lede text-body">
          {hello?.sentence ?? ''}
        </p>
        {hello === null ? null : (
          <button
            type="button"
            {...(folderIsMissing && askingHere ? { 'data-add-workspace': true } : {})}
            onClick={act[now?.id ?? 'agent']}
            className="btn-primary"
          >
            {hello.act}
          </button>
        )}
      </div>
    );
  }

  const doneSoFar = steps.filter((step) => step.state === 'done').length;

  return (
    <div
      data-first-open
      className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-[18px] py-3"
    >
      {/* ── DROGA ─────────────────────────────────────────────────────────────────────────
          Wchodzi sprężyną RAZ (`enter`, `both`) — pojawienie się powierzchni jest jednym
          z trzech miejsc, w których DESIGN §7 na nią pozwala. */}
      <div
        data-first-road
        className="enter flex shrink-0 items-center gap-[22px] rounded-lg border border-accent-edge px-[18px] py-[13px]"
        style={{
          background:
            'linear-gradient(90deg, var(--color-accent-soft), rgba(255,255,255,0.02) 60%, transparent)',
        }}
      >
        <div className="w-[126px] shrink-0">
          <p className="text-eyebrow">Your first run</p>
          <p data-road-count className="mt-[3px] font-mono text-title text-ink">
            {String(doneSoFar)} <span className="text-note text-muted">of 3 done</span>
          </p>
        </div>

        {/* `<ol>`, bo to jest kolejność, a nie zbiór: krok drugi bez pierwszego nie ma sensu,
            i czytnik ekranu ma o tym powiedzieć tak samo jak oko. */}
        {/* `flex-wrap`, i to jest odpowiedź na rozmiar okna, a nie ozdoba. Przystanki noszą
            napisy, które się nie łamią (`whitespace-nowrap` niżej, dokładnie jak `.stop b`
            w makiecie), więc rząd bez zawijania ma 779 px szerokości minimalnej, a cała tafla
            z nim 1118. `src-tauri/tauri.conf.json` pozwala zwęzić okno do 1100 px, czyli do
            766 px na tę taflę — i bez tej jednej klasy nadmiar wychodzi na zewnątrz i przewraca
            CAŁY ekran w bok, dokładnie tak, jak przed tą naprawą robiła to kolumna kroków
            (zmierzone: `main` 766 px własne, 798 px treści). Przy 1512 px nic się nie zawija
            i tafla jest co do piksela taka, jak rysuje ją makieta.

            `gap-y-3` daje zawiniętym wierszom odstęp; bez niego drugi dotyka pierwszego. */}
        <ol data-first-run className="flex min-w-0 flex-1 list-none flex-wrap items-center gap-y-3">
          {steps.map((step, at) => (
            <Fragment key={step.id}>
              {at === 0 ? null : (
                /* Łączka między przystankami zapala się dopiero wtedy, gdy przystanek PRZED
                   nią jest zrobiony — czyli rysuje relację, która w danych jest. */
                <li
                  aria-hidden
                  data-first-link={steps[at - 1]?.state === 'done' ? 'lit' : 'waiting'}
                  className="mx-4 h-0.5 min-w-4 flex-1 rounded-pill"
                  style={
                    steps[at - 1]?.state === 'done'
                      ? {
                          background:
                            'linear-gradient(90deg, var(--color-ok), var(--color-ok-soft))',
                        }
                      : {
                          background:
                            'repeating-linear-gradient(90deg, var(--color-line-strong) 0 5px, transparent 5px 11px)',
                        }
                  }
                />
              )}
              <li
                data-first-step={step.id}
                data-step-state={step.state}
                className="flex shrink-0 items-center gap-[11px]"
              >
                <span
                  aria-hidden
                  className={
                    'grid size-8 shrink-0 place-items-center rounded-pill border font-mono text-mono-strong ' +
                    (step.state === 'done'
                      ? 'border-ok-edge bg-ok-soft text-ok'
                      : step.state === 'now'
                        ? 'border-transparent bg-accent text-bg'
                        : 'border-line-strong bg-well text-muted')
                  }
                >
                  {step.state === 'done' ? '✓' : String(at + 1)}
                </span>
                <span className="block">
                  <b
                    className={
                      'block whitespace-nowrap text-subhead ' +
                      (step.state === 'now' ? 'text-ink' : 'text-body')
                    }
                  >
                    {step.title}
                  </b>
                  <span className="mt-px block whitespace-nowrap text-meta text-muted">
                    {step.note}
                  </span>
                </span>
                {step.id === 'workspace' && folderIsMissing && !askingHere ? (
                  <button
                    type="button"
                    data-add-workspace
                    onClick={onAddWorkspace}
                    className="btn-quiet ml-1"
                  >
                    {FIRST_INVITE}
                  </button>
                ) : null}
              </li>
            </Fragment>
          ))}
        </ol>

        <button
          type="button"
          data-step-aside
          onClick={stepAside}
          className="btn-quiet shrink-0"
          title="Put this guidance away for now"
        >
          I know my way <kbd className={KEY_CAP}>Esc</kbd>
        </button>
      </div>

      {/* ── POWITANIE ─────────────────────────────────────────────────────────────────────
          Jedyne miejsce w aplikacji, które nosi `--text-display`, i jedyna rzecz na tym
          ekranie, która oddycha. */}
      <div
        data-first-hero
        /* `grow shrink-0`, NIE `flex-1`. `flex-1` znaczy `1 1 0%`, czyli „rośnij, ale wolno cię
           też ścisnąć do zera" — a powitanie ze `justify-center` ściśnięte poniżej swojej treści
           wylewa ją w OBIE strony i maluje po drodze i po galerii. Zmierzone w chromium przy
           oknie 1100×700, czyli najmniejszym, jakie `src-tauri/tauri.conf.json` dopuszcza.
           `grow` daje wzrost w wolną wysokość, `shrink-0` trzyma dolną granicę na treści,
           a przewijaniem zajmuje się `overflow-y-auto` na całej tafli wyżej. */
        className="flex shrink-0 grow flex-col items-center justify-center py-2 text-center"
      >
        <span
          aria-hidden
          data-first-orb
          className="relative mb-5 grid size-[88px] place-items-center"
        >
          <span
            className="size-full rounded-pill"
            style={{
              background:
                'radial-gradient(circle at 34% 28%, var(--color-accent-hover), var(--color-accent) 52%, var(--color-accent-active))',
              boxShadow: '0 0 70px 14px var(--color-accent-soft)',
            }}
          />
          {/* Pierścień. STOI W MIEJSCU, i to jest ograniczenie, nie wybór: `docs/ARCHITECTURE.md`
              §7 daje na całą aplikację DWA miejsca, które się ruszają, a `exactly-one-thing
              -pulses.test.ts` żąda dodatkowo, żeby każde z nich niosło barwę „dzieje się teraz".
              Powitanie nie dzieje się teraz — czeka. Ruch tego ekranu jest więc w wejściu
              (`.enter`, `.fade-in`), czyli w chwili, w której powierzchnia się pojawia. */}
          <span className="absolute -inset-[17px] rounded-pill border border-dashed border-accent-edge" />
        </span>
        <h1 className="flex items-center gap-3 text-display text-ink">
          <span
            aria-hidden
            className="size-[11px] shrink-0 rounded-pill bg-accent"
            style={{ boxShadow: '0 0 14px var(--color-accent)' }}
          />
          {hello.title}
        </h1>
        {/* Znacznik pustego ekranu siedzi na SAMYM zdaniu — `src/sections/empty-screen-invites
            .test.tsx` czyta jego treść i żąda jednego zdania bez glifu i bez przycisku w środku.
            Dlatego każde z trzech powitań w `./welcome.ts` jest jednym zdaniem. */}
        <p data-empty className="mt-3 max-w-[44ch] text-lede text-body">
          {hello.sentence}
        </p>
        <button
          type="button"
          {...(askingHere ? { 'data-add-workspace': true } : {})}
          onClick={act[now.id]}
          className="btn-primary mt-6 h-14 gap-3 px-8 text-question"
        >
          {hello.act}
          <span aria-hidden>→</span>
          {hello.press === '' ? null : (
            <kbd className={KEY_CAP_ON_ACCENT} style={ON_ACCENT}>
              {hello.press}
            </kbd>
          )}
        </button>
        <p data-first-reassure className="mt-4 text-note text-muted">
          {hello.reassure}
        </p>
      </div>

      {/* ── GALERIA GOTOWYCH ──────────────────────────────────────────────────────────────
          Każdy z tych przycisków NAPRAWDĘ zapisuje plik agenta (`./starters.ts`). */}
      <div data-starters className="fade-in shrink-0">
        <p className="flex items-center gap-3 text-eyebrow text-muted">
          Or take one that is ready
          <span aria-hidden className="h-px flex-1 bg-line" />
        </p>
        <div className="mt-3 grid grid-cols-4 gap-3">
          {STARTERS.map((one) => (
            <ReadyMade key={one.agent.name} one={one} busy={taking.busy !== null} />
          ))}
          <button
            type="button"
            data-write-your-own
            onClick={openAgents}
            className="flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-line-strong p-[15px] text-center transition-colors hover:border-accent-edge hover:bg-hover"
          >
            <span
              aria-hidden
              className="grid size-10 place-items-center rounded-pill border border-line-strong bg-raised text-body"
            >
              <svg
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
                className="size-4"
              >
                <path d="M8 3.4 V12.6 M3.4 8 H12.6" />
              </svg>
            </span>
            <span className="block text-subhead text-ink">Write your own</span>
            <span className="block text-note text-body">
              A name, a job in one line, a model and a spend limit.
            </span>
          </button>
        </div>
      </div>

      {/* ODMOWA DYSKU MA GŁOS. Kliknięcie w kartę, po którym nic nie drgnie, czyta się jak
          kliknięcie, które nie doszło (niezmiennik 16). Zdanie jest słowo w słowo od Rusta. */}
      {taking.said === null ? null : (
        <p data-starter-said className="fade-in shrink-0 text-center lead" data-tone="fail">
          {taking.said}
        </p>
      )}

      {/* ── KLAWISZE ──────────────────────────────────────────────────────────────────────
          Wyłącznie te, na które ta aplikacja naprawdę odpowiada — powód w nagłówku pliku. */}
      <p
        data-first-keys
        className="flex shrink-0 flex-wrap items-center justify-center gap-2 text-note text-muted"
      >
        <kbd className={KEY_CAP}>⌘K</kbd> reaches anything by name ·{' '}
        <kbd className={KEY_CAP}>⌘1</kbd>–<kbd className={KEY_CAP}>⌘7</kbd> switch sections ·{' '}
        {/* DOPISANE 2026-08-31 razem z drugim trybem bocznego menu. Skrót, którego się nie zna,
            nie istnieje — a ten wiersz jest jedynym miejscem, w którym człowiek uczy się
            klawiszy PATRZĄC, nie czytając dokumentacji. Kryterium „names only keys this
            application really answers" pyta o niego `moveFor`, więc obietnica nie ma jak się
            rozjechać z klawiaturą. */}
        <kbd className={KEY_CAP}>⌘B</kbd> folds the side nav · every screen here tells you the next
        step
      </p>
    </div>
  );
}
