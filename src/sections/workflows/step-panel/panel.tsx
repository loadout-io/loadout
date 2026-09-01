/* Panel kroku po prawej — PIĘĆ rzeczy w widoku, reszta za jedną linią.
 *
 * Pięć etykiet, w tej kolejności, i ani jednej szóstej:
 *   Name · Who does this · What to do · Where it works · How many at once
 *
 * To jest odpowiedź na pytanie „co ten krok robi": kto, co, gdzie i ile naraz. Wszystko poza
 * tą piątką ma działającą wartość dziedziczoną z agenta, więc stoi za ujawnieniem
 * (`./more-settings.tsx`) i jest tam wymienione co do sztuki:
 *   Can it change files · Give up after · Write results to · Try again up to ·
 *   If this step does not pass · What it hands over · Skills · Borrow from this project
 *
 * 2026-08-31 — POWÓD, ZMIERZONY. Ten nagłówek mówił „siedem etykiet, w tej kolejności, i ani
 * jednej ósmej" i był o SZEŚĆ bloków nieaktualny. Panel montował 21 kontrolek stałych,
 * a realistycznie 34 (przy sześciu umiejętnościach i pięciu pozycjach do pożyczenia) w kolumnie
 * 330 px — bez grup, bez zwijania, bez nadoczek. To jest ~60 elementów niosących tekst, czyli
 * CAŁY sufit widoku z `docs/ARCHITECTURE.md` §7, zjedzony przez jedną kolumnę. Żaden wiersz nie
 * wszedł tu błędnie: przyrastały po jednym i każdy z osobna był do obrony. Równość „pięć etykiet"
 * ma teraz kryterium (`panel-keeps-five-things-in-view.test.tsx`), bo zdanie w nagłówku pliku już
 * raz przegrało z sześcioma kolejnymi zadaniami.
 *
 * Wartości efektywne pochodzące z agenta niosą kropkę, szare `Agent uses: …` i `Reset`, kiedy
 * krok się od agenta różni [T4 §4.5]. Licznik „N changed" jest liczony z `step.overrides` TUTAJ
 * i nigdzie indziej (niezmiennik 13: jeden fakt, jedno miejsce), a POKAZUJE go zwinięte
 * ujawnienie — bo to za nim stoi wszystko, co ta liczba liczy. Do 2026-08-31 siedział przy
 * „Who does this"; dwa żywe regiony na jeden fakt są tym samym niezmiennikiem czytanym
 * w drugą stronę.
 *
 * Czego tu NIE MA: przełącznika „Let it split into helpers" z makiety (linia 625). Żadne pole
 * schematu go nie niesie, a T3 §7.3 i T4 §3.3 zgodnie wykluczają głębokość delegacji z v1.
 * Przepisanie makiety jeden do jednego jest tu dokładnie tym, jak łamie się niezmiennik 16:
 * trzeci przełącznik wygląda identycznie jak dwa działające.
 *
 * Wiersze, które są osobnymi komponentami (Skills, Borrow, przekazanie), dokłada `AgentPanel`
 * na dole tego pliku — do ŚRODKA ujawnienia i razem z ich liczbą, żeby zwinięta linia mówiła
 * o tym, co naprawdę powstało, a nie o tym, co zwykle powstaje.
 *
 * Panel jest STEROWANY — wartości i każde kliknięcie wychodzą propsami. Powód jest testowy:
 * w repo nie ma `jsdom` ani `@testing-library/react` (`package.json` jest na liście DENIED
 * w `checks/quick-scope.sh`), więc panel sprawdzamy przez `renderToStaticMarkup`, a stan
 * trzymany wewnątrz komponentu byłby dla takiego testu niewidoczny.
 *
 * 2026-08-18, WIECZÓR — DWIE NAPRAWY, KTÓRE SĄ POWODEM PIERWSZEGO ZDANIA WŁAŚCICIELA
 * („ustawiasz workflow ale agentów nie da się wybrać"):
 *
 *   1. Lista wyboru agenta stała WYŁĄCZNIE w `PickAnAgent`, czyli w panelu kroku, którego
 *      agenta NIE DA SIĘ rozwiązać. Po pierwszym wyborze panel przełączał się na `StepPanel`,
 *      a tam wiersz „Who does this" był nieklikalnym `<span>` z nazwą agenta — komentarz obok
 *      przyznawał to wprost. Pomyłka przy wyborze była więc NIEODWRACALNA z okna: jedyną drogą
 *      naprawy było otwarcie pliku JSON w edytorze tekstu. Wybór jest teraz w JEDNYM miejscu
 *      (`AgentChoice`) i oba panele montują to samo.
 *   2. Panel świeżego kroku miał dwa wiersze: Name i Who does this. Pola `What to do` nie było
 *      w nim wcale, a `<textarea id="step-instructions">` żyło tylko w `StepPanel`, czyli za
 *      blokadą z punktu 1. Dowód, że właściciel tam nie dotarł: oba jego pliki workflow mają
 *      `"instructions": ""`. Instrukcje kroku są teraz edytowalne od pierwszej chwili — razem
 *      z `How many at once`, bo to też jest pole KROKU i nie potrzebuje agenta.
 *
 * 2026-08-22 — TRZECIA NAPRAWA TEGO SAMEGO WIERSZA, ze zrzutu ekranu właściciela.
 *
 * Wybór DAŁO się już zrobić, ale rozwijał się jako natywny `<select>`, w którym każda pozycja
 * niosła `nazwa — opis`. Zmierzone na jego bibliotece: siedemnaście agentów, opisy po dwieście
 * znaków, więc lista brała szerokość najdłuższego opisu i całą wysokość okna — zasłaniała ekran,
 * na którym się wybiera, razem z panelem, do którego wybór wraca. Rozwijanej listy natywnej nie
 * da się ani zwęzić, ani przyciąć, ani przeszukać: `<option>` nie przyjmuje ani stylu, ani
 * drugiego wiersza. Wiersz jest teraz własną kontrolką: zwinięty pokazuje samą nazwę, rozwinięty
 * daje pole szukania i listę z sufitem wysokości, w której opis jest przycięty do dwóch wierszy.
 *
 * Co z tego wynika dla testów: `AgentChoice` trzyma trzy pola stanu WIDOKU (`open`, `query`,
 * `at`). Nagłówek wyżej dalej obowiązuje — dotyczy WARTOŚCI, a rozwinięcie listy wartością nie
 * jest. Renderowanie statyczne widzi więc stan zwinięty i to jest ten, o który pytają kryteria.
 */
import { useState } from 'react';
import type { ReactElement, ReactNode } from 'react';
import type { Agent, FileAccess } from '../../../state/agents';
import type {
  AgentStep,
  CheckpointStep,
  OverridableField,
  Overrides,
  ServeStep,
  SkillChoice,
  Step,
  WhenItFails,
} from '../../../state/workflows';
import { SKILL_SUBSETTING } from './capabilities';
import type { CheckFields } from './check-panel';
import { CheckPanel } from './check-panel';
import { CheckpointPanel } from './checkpoint-panel';
import { MoreSettings } from './more-settings';
import { ServePanel } from './serve-panel';
import { resolve } from './overrides';
import { BorrowRow, borrowRowStands, nothingBorrowed, useHostMaterial } from './borrow-row';
import { HandoverRow } from './handover-row';
import { SkillsRow, skillsRowStands } from './skills-row';
import { WhereItWorks } from './where-it-works';

/** Pola, które należą do samego KROKU agenta, a nie do agenta (patrz nagłówek pliku). */
export type AgentStepFields = Partial<
  Pick<
    AgentStep,
    'name' | 'instructions' | 'copies' | 'folder' | 'whenItFails' | 'borrow' | 'handover'
  >
>;

/** Oba pola punktu kontrolnego. Punkt kontrolny nie dziedziczy niczego, więc to jest całość. */
export type CheckpointFields = Partial<Pick<CheckpointStep, 'name' | 'question'>>;

/** Trzy pola kafelka „uruchom i zostaw" — z tego samego powodu, co wyżej.
 *
 * `folder` DOSZŁO 2026-08-23, po pierwszym prawdziwym użyciu: dla serwera to nie jest szczegół,
 * tylko treść. Powód w całości stoi przy `OFFERS` w `./serve-panel.tsx`. */
export type ServeFields = Partial<Pick<ServeStep, 'name' | 'command' | 'folder' | 'commandFrom'>>;

/* WARTOWNIKA `create-a-new-agent` TU JUŻ NIE MA, i to jest skutek, nie przeoczenie.
 *
 * Dopóki lista była `<select>`, skrót do sekcji Agents musiał być `<option>` z wartością,
 * której żaden agent nie może nosić — inaczej rozpoznawało by się go po TEKŚCIE, a zmiana copy
 * cicho zamieniałaby skrót w wybór agenta o nazwie, której nie ma. Lista jest teraz zbiorem
 * przycisków, a skrót ma własny `onClick`, więc nie ma wartości, z którą mógłby się zderzyć. */

export interface StepPanelProps {
  step: AgentStep;
  /** Agent wskazany przez krok. Panel czyta go, żeby pokazać wartości efektywne — i NIGDY go
   * nie zapisuje (`docs/mockup/index.html:604`). */
  agent: Agent;
  /** Cała biblioteka: wiersz „Who does this" jest listą WYBORU, także po wyborze. */
  agents: readonly Agent[];
  /** Zmiana agenta na tym kroku. Nie jest nadpisaniem — to pole samego kroku. */
  onChooseAgent: (agentId: string) => void;
  /** Skrót do sekcji Agents z pozycji `＋ Create a new agent…`. */
  onCreateAgent: () => void;
  /** Zmiana wiersza pochodzącego z agenta, podana wartością efektywną. */
  onEdit: (edit: Overrides) => void;
  /** Zmiana pola, które należy do samego kroku. */
  onEditStep: (fields: AgentStepFields) => void;
  /** `Reset` przy jednym wierszu. */
  onReset: (field: OverridableField) => void;
  /**
   * Wiersze, które wołający dokłada DO ŚRODKA ujawnienia — te, których ten panel sam nie zna,
   * bo są osobnymi komponentami (przekazanie, Skills, Borrow, liczba rund).
   *
   * Lista już ODSIANA: mają w niej stać wyłącznie wiersze, które naprawdę powstaną. Zwinięta
   * pokrywa nazywa liczbę rzeczy w środku, a liczba policzona z wierszy, z których dwa oddały
   * `null`, jest zdaniem o panelu, którego człowiek nigdy nie zobaczy. Odsianie robi wołający,
   * bo to on ma odpowiedzi (`skillsRowStands`, `borrowRowStands`) — te same, którymi wiersze
   * rozstrzygają to u siebie.
   */
  more?: readonly ReactNode[];
}

/* 2026-08-31 — `ROW`, `LABEL`, `FROM_AGENT`, `CHIP` I `AREA` ZNIKŁY.
 *
 * Każda z nich była napisem-listą-klas dla roli, którą `theme.css` nosi od tej daty pod nazwą:
 * `.stack` (etykieta nad kontrolką), `.label` (etykieta pola), `.lead` (zdanie drugoplanowe),
 * `.chip` (pigułka odczytu). `AREA` była drugą nazwą na `.field` — jeden fakt, dwa napisy.
 *
 * `LABEL` i `FROM_AGENT` miały do dziś TĘ SAMĄ treść, choć opisywały dwie różne role: etykietę
 * nad polem i zdanie pod nim. Rozdzielenie ich na `.label` i `.lead` jest jedyną zmianą
 * wyglądu, którą ten plik wnosi świadomie — DESIGN §6 rozstrzyga stopień zdania drugoplanowego
 * na `--t-note`, a zdania „Agent uses: …" i podpowiedzi pod polami są właśnie nim. */
/* POLE BIERZE KLASE DOMU, NIE WLASNY OPIS.
 *
 * `theme.css` ma klase `.field` od pierwszego dnia: studnia, mocny obrys, promien z pasma, kroj
 * maszynowy i `user-select: text` — to ostatnie jest czescia pola, nie ozdoba, bo `body` wylacza
 * zaznaczanie w calej aplikacji. Do 2026-08-19 wolaly ja DWA miejsca, a cztery sekcje przepisywaly
 * ten sam wyglad recznie w dwunastu stalych — i rozjechaly sie: tu obrys byl `--line`, w Skills
 * `--line-strong`. Jeden fakt, jedno miejsce (niezmiennik 13); dwa opisy tego samego pola czyta
 * sie jak dwa rozne stany, a nie jak dwa pola.
 *
 * Skupienia tu nie ma z tego samego powodu. `theme.css` daje `.field:focus` obwodke w akcencie
 * i globalny `:focus-visible` obrys — jedna regula na cala aplikacje. Dopisanie tego samego
 * narzedziem na kazdym polu byloby trzecia kopia decyzji, ktora juz jest podjeta. */
const FIELD = 'field';
/* Podkreślony napis, nie przycisk: `Reset` i `＋ Create a new agent` stoją W WIERSZU z etykietą
 * (`flex items-baseline`), a `.btn-quiet` ma 28 px wysokości i padding — postawiony tutaj
 * rozepchnąłby trzy wiersze panelu. Brakowało im wyłącznie reakcji na najechanie i to jest
 * jedyna rzecz, którą dostały 2026-08-31. */
const QUIET = 'label underline hover:text-ink';

/* Brzmienia wartości — te same, które ma formularz agenta (`src/sections/agents/agent-form.tsx`).
 * Druga kopia, świadoma: tamten plik ich nie eksportuje i nie należy do tego zadania, a wpisanie
 * `look-only` w szary wiersz „Agent uses:" wpuściłoby nazwę z drutu na ekran (niezmiennik 14).
 * Wspólny moduł brzmień jest właściwym domem dla obu — kiedy ktoś będzie posiadał oba pliki. */
const FILE_ACCESS: ReadonlyArray<{ value: FileAccess; label: string }> = [
  { value: 'look-only', label: 'Look only' },
  { value: 'ask-first', label: 'Ask first' },
  { value: 'work-freely', label: 'Work freely' },
];

const THINKING: Record<Agent['thinking'], string> = {
  quick: 'Quick',
  balanced: 'Balanced',
  deep: 'Deep',
  deepest: 'Deepest',
};

/** Ile minut, po ludzku. `0` znaczy „bez limitu" i nigdy nie jest pustym polem [T4 §4.3]. */
function minutes(value: number): string {
  return value === 0 ? 'no limit' : `${String(value)} minutes`;
}

/** Wartość, którą wziąłby agent, jako zdanie po angielsku.
 *
 * To jest treść szarego wiersza `Agent uses: …`, więc nie ma prawa być nazwą z drutu ani
 * `[object Object]`: użytkownik czyta ją zamiast otwierać sekcję Agents [T4 §4.5]. */
function agentUses(field: OverridableField, agent: Agent): string {
  switch (field) {
    case 'thinking':
      return THINKING[agent.thinking];
    case 'fileAccess':
      return FILE_ACCESS.find((one) => one.value === agent.fileAccess)?.label ?? '';
    case 'giveUpAfterMinutes':
      return minutes(agent.giveUpAfterMinutes);
    case 'tools':
      return agent.tools === 'everything' ? 'Everything' : agent.tools.only.join(', ');
    case 'skills':
      return agent.skills.length === 0 ? 'none' : agent.skills.join(', ');
    case 'connections':
      return agent.connections.length === 0 ? 'none' : agent.connections.join(', ');
    case 'instructions':
      return agent.instructions;
    case 'model':
      return agent.model;
    case 'writeResultsTo':
      return agent.writeResultsTo;
  }
}

/** Etykiety wierszy, które mają własną kontrolkę. Reszta zmienionych ustawień pokazuje się
 * szarym wierszem pod „Who does this" — patrz `noRowOfTheirOwn`. */
const HAS_A_ROW: readonly OverridableField[] = [
  'fileAccess',
  'giveUpAfterMinutes',
  'writeResultsTo',
];

/** Zmienione ustawienia, które nie mają własnej kontrolki w żadnym wierszu panelu.
 *
 * Bez tego zmiana `thinking` znikałaby z ekranu: licznik mówiłby „1 changed", a użytkownik nie
 * miałby jak zobaczyć CZEGO ani jak to cofnąć. Szare wiersze stoją w tym samym ujawnieniu, co
 * reszta ustawień dziedziczonych, i liczą się do jego liczby — inaczej pokrywa mówiłaby
 * o mniejszej liczbie rzeczy, niż pod nią stoi. */
function noRowOfTheirOwn(changed: OverridableField[]): OverridableField[] {
  return changed.filter((field) => !HAS_A_ROW.includes(field));
}

interface AgentChoiceProps {
  /** `step.agent`. Pusty napis znaczy „jeszcze nikt" i tak wychodzi krok z `＋ Add step`. */
  chosen: string;
  agents: readonly Agent[];
  onChooseAgent: (agentId: string) => void;
  onCreateAgent: () => void;
}

/* Powierzchnie listy wyboru. Ramkę rysuje `.paper` — kartka treści z `theme.css`, czyli ta sama
 * klasa domu, którą biorą wszystkie nieprzejrzyste powierzchnie pod tekstem do czytania. `.pane`
 * byłby tu błędem: pływa i ma cień, a w aplikacji pływa dokładnie jedna rzecz (DESIGN §3), a ta
 * lista rozwija się W TOKU panelu. */
const LIST = 'paper flex flex-col';
/* Sufit wysokości, po którym lista zaczyna się przewijać SAMA. To jest cała różnica wobec
 * `<select>`: siedemnaście pozycji rozwijanych natywnie rysowało się na całą wysokość okna,
 * a szerokość brała najdłuższy opis, więc lista zasłaniała ekran, na którym się wybiera.
 *
 * Wysokość jest dobrana tak, żeby ostatni widoczny wiersz był PRZYCIĘTY W POŁOWIE. macOS chowa
 * paski przewijania, dopóki nikt nie kręci kółkiem, więc niepełny wiersz jest jedyną rzeczą,
 * która mówi „lista jedzie dalej" — sufit równy całkowitej liczbie wierszy wygląda jak koniec. */
const SCROLLER = 'max-h-80 overflow-auto';
/* WIERSZ MENU, czyli `.row`: myjka pod kursorem, wciśnięcie, pierścień skupienia i jeden
 * kształt zaznaczenia na całą aplikację. `flex-col items-start` robi z niego wiersz o dwóch
 * liniach — to jest rozmieszczenie, a nie rola, więc należy do miejsca. */
const PICK = 'row flex-col items-start gap-0.5 text-left';
/* Podświetlenie klawiatury i myszy to JEDNA wartość — `at` niżej — więc i jedna klasa. Dwa
 * osobne wyróżnienia dają ekran, na którym dwie pozycje wyglądają na wybrane naraz. */
const PICK_ON = 'bg-accent-soft';
/* `.value` niesie rodzinę maszynową RAZEM ze stopniem — `font-mono` dopisane obok `text-mono`
 * było deklaracją tej samej rodziny dwa razy. Barwa idzie `data-tone`, nie klasą-bliźniakiem. */
const PICK_NAME = 'value truncate';
/* Opis jest PRZYCIĘTY DO DWÓCH WIERSZY i to jest decyzja, nie oszczędność miejsca: opisy
 * agentów w bibliotece właściciela mają po dwieście znaków, a lista, w której jedna pozycja
 * zajmuje sześć wierszy, przestaje być listą do przebiegnięcia wzrokiem. */
const PICK_SAYS = 'lead line-clamp-2';
/* Też wiersz menu, tyle że za linią. `border-t-line` maluje WYŁĄCZNIE górną krawędź: `.row`
 * ma obrys 1 px w przezroczystości ze wszystkich stron, więc `border-line` zapaliłby też
 * pozostałe trzy. */
const CREATE_ROW = 'row border-t border-t-line text-label text-muted';
const NO_MATCH = 'lead px-2.5 py-2';

/** Czy agent pasuje do wpisanego szukania.
 *
 * Nazwa I opis, bo w bibliotece, która wywróciła ten ekran, połowa pozycji nazywa się
 * `*-verifier` albo `*-dev`: po samej nazwie nie da się ich rozróżnić, a po opisie tak. */
function looksLike(one: Agent, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === '') return true;
  return `${one.name} ${one.summary}`.toLowerCase().includes(needle);
}

/** Na którym wierszu stanąć po rozwinięciu listy: na agencie, którego krok już ma.
 *
 * Zero dla kroku bez agenta i dla agenta, którego w bibliotece nie ma (plik workflow mógł
 * przeżyć skasowanie agenta). Wersja bez tej funkcji — stałe zero — po `Change` podświetlała
 * PIERWSZEGO z listy, czyli Enter podmieniał agenta na kogoś innego jednym klawiszem. */
function standsAt(agents: readonly Agent[], chosen: string): number {
  const seat = agents.findIndex((one) => one.id === chosen);
  return seat === -1 ? 0 : seat;
}

/** Kontrolka wyboru agenta — JEDNA na całą sekcję i to jest cały sens tego komponentu.
 *
 * Do 2026-08-18 istniała tylko w panelu kroku BEZ agenta, więc zmiana raz podjętej decyzji była
 * niemożliwa z okna. Dwie kopie tej listy — jedna „przed wyborem", druga „po" — byłyby dwoma
 * miejscami, w których mieszka odpowiedź na pytanie „jak wybiera się agenta" (niezmiennik 13),
 * i pierwszą okazją, żeby jedna z nich zapisywała coś innego niż druga.
 *
 * Pusta biblioteka nie dostaje listy, tylko zdanie i DROGĘ WYJŚCIA. Wcześniej stało tam samo
 * zdanie („Make one in Agents, then come back") i ani jednej ścieżki z powrotem — czyli
 * instrukcja nawigacji zamiast nawigacji.
 *
 * DLACZEGO STAN JEST TUTAJ, W PANELU, KTÓRY POZA TYM JEST STEROWANY. Nagłówek pliku mówi, że
 * wartości i kliknięcia wychodzą propsami, bo test statyczny nie zobaczy stanu wewnętrznego.
 * To dalej obowiązuje dla WARTOŚCI. `open`, `query` i `at` wartościami nie są: nie ma ich
 * w dokumencie, nie przeżywają zamknięcia panelu i nie ma czego o nich zapisać na dysk.
 * Wypchnięcie ich propsami znaczyłoby trzy pola „czy lista jest rozwinięta" w magazynie
 * workflow — czyli stan widoku w pliku, który ma być prawdą o kroku.
 */
function AgentChoice({
  chosen,
  agents,
  onChooseAgent,
  onCreateAgent,
}: AgentChoiceProps): ReactElement {
  /* ROZWINIĘTA OD RAZU, KIEDY KROK NIE MA JESZCZE AGENTA — i to nie jest wygoda, tylko warunek
   * z kryterium `every-tile-opens-a-panel`: panel kroku bez agenta ma OFEROWAĆ bibliotekę, a nie
   * pokazywać dziurę z przyciskiem, za którym ona jest. Krok prosto z `＋ Add step` ma dokładnie
   * jedno zadanie i to ono stoi otwarte. Po wyborze lista zwija się sama i już się nie otwiera. */
  const [open, setOpen] = useState(() => !agents.some((one) => one.id === chosen));
  const [query, setQuery] = useState('');
  /** Podświetlona pozycja — indeks w LIŚCIE PO ODSIANIU, nie w bibliotece. */
  const [at, setAt] = useState(() => standsAt(agents, chosen));

  if (agents.length === 0) {
    return (
      <>
        <span className="lead">You have not saved anyone yet.</span>
        <button type="button" className={QUIET} onClick={onCreateAgent}>
          ＋ Create a new agent
        </button>
      </>
    );
  }

  const picked = agents.find((one) => one.id === chosen);
  const shown = agents.filter((one) => looksLike(one, query));

  const choose = (agentId: string): void => {
    setOpen(false);
    setQuery('');
    setAt(0);
    onChooseAgent(agentId);
  };

  /* ZWINIĘTA: jeden wiersz, który mówi, kto to robi, i nic poza tym.
   *
   * Opisu agenta tu NIE MA świadomie. Stał w treści `<option>` razem z nazwą i to jest dokładny
   * powód, dla którego rozwinięta lista miała szerokość ekranu — a po wyborze i tak widać go
   * było tylko w jednej trzeciej, bo `<select>` przycina zamkniętą wartość. Opis jest tam, gdzie
   * jest potrzebny: przy WYBIERANIU, w rozwiniętej liście. */
  if (!open) {
    return (
      <button
        id="step-agent"
        type="button"
        className={`${FIELD} flex items-center justify-between gap-2 text-left`}
        onClick={() => {
          setOpen(true);
          setQuery('');
          setAt(standsAt(agents, chosen));
        }}
      >
        {picked === undefined ? (
          /* Barwa tuszu mówi o stanie i tylko ona — studnia, obrys i promień zostają te same,
           * co w każdym innym polu panelu. Krok bez agenta ma jedno zadanie i to zdanie jest
           * jedyną rzeczą w panelu, która niesie akcent. */
          <span className="text-accent">Pick an agent</span>
        ) : (
          <span className={PICK_NAME} data-tone="ink">
            {picked.name}
          </span>
        )}
        <span className="lead">{picked === undefined ? 'Choose' : 'Change'}</span>
      </button>
    );
  }

  return (
    <div
      className="flex flex-col gap-2"
      onBlur={(event) => {
        /* Zamknięcie po WYJŚCIU OGNISKA, nie po kliknięciu w tło — tła nie ma, bo lista rozwija
         * się w toku panelu. Kliknięcia w samą listę tu nie trafiają: wiersze zabraniają myszy
         * zabierać ognisko (`onMouseDown` niżej), więc `relatedTarget` poza tym drzewem znaczy
         * naprawdę „człowiek poszedł gdzie indziej". Bez tego bloku lista zostawałaby otwarta
         * pod polem, w którym ktoś już pisze instrukcje. */
        const next = event.relatedTarget;
        if (next === null || !event.currentTarget.contains(next)) setOpen(false);
      }}
    >
      <input
        id="step-agent"
        className={FIELD}
        /* Ognisko wchodzi w pole szukania od razu, bo rozwinięcie listy JEST początkiem pisania.
         * Zmierzone w tym repo: `autoFocus` daje i `autofocus=""` w markupie, i `.focus()`
         * w oknie — jedno i drugie jest tu potrzebne. */
        autoFocus
        value={query}
        placeholder="Type a name"
        aria-label="Find an agent"
        onChange={(event) => {
          setQuery(event.target.value);
          setAt(0);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            setOpen(false);
            return;
          }
          if (event.key === 'ArrowDown') {
            event.preventDefault();
            setAt((now) => (now + 1 >= shown.length ? now : now + 1));
            return;
          }
          if (event.key === 'ArrowUp') {
            event.preventDefault();
            setAt((now) => (now === 0 ? 0 : now - 1));
            return;
          }
          if (event.key === 'Enter') {
            event.preventDefault();
            const hit = shown[at];
            /* Enter na liście bez ani jednego trafienia nie robi NIC. Wybranie „pierwszego
             * z brzegu", kiedy odsianie nie zostawiło nikogo, przypisałoby krokowi agenta,
             * którego człowiek w tej chwili nie widzi na ekranie. */
            if (hit !== undefined) choose(hit.id);
          }
        }}
      />

      <div
        className={LIST}
        /* Mysz NIE zabiera ogniska polu szukania. Bez tego kliknięcie w wiersz najpierw
         * zamyka listę przez `onBlur`, a `onClick` leci już w element, którego nie ma —
         * czyli wybór myszą po prostu nie działa, i wygląda to na zawieszenie. */
        onMouseDown={(event) => {
          event.preventDefault();
        }}
      >
        <div className={SCROLLER}>
          {shown.length === 0 ? (
            <p className={NO_MATCH}>Nobody here goes by that.</p>
          ) : (
            shown.map((one, index) => (
              <button
                key={one.id}
                data-agent={one.id}
                type="button"
                className={index === at ? `${PICK} ${PICK_ON}` : PICK}
                /* Podświetlony wiersz dojeżdża na ekran SAM. Bez tego strzałka w dół gubi się
                   pod krawędzią przy piątej pozycji, a lista wygląda, jakby przestała reagować.
                   `nearest` nie rusza niczym, kiedy wiersz i tak jest widoczny — czyli przy
                   podświetleniu myszą, które zawsze pada na wiersz pod kursorem. */
                ref={
                  index === at
                    ? (node) => {
                        node?.scrollIntoView({ block: 'nearest' });
                      }
                    : null
                }
                onMouseEnter={() => {
                  setAt(index);
                }}
                onClick={() => {
                  choose(one.id);
                }}
              >
                <span className={PICK_NAME} data-tone="ink">
                  {one.name}
                </span>
                {one.summary === '' ? null : <span className={PICK_SAYS}>{one.summary}</span>}
              </button>
            ))
          )}
        </div>

        {/* Skrót do sekcji Agents stoi POD listą i za linią, bo nie jest jedną z pozycji do
            wyboru. W `<select>` musiał nią być — `<option>` to jedyne, co lista rozwijana
            umie w sobie zmieścić — i przez to stał w tym samym rzędzie, co siedemnastu
            agentów, tuż pod ostatnim z nich. */}
        <button
          type="button"
          className={CREATE_ROW}
          onClick={() => {
            setOpen(false);
            onCreateAgent();
          }}
        >
          ＋ Create a new agent…
        </button>
      </div>
    </div>
  );
}

interface WhoDoesThisProps extends AgentChoiceProps {
  /** Zdanie pod listą. Różne w obu panelach, bo mówią o różnych rzeczach. */
  note: string;
}

/** Wiersz „Who does this" — jedna etykieta, jedna lista wyboru, w obu panelach ta sama.
 *
 * 2026-08-31 — ZNACZNIK „N changed" I SZARE WIERSZE „Agent uses: …" WYPROWADZIŁY SIĘ STĄD do
 * zwiniętego ujawnienia. Obie rzeczy mówią o ustawieniach, które od tego dnia stoją w środku,
 * a licznik pokazany nad zamkniętą pokrywą i drugi raz na niej samej byłby dwoma żywymi
 * regionami na jeden fakt (niezmiennik 13). */
function WhoDoesThis({ note, ...choice }: WhoDoesThisProps): ReactElement {
  return (
    <div data-row="who-does-this" className="stack">
      {/* `htmlFor` celuje w `step-agent` z `AgentChoice` — czyli w przycisk, kiedy lista jest
          zwinięta, i w pole szukania, kiedy jest rozwinięta. Jeden identyfikator na obie
          postacie, bo obie są TĄ SAMĄ kontrolką w dwóch stanach, a nigdy nie stoją obok
          siebie. Przy pustej bibliotece nie ma żadnej z nich i etykieta zostaje bez celu —
          świadomie: alternatywą jest pusta lista wyboru, czyli kontrolka, która nie ma czego
          zrobić (niezmiennik 16). */}
      <label htmlFor="step-agent" className="label">
        Who does this
      </label>
      <AgentChoice {...choice} />
      <span className="lead">{note}</span>
    </div>
  );
}

/** Wiersz `Name` — pole samego kroku, więc identyczny w obu panelach. */
function NameRow({
  value,
  onEditStep,
}: {
  value: string;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
    <div data-row="name" className="stack">
      <label htmlFor="step-name" className="label">
        Name
      </label>
      <input
        id="step-name"
        className={FIELD}
        value={value}
        onChange={(event) => {
          onEditStep({ name: event.target.value });
        }}
      />
    </div>
  );
}

/** Wiersz `What to do` — prompt KROKU, nie nadpisanie agenta.
 *
 * Osobny komponent, bo od 2026-08-18 stoi w OBU panelach. Wcześniej stał tylko w tym z agentem,
 * czyli za blokadą wyboru: pole, w które trafia jedyne zdanie mówiące, co ten krok ma zrobić,
 * było nieosiągalne dla każdego świeżo dodanego kroku. */
function WhatToDoRow({
  value,
  onEditStep,
}: {
  value: string;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
    <div data-row="what-to-do" className="stack">
      <label htmlFor="step-instructions" className="label">
        What to do
      </label>
      <textarea
        id="step-instructions"
        className={FIELD}
        value={value}
        onChange={(event) => {
          onEditStep({ instructions: event.target.value });
        }}
      />
    </div>
  );
}

/** Ile kopii tego kroku wolno postawić naraz [T3 §4.4]. */
const MOST_COPIES = 8;

/** Wiersz `How many at once` — też pole samego kroku, więc też w obu panelach.
 *
 * 2026-08-31 — ZDANIE O SUFICIE. `copiesFrom` przycina do ośmiu i robiło to PO CICHU: wpisanie
 * „9" zamieniało się w polu na „8", a ekran nie mówił dlaczego. Człowiek widzi wtedy liczbę,
 * której nie wpisał, i nie ma jak rozstrzygnąć, czy to sufit, czy pomyłka jego palców.
 *
 * Zdanie stoi przy ośmiu i tylko przy ośmiu, czyli DOKŁADNIE w chwili, w której przycięcie
 * właśnie się wydarzyło (albo w której następne się wydarzy). Napisane pod polem na stałe
 * byłoby szumem przy każdej innej liczbie, a szum czyta się jak brak zdania. Stanu tu nie ma
 * i nie jest potrzebny: pole jest sterowane, więc „po przycięciu" i „ósemka wpisana wprost"
 * to ten sam widok — i ta sama prawda o suficie. */
function CopiesRow({
  value,
  onEditStep,
}: {
  value: number;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
    <div data-row="how-many-at-once" className="stack">
      <label htmlFor="step-copies" className="label">
        How many at once
      </label>
      <input
        id="step-copies"
        className={FIELD}
        type="number"
        min={1}
        max={MOST_COPIES}
        value={String(value)}
        onChange={(event) => {
          onEditStep({ copies: copiesFrom(event.target.value) });
        }}
      />
      <span className="lead">
        More than one only helps when the copies work on different questions.
      </span>
      {value < MOST_COPIES ? null : (
        <span className="lead">
          Eight at once is the most, so anything higher lands here as eight.
        </span>
      )}
    </div>
  );
}

/** Wiersz „Try again up to" — liczba rund POWROTU wychodzącego z tego kroku.
 *
 * NIE MA GO W MAKIECIE, i mówię to wprost, bo makieta jest jedyną wyrocznią wyglądu tej
 * aplikacji. Grep na `loop`, `again`, `retry` i `turns` w `docs/mockup/index.html` nie daje ani
 * jednego trafienia: pętla powstała po makiecie, na prośbę właściciela, więc tej kontrolki nie ma
 * skąd przepisać. Kształt jest zapożyczony z wiersza „How many at once", bo pyta o to samo —
 * o jedną liczbę należącą do kroku.
 *
 * DLACZEGO NA KROKU, SKORO LICZBA NALEŻY DO STRZAŁKI. Panelu strzałki w tym repo nie ma i nie
 * powstaje przy okazji. Rust już rozstrzygnął, gdzie to postawić: uwaga o złym zakresie
 * (`check::turns_out_of_range`) czepia się kroku, z którego powrót WYCHODZI — „to on jest sędzią
 * pętli i to jego kafelek człowiek otworzy, żeby zmienić tę liczbę" — a kliknięcie tej uwagi
 * otwiera panel dokładnie tego kroku. Postawienie kontrolki gdzie indziej znaczyłoby, że uwaga
 * prowadzi w miejsce, w którym nie da się jej spełnić.
 *
 * WIERSZA NIE MA, GDY Z KROKU NIE WYCHODZI POWRÓT. Pole „ile rund" przy kroku bez pętli jest
 * kontrolką bez skutku (niezmiennik 16) — i to gorszego rodzaju, bo wyglądałoby na ustawienie,
 * które czeka na włączenie gdzie indziej. */
/** Co się dzieje z robotą, kiedy ten krok nie przejdzie.
 *
 * 2026-08-23, zamówienie właściciela: „workflows zawsze ma mieć opcje kontynuacji a nie ślepe
 * punkty". Do tego dnia każdy nieudany krok kasował cały stożek potomków — i nie było gdzie
 * powiedzieć, że ma być inaczej.
 *
 * STOI PRZY KAŻDYM KROKU AGENTA, nie tylko przy sędzim pętli. Krok, który padł zwyczajnie, był
 * dokładnie tym samym ślepym punktem, co sędzia po wyczerpaniu prób — a kontrolka pokazana
 * tylko przy jednym z nich kazałaby zgadywać, czemu drugi jej nie ma.
 *
 * `select`, nie trzy przyciski: to jest wybór jednej z trzech wykluczających się odpowiedzi,
 * czyli dokładnie to, do czego lista służy — i ten sam kształt, co pozostałe pola tego panelu. */
function WhenItFailsRow({
  value,
  onEditStep,
}: {
  value: WhenItFails | undefined;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
    <div data-row="when-it-fails" className="stack">
      <label htmlFor="step-when-it-fails" className="label">
        If this step does not pass
      </label>
      <select
        id="step-when-it-fails"
        className={FIELD}
        value={value ?? 'carry-on'}
        onChange={(event) => {
          onEditStep({ whenItFails: event.target.value as WhenItFails });
        }}
      >
        <option value="stop">Stop here</option>
        <option value="carry-on">Carry on anyway</option>
        <option value="ask-me">Ask me what to do</option>
      </select>
      <span className="lead">
        Carrying on is what a step does unless you say otherwise: the work goes to the steps after
        it even though it did not pass, and they are told so.
      </span>
    </div>
  );
}

function TriesRow({
  value,
  onEditWayBack,
}: {
  value: number;
  onEditWayBack: (turns: number) => void;
}): ReactElement {
  return (
    <div data-row="try-again" className="stack">
      <label htmlFor="step-tries" className="label">
        Try again up to
      </label>
      <input
        id="step-tries"
        className={FIELD}
        type="number"
        min={1}
        max={10}
        value={String(value)}
        onChange={(event) => {
          onEditWayBack(turnsFrom(event.target.value));
        }}
      />
      <span className="lead">
        The tester sends the work back until it passes, or until the tries run out.
      </span>
    </div>
  );
}

/** Liczba tur z pola tekstowego, przycięta do zakresu, który przyjmuje plik.
 *
 * Pole `number` przepuszcza pustkę i tekst, a `Number('')` to zero — czyli pętla, która nie
 * wykonuje się ani razu i którą walidator odrzuca. Zaciskamy TUTAJ, bo dokument ma być poprawny
 * po każdym naciśnięciu klawisza: zapis leci autosavem 400 ms po ostatniej zmianie i nie ma
 * chwili, w której wolno mu być nieprawidłowy. To ta sama decyzja, co przy `copiesFrom`. */
function turnsFrom(typed: string): number {
  const value = Number.parseInt(typed, 10);
  if (Number.isNaN(value)) return 1;
  return Math.min(10, Math.max(1, value));
}

/* WŁASNEJ LISTY MIEJSC PRACY TU JUŻ NIE MA (2026-08-31).
 *
 * Stała tu trzecia kopia tej samej decyzji — obok `check-panel.tsx` i `serve-panel.tsx` —
 * z własnym pytaniem („Where it works" kontra „Where it runs") i własnymi brzmieniami tych
 * samych trzech odpowiedzi. Powód scalenia w całości stoi w `./where-it-works.tsx`; tutaj
 * zostaje tylko to, co było w tej kopii prawdziwe i zostało przeniesione: `pick` nie zaznacza
 * niczego (niezmiennik 17), a wybór jest jednym faktem o `folder` — kliknięcie zapisuje jedną
 * wartość, ponowne otwarcie zaznacza tę samą [T-151]. */

export function StepPanel({
  step,
  agent,
  agents,
  onChooseAgent,
  onCreateAgent,
  onEdit,
  onEditStep,
  onReset,
  more,
}: StepPanelProps): ReactElement {
  /* Wartości EFEKTYWNE do pokazania i lista zmienionych pól — jedno wywołanie, jeden fakt.
   * Licznik „N changed" jest długością tej listy i nie istnieje nigdzie indziej: osobna liczba
   * trzymana w stanie kroku rozjeżdża się z patchem przy pierwszym `Reset`, a obie wyglądają
   * wiarygodnie (niezmiennik 13). */
  const { agent: effective, changed } = resolve(agent, step.overrides);

  /** Kropka, `Reset` i szary wiersz — wszystko, co odróżnia wiersz zmieniony od dziedziczonego. */
  const mark = (field: OverridableField) =>
    changed.includes(field) ? (
      <>
        <span className="label">●</span>
        <button type="button" className={QUIET} onClick={() => onReset(field)}>
          Reset
        </button>
      </>
    ) : null;

  const wasUsing = (field: OverridableField) =>
    changed.includes(field) ? (
      <span className="lead">Agent uses: {agentUses(field, agent)}</span>
    ) : null;

  /* Szare wiersze dla ustawień, które zmieniono, a które nie mają własnej kontrolki. Liczą się
   * do tego, co stoi w środku: bez nich zmiana `thinking` znikałaby z ekranu — licznik mówiłby
   * „1 changed", a człowiek nie miałby jak zobaczyć CZEGO ani jak to cofnąć. */
  const grey = noRowOfTheirOwn(changed);
  const brought = more ?? [];

  return (
    <>
      <NameRow value={step.name} onEditStep={onEditStep} />

      <WhoDoesThis
        chosen={step.agent}
        agents={agents}
        onChooseAgent={onChooseAgent}
        onCreateAgent={onCreateAgent}
        note="This comes from the agent. Changing it here does not change the agent."
      />

      <WhatToDoRow value={step.instructions} onEditStep={onEditStep} />

      <WhereItWorks
        group="step-folder"
        value={step.folder}
        onChoose={(folder) => {
          onEditStep({ folder });
        }}
      />

      <CopiesRow value={step.copies} onEditStep={onEditStep} />

      {/* WSZYSTKO PONIŻEJ MA DZIAŁAJĄCĄ WARTOŚĆ DZIEDZICZONĄ Z AGENTA, więc krok, którego nikt
          tu nie tknął, biegnie poprawnie. To jest cały warunek, pod którym wolno to schować:
          za pokrywą nie stoi ani jedno pole, które trzeba wypełnić, żeby ruszyć. */}
      <MoreSettings inside={3 + grey.length + brought.length} changed={changed.length}>
        <div data-row="can-it-change-files" className="stack">
          <div className="flex items-baseline gap-2">
            <label htmlFor="step-file-access" className="label">
              Can it change files
            </label>
            {mark('fileAccess')}
          </div>
          <select
            id="step-file-access"
            className={FIELD}
            value={effective.fileAccess}
            onChange={(event) => {
              onEdit({ fileAccess: fileAccessFrom(event.target.value, effective.fileAccess) });
            }}
          >
            {FILE_ACCESS.map((one) => (
              <option key={one.value} value={one.value}>
                {one.label}
              </option>
            ))}
          </select>
          {wasUsing('fileAccess')}
        </div>

        <div data-row="give-up-after" className="stack">
          <div className="flex items-baseline gap-2">
            <label htmlFor="step-give-up-after" className="label">
              Give up after
            </label>
            {mark('giveUpAfterMinutes')}
          </div>
          {/* JEDNOSTKA PRZY POLU, nie w głowie autora. Do 2026-08-31 stało tu gołe pole
              liczbowe: „20" bez ani jednego słowa o tym, czego jest dwadzieścia. Jednostkę
              znała wyłącznie `minutes()`, czyli szary wiersz „Agent uses: …" — widoczny dopiero
              wtedy, gdy krok już się od agenta różni. */}
          <div className="flex items-baseline gap-2">
            <input
              id="step-give-up-after"
              className={FIELD}
              type="number"
              min={0}
              value={String(effective.giveUpAfterMinutes)}
              onChange={(event) => {
                onEdit({ giveUpAfterMinutes: minutesFrom(event.target.value) });
              }}
            />
            <span className="label">minutes</span>
          </div>
          {/* ZERO MÓWI, ŻE JEST ZEREM. `minutesFrom('')` daje 0, a 0 znaczy „bez limitu" — więc
              wyczyszczenie tego pola zdejmowało krokowi limit czasu i pole pokazywało wtedy
              „0", a nie „no limit". Zdanie stoi TYLKO przy zerze: napisane pod polem na stałe
              byłoby prawdą o czymś, co się właśnie nie dzieje, przy każdej innej liczbie. */}
          {effective.giveUpAfterMinutes === 0 ? (
            <span className="lead">
              Zero is no limit: this step runs until it is done. Type a number to give it one.
            </span>
          ) : null}
          {wasUsing('giveUpAfterMinutes')}
        </div>

        <div data-row="write-results-to" className="stack">
          <div className="flex items-baseline gap-2">
            <label htmlFor="step-write-results-to" className="label">
              Write results to
            </label>
            {mark('writeResultsTo')}
          </div>
          <input
            id="step-write-results-to"
            className={FIELD}
            value={effective.writeResultsTo}
            onChange={(event) => {
              onEdit({ writeResultsTo: event.target.value });
            }}
          />
          {wasUsing('writeResultsTo')}
        </div>

        {grey.map((field) => (
          <div key={field} data-row={field} className="flex items-baseline gap-2">
            {mark(field)}
            {wasUsing(field)}
          </div>
        ))}

        {brought}
      </MoreSettings>
    </>
  );
}

/** 1–8 kopii [T3 §4.4]. Poza zakresem wraca do jedynki, bo „zero kopii" to krok, który nie
 * biegnie, a taki krok kasuje się, nie zeruje.
 *
 * Sufit jest tą samą stałą, którą wiersz wypisuje w polu (`max`) i nazywa zdaniem — do
 * 2026-08-31 była wpisana w trzech miejscach jako trzy różne ósemki, a przycięcie nie miało
 * na ekranie ani jednego słowa. */
function copiesFrom(raw: string): number {
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed)) return 1;
  return Math.min(MOST_COPIES, Math.max(1, parsed));
}

/** „Bez limitu" to zero, nigdy pusta wartość [T4 §4.3, reguła 1]. */
function minutesFrom(raw: string): number {
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

/** Wariant z listy albo dotychczasowy. Rzutowanie napisu z DOM-u na wariant enuma byłoby
 * obietnicą, której ten napis nie składa. */
function fileAccessFrom(raw: string, now: FileAccess): FileAccess {
  return FILE_ACCESS.find((one) => one.value === raw)?.value ?? now;
}

export interface PickAnAgentProps {
  step: AgentStep;
  /** Biblioteka agentów. Pusta znaczy „nie ma z czego wybierać" i mówimy to zdaniem. */
  agents: readonly Agent[];
  onChooseAgent: (agentId: string) => void;
  onCreateAgent: () => void;
  onEditStep: (fields: AgentStepFields) => void;
}

/** Panel kroku, który nie ma jeszcze agenta — czyli KAŻDEGO kroku prosto z `＋ Add step`.
 *
 * Dlaczego osobny komponent, a nie ten sam panel z ukrytą częścią: wiersze `StepPanel` stojące
 * za ujawnieniem pokazują wartości EFEKTYWNE, a te nie istnieją, dopóki nie ma od kogo
 * dziedziczyć — więc ujawnienie nazwałoby liczbę rzeczy, których nie ma z czego policzyć.
 * Wypisanie w nich zer i pustych napisów byłoby ekranem, który mówi nieprawdę o tym, co się
 * stanie po uruchomieniu, a wyszarzenie ich obiecuje ustawienie „na później".
 *
 * CZTERY WIERSZE, NIE DWA — poprawione 2026-08-18. Do tego dnia stały tu Name i Who does this,
 * i to wszystko. `What to do` oraz `How many at once` są polami SAMEGO kroku: nie dziedziczą
 * niczego, nie mają wartości efektywnej i nie ma żadnego powodu, żeby czekały na wybór agenta.
 * Skutek tamtej wersji był zmierzony na dysku właściciela: oba jego pliki mają
 * `"instructions": ""` przy każdym kroku. */
function PickAnAgent({
  step,
  agents,
  onChooseAgent,
  onCreateAgent,
  onEditStep,
}: PickAnAgentProps): ReactElement {
  return (
    <div data-needs-agent className="flex flex-col gap-3">
      <NameRow value={step.name} onEditStep={onEditStep} />

      <WhoDoesThis
        chosen={step.agent}
        agents={agents}
        onChooseAgent={onChooseAgent}
        onCreateAgent={onCreateAgent}
        note="This step waits here until it has someone to do it."
      />

      <WhatToDoRow value={step.instructions} onEditStep={onEditStep} />

      <CopiesRow value={step.copies} onEditStep={onEditStep} />
    </div>
  );
}

export interface PanelForStepProps {
  /** Zaznaczony kafelek — DOWOLNEGO rodzaju. Rozstrzygnięcie, co z nim zrobić, jest niżej. */
  step: Step;
  /** Biblioteka agentów: panel pokazuje wartości efektywne, więc musi znać agenta kroku. */
  agents: readonly Agent[];
  /**
   * Umiejętności, które NAPRAWDĘ leżą w katalogach agentów (`list_skills`).
   *
   * Nazwy, nie obiekty: wiersz Skills zapisuje w kroku listę nazw i niczego więcej o nich nie
   * wie. Pusta lista znaczy „nie ma czego wybierać" i wiersz wtedy nie powstaje.
   */
  skills: readonly string[];
  onChooseAgent: (agentId: string) => void;
  /** Skrót na sekcję Agents — z pozycji `＋ Create a new agent…` i z pustej biblioteki. */
  onCreateAgent: () => void;
  /** Agent jedzie Z POWROTEM do wołającego, bo to tutaj rozwiązuje się `step.agent`
   * (niezmiennik 13). Ekran, który rozwiązywałby go drugi raz u siebie, mógłby rozwiązać
   * inaczej i pokazać wartości efektywne innego agenta niż ten, którego panel nazywa. */
  onEdit: (agent: Agent, edit: Overrides) => void;
  onEditStep: (fields: AgentStepFields) => void;
  onEditCheckpoint: (fields: CheckpointFields) => void;
  onEditServe: (fields: ServeFields) => void;
  /**
   * Kto po strzałce stoi przed kafelkiem „uruchom i zostaw", czy już oddaje jego pole, i jak go
   * o nie poprosić.
   *
   * Liczy to EDYTOR, nie ten panel: panel nie zna strzałek, więc nie ma jak pomylić się co do
   * tego, który krok jest tym przed (ten sam ruch i ten sam powód, co przy `wayBack`).
   */
  stepBefore?: string | null | undefined;
  handsItOver?: boolean | undefined;
  onAskTheStepBefore?: (() => void) | undefined;
  /**
   * Zmiana pola kafelka „sprawdź". Brak propsu znaczy „ten ekran sprawdzenia nie edytuje".
   *
   * OPCJONALNY, i to jest decyzja o kształcie, nie niedbałość. Trzy kryteria spoza tego zadania
   * (`tries-row`, `fresh-copy-row`, `when-it-fails-row`) montują `PanelForStep` z kompletem
   * propsów, więc nowy OBOWIĄZKOWY wywróciłby je na typach — czyli kazałby dopisać wiersz
   * w trzech plikach, których to zadanie nie posiada. Dokładanie addytywne zostawia je
   * nietknięte i nic nie kosztuje: ten prop czyta wyłącznie gałąź kroku `check`, a żaden
   * z tamtych trzech takiego kroku nie renderuje.
   */
  onEditCheck?: (fields: CheckFields) => void;
  onReset: (field: OverridableField) => void;
  onChooseSkills: (choice: SkillChoice) => void;
  /**
   * Ile rund ma powrót wychodzący z tego kroku, albo `null`, gdy żaden z niego nie wychodzi.
   *
   * Liczba, nie strzałka: panel nie ma potrzeby wiedzieć, DOKĄD ten powrót prowadzi, a im mniej
   * dokumentu tu wchodzi, tym mniej jest miejsc, w których panel mógłby coś o nim skłamać.
   */
  wayBack: number | null;
  /** Nowa liczba rund dla tego powrotu. Nie ma jak jej podać, gdy `wayBack` jest `null`. */
  onEditWayBack: (turns: number) => void;
}

/** Droga powrotna dla ekranu, który sprawdzenia nie edytuje — patrz `onEditCheck` wyżej.
 *
 * Nie jest to kontrolka bez skutku (niezmiennik 16): ekran, który tego propsu nie podaje, nie
 * renderuje ani jednego kafelka `check`, więc nie ma czym w to kliknąć. */
const nowhereToWrite = (): void => undefined;

/** Jaki panel dostaje zaznaczony kafelek. Cztery odpowiedzi i ani jednego „nic".
 *
 * Kafelek bez panelu jest kafelkiem, którego nie da się skonfigurować — a płótno pozwala
 * postawić go jednym kliknięciem. Dlatego ta funkcja jest CAŁKOWITA: nie ma wejścia, dla
 * którego oddałaby `null`. Dopóki decyzja mieszkała w `editor.tsx` jako warunek
 * `open === undefined || agentOf === undefined`, dwa z trzech wejść dostawały zdanie
 * „Pick a step to see what it was given." — czyli odpowiedź na zupełnie inne pytanie.
 *
 * RAMKA JEST TUTAJ, jedna, od 2026-08-18. Wcześniej każdy z trzech paneli rysował własne
 * `<aside class="w-82 border-l bg-panel p-4">` WEWNĄTRZ `<aside>` szerokości 330 px, którą
 * rysuje `editor.tsx` — czyli dwie ramki, dwa razy padding i poziomy pasek przewijania na
 * każdym otwartym kroku. Ramkę ma teraz ekran (bo to on wie, ile miejsca oddał kolumnie),
 * a panele są jej treścią. */
export function PanelForStep({
  step,
  agents,
  skills,
  onChooseAgent,
  onCreateAgent,
  onEdit,
  onEditStep,
  onEditCheckpoint,
  onEditServe,
  stepBefore,
  handsItOver,
  onAskTheStepBefore,
  onEditCheck,
  onReset,
  onChooseSkills,
  wayBack,
  onEditWayBack,
}: PanelForStepProps): ReactElement {
  if (step.kind === 'checkpoint') {
    return (
      <div data-step-panel className="enter flex flex-col gap-3">
        <CheckpointPanel step={step} onEditStep={onEditCheckpoint} />
      </div>
    );
  }

  if (step.kind === 'check') {
    return (
      <div data-step-panel className="enter flex flex-col gap-3">
        <CheckPanel step={step} onEditStep={onEditCheck ?? nowhereToWrite} />
      </div>
    );
  }

  if (step.kind === 'serve') {
    return (
      <div data-step-panel className="enter flex flex-col gap-3">
        <ServePanel
          step={step}
          onEditStep={onEditServe}
          stepBefore={stepBefore}
          handsItOver={handsItOver}
          onAskTheStepBefore={onAskTheStepBefore}
        />
      </div>
    );
  }

  const agent = agents.find((one) => one.id === step.agent);
  if (agent === undefined) {
    return (
      <div data-step-panel className="enter flex flex-col gap-3">
        <PickAnAgent
          step={step}
          agents={agents}
          onChooseAgent={onChooseAgent}
          onCreateAgent={onCreateAgent}
          onEditStep={onEditStep}
        />
      </div>
    );
  }

  return (
    <div data-step-panel className="enter flex flex-col gap-3">
      <AgentPanel
        step={step}
        agent={agent}
        agents={agents}
        skills={skills}
        onChooseAgent={onChooseAgent}
        onCreateAgent={onCreateAgent}
        onEdit={onEdit}
        onEditStep={onEditStep}
        onReset={onReset}
        onChooseSkills={onChooseSkills}
        wayBack={wayBack}
        onEditWayBack={onEditWayBack}
      />
    </div>
  );
}

interface AgentPanelProps extends Omit<StepPanelProps, 'onEdit' | 'more'> {
  skills: readonly string[];
  /** Agent jedzie Z POWROTEM do wołającego — ten sam powód, co w `PanelForStepProps`. */
  onEdit: (agent: Agent, edit: Overrides) => void;
  onChooseSkills: (choice: SkillChoice) => void;
  wayBack: number | null;
  onEditWayBack: (turns: number) => void;
}

/**
 * Panel kroku, którego agent DAŁ SIĘ rozwiązać — czyli `StepPanel` razem z wierszami, które są
 * osobnymi komponentami.
 *
 * 2026-08-31 — WYDZIELONY Z `PanelForStep`, i to nie jest porządkowanie. `PanelForStep` ma cztery
 * wcześniejsze wyjścia (punkt kontrolny, sprawdzenie, „uruchom i zostaw", krok bez agenta),
 * a pytanie o to, co ten folder ma do użyczenia, jest hakiem — haka nie wolno wołać za
 * warunkiem. Tutaj żadnego warunku przed nim nie ma.
 *
 * Odsianie wierszy dzieje się TU, przed oddaniem ich panelowi, bo zwinięta pokrywa nazywa ich
 * liczbę: wiersz, który odda `null`, policzony jako stojący, robi z tej liczby zdanie o panelu,
 * którego nikt nie zobaczy. Odpowiedzi „czy ten wiersz powstanie" są w plikach samych wierszy
 * i to te same, którymi rozstrzygają to u siebie.
 */
function AgentPanel({
  step,
  agent,
  agents,
  skills,
  onChooseAgent,
  onCreateAgent,
  onEdit,
  onEditStep,
  onReset,
  onChooseSkills,
  wayBack,
  onEditWayBack,
}: AgentPanelProps): ReactElement {
  const material = useHostMaterial();
  const borrows = step.borrow ?? {};

  const more: ReactNode[] = [];

  /* Liczba rund powrotu — tylko na kroku, z którego powrót wychodzi. */
  if (wayBack !== null) {
    more.push(<TriesRow key="try-again" value={wayBack} onEditWayBack={onEditWayBack} />);
  }

  /* Co się dzieje, kiedy próby się skończą — albo kiedy krok padnie z każdego innego powodu.
     Stoi przy każdym kroku agenta, bo ślepy punkt jest ten sam niezależnie od tego, dlaczego
     krok nie przeszedł. */
  more.push(
    <WhenItFailsRow key="when-it-fails" value={step.whenItFails} onEditStep={onEditStep} />,
  );

  /* CO TEN KROK PŁACI NASTĘPNEMU. Stoi zaraz za „co, gdy nie przejdzie", bo obie odpowiedzi
     dotyczą tego samego: co wychodzi z tego kafelka i co dostaje ten za nim. Powód, dla którego
     ten wiersz w ogóle powstał, stoi w całości w `./handover-row.tsx`. */
  more.push(<HandoverRow key="handover" value={step.handover} onEditStep={onEditStep} />);

  /* Wiersza Skills nie ma przy agencie na Codeksie ani przy pustym katalogu umiejętności —
     powód w całości stoi przy `skillsRowStands`. */
  if (skillsRowStands(agent.runsWith, skills)) {
    more.push(
      <SkillsRow
        key="skills"
        mode={SKILL_SUBSETTING}
        runsWith={agent.runsWith}
        available={[...skills]}
        value={step.skills}
        onChoose={onChooseSkills}
      />,
    );
  }

  /* Co ten kafelek bierze z repozytorium, w którym pracuje bieg. Jedzie tą samą drogą, co nazwa
     i instrukcje — `onEditStep`, czyli `commit`, czyli autosave — bo `borrow` jest polem samego
     KROKU, a nie nadpisaniem agenta. */
  if (borrowRowStands(material, borrows)) {
    more.push(
      <BorrowRow
        key="borrow"
        material={material}
        value={borrows}
        onChoose={(borrow) => {
          /* Pusty wybór jedzie jako BRAK KLUCZA, nie `{}`: `JSON.stringify` zdejmuje pole
             o wartości `undefined`, więc krok, z którego wszystko odznaczono, wraca do kształtu
             pliku sprzed tego zadania — co do bajtu. */
          onEditStep({ borrow: nothingBorrowed(borrow) ? undefined : borrow });
        }}
      />,
    );
  }

  return (
    <StepPanel
      step={step}
      agent={agent}
      agents={agents}
      onChooseAgent={onChooseAgent}
      onCreateAgent={onCreateAgent}
      onEdit={(edit) => {
        onEdit(agent, edit);
      }}
      onEditStep={onEditStep}
      onReset={onReset}
      more={more}
    />
  );
}
