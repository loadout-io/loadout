/* Kafelek kroku na płótnie biegu. Cztery linie tekstu, stan wyrażony FORMĄ.
 *
 * TEN SAM OBIEKT, CO KAFELEK EDYTORA (`workflows/canvas/tile.tsx`): ta sama karta, ten sam
 * wiersz nazwy, ta sama czwarta linia wyliczona ze strzałek. To jest cała teza tej przebudowy
 * — człowiek, który ułożył graf, ma zobaczyć w biegu TEN graf, a nie drugi rysunek, który
 * wygląda podobnie. Szerokość 246 px z makiety nakłada płótno, nie ta karta (patrz `CARD`):
 * ten sam kafelek stoi też w liście, gdzie stała szerokość byłaby kolumną wąskich pudełek.
 *
 * STAN JEST KSZTAŁTEM, NIE NOWYM KOLOREM. Cztery barwy semantyczne w tej aplikacji są całym
 * słownikiem (DESIGN §3), a dwie z nich — `--live` i `--fail` — dzieli 13 stopni odcienia
 * i potrafią stać na sąsiednich kafelkach. Rozstrzyga więc FORMA i jest ona ROZŁĄCZNA:
 *   `--live` = podkład, obrys i pulsująca kropka,
 *   `--fail` = glif i lewa krawędź bloku.
 * Pilnuje tego statycznie `../live-and-fail-never-share-a-form.test.ts`, na wszystkich plikach
 * tej sekcji naraz.
 *
 * CZEGO TU NIE MA: KLASY `.enter`. Kafelek edytora ją nosi, ten nie — i to nie jest
 * przeoczenie. Sufit z ARCHITECTURE §7 to DWA regiony animujące się na jedno zdarzenie, a ruch
 * na tym ekranie ma nieść treść, nie wejście pudełka.
 *
 * 2026-08-31 (druga runda) — KROPKA `animate-blip` NA KROKU, KTÓRY PRACUJE, JEST. Poprzednia
 * runda zgłosiła jej brak jako odstępstwo, bo oba sloty §7 były wtedy wydane: kropka strefy
 * TERAZ i kropka żywej karty. Strefa TERAZ zeszła z ekranu razem z drugą kopią planu, więc slot
 * się zwolnił — zmierzone `../exactly-one-thing-pulses.test.ts`: przed tą zmianą liczył JEDEN
 * literał `animate-*` w całym `src/` przy suficie 2.
 *
 * TRZECIEGO RODZAJU RUCHU NIE MA I NIE MOŻE BYĆ. Płynąca kreska po strzałce, którą przyszła
 * praca, była dla tamtej wyroczni niewidzialna (ruch definiuje arkusz biblioteki, a kod mówi
 * tylko `animated`), więc rodzajów byłoby trzy przy suficie dwa. Strzałka przestała płynąć
 * i została przy samej barwie — powód w całości stoi przy `LIVE_ARROW` w `./model.ts`.
 *
 * SŁOWO O STANIE JEST TU OD 2026-08-31, i to jest naprawa dostępności, nie ozdoba. Stan agenta
 * stał SŁOWEM w kolumnie agentów; kolumna zeszła z ekranu, a ten kafelek mówił stan WYŁĄCZNIE
 * formą — sześć rozłącznych kompletów klas plus glif z `aria-hidden`. Osoba, która nie odróżnia
 * dwóch przygaszonych odcieni, traciła tę informację w całości, a `aria-label` jest odpowiedzią
 * na ślepotę, nie na daltonizm: ona ten kafelek WIDZI i czytnika nie ma włączonego.
 *
 * Słowo nie jest PIĄTĄ LINIĄ i nie ma prawa nią być [ARCHITECTURE §7, `../graph/each-state-
 * draws-its-own-step.test.tsx`]. Stoi na wierszu nazwy, jako chip — czyli w slocie, w którym
 * do dziś stał sam glif. To nie jest też drugi dom jednego faktu: kolumny agentów, która mówiła
 * to samo, już nie ma, więc miejsce jest dokładnie jedno (niezmiennik 13).
 */
import type { CSSProperties, ReactElement } from 'react';
import type { AgentStatus } from '../rail/card';
import { initialsOf } from '../feed/who';
import type { GraphStep, Plan, Who } from './model';
import { measureOf } from './model';

export interface RunTileProps {
  step: GraphStep;
  /** Cały plan, bo czwarta linia nazywa poprzednika NAZWĄ i liczy się ze strzałek. */
  plan: Plan;
  /**
   * Wejście w tego, kto ten krok robi — albo `undefined`, kiedy nie ma dokąd wejść.
   *
   * 2026-08-31 — DOSZŁO RAZEM ZE ZDJĘCIEM PRAWEJ KOLUMNY. Do dziś jedyną drogą do ekranu
   * jednego agenta był kafelek w liście agentów, a ta lista zniknęła: `openAgent` zostałby
   * wtedy funkcją bez ani jednego produkcyjnego wołającego, a `session/` — 354 liniami
   * z kompletem testów i zerem wołających, czyli tą samą wadą, dla której to repo powstało
   * (niezmiennik 16). Kafelek jest więc przyciskiem dokładnie wtedy, gdy wołający ma co
   * z tym kliknięciem zrobić.
   *
   * OPCJONALNE, bo krok, o którym strumień nic nie powiedział, nie ma czego otworzyć —
   * a przycisk, który otwiera pusty ekran, jest kontrolką bez skutku z dodatkowym krokiem.
   */
  onOpen?: () => void;
  /**
   * Gdzie ta karta stoi w siatce, którą rozkłada wołający — albo brak, kiedy sama się układa.
   *
   * 2026-08-31 — DOSZŁO RAZEM ZE ŚCIEŻKĄ KROKÓW (`./path.tsx`). Kolumna planu jest od dziś
   * siatką dwukolumnową: znaczniki po lewej, karty po prawej, wiersze jawne. Karta MUSI dostać
   * swoje miejsce od tej siatki, a nie od opakowania, bo cudze kryteria liczą kafelki jako
   * BEZPOŚREDNIE dzieci listy (`e2e/tests/t161-long-workflow-stays-inside-run.spec.ts`,
   * `:scope > [data-step]`) — `<div>` na krok schowałby przed nimi każdy z nich naraz.
   *
   * Wyłącznie miejsce w siatce: barwy, obrysu ani stopnia tędy nie podaje nikt i podawać nie
   * ma prawa — forma stanu mieszka w `TONE` wyżej i tam jest sądzona.
   */
  style?: CSSProperties;
}

/* Karta z makiety (`.node`), bez szerokości: na płótnie narzuca ją komponent węzła (246 px,
 * kolumny), a na liście kafelek wypełnia swój wiersz. Szerokość wpisana tutaj robiłaby z listy
 * kolumnę wąskich pudełek przy szerokim oknie. */
/* KAFELEK KROKU JEST CIASNIEJSZY NIZ DOMOWA KARTA — zgloszenie wlasciciela 2026-09-01:
 * „troche zmniejsz te kafelki, dzieki temu bedzie wiecej space na terminal nasz". `.card` daje
 * 12 px marginesu wewnetrznego i jest wspolna dla calego produktu; kolumna krokow to dziesiec
 * takich kart jedna pod druga, wiec kazdy piksel liczy sie tam dziesiec razy. `p-[9px]` nadpisuje
 * sam ten wymiar i nie rusza ani obrysu, ani promienia, ani tla — reszta karty zostaje domowa. */
const CARD = 'card grid gap-[2px] bg-raised p-[9px] text-body';

/**
 * Sześć stanów, sześć kompletów klas — `Record`, nie `switch` z gałęzią domyślną.
 *
 * Siódmy stan dopisany do `AgentStatus` przestaje TU się kompilować, zamiast po cichu wpaść
 * w „resztę" i dostać formę, której nikt mu nie przydzielił.
 *
 * `done` i `stopped` mają ten sam komplet z rozmysłem: obie rzeczy się już nie dzieją, więc
 * obie są ciche. Różni je GLIF — przekreślone kółko przy tej, którą ktoś zatrzymał.
 */
const TONE: Readonly<Record<AgentStatus, string>> = {
  /* Czeka na kolegę, nie na ciebie: przerywany obrys, żadnego wypełnienia, żadnego ruchu.
   * Pomarańczowy przy każdym bezczynnym kroku to sposób, w jaki kolor przestaje znaczyć. */
  waiting: 'border border-dashed border-line',
  /* Dzieje się TERAZ: podkład, obrys i poświata — trzy wystąpienia jednej barwy, wszystkie
   * z listy form `--live` (DESIGN §3). Poświatę niesie `./graph.css`, bo cień nie ma tokenu
   * w tej barwie, a hex w kodzie komponentu jest zakazany. Czwartą formą z tej samej listy
   * jest pulsująca kropka i stoi ona w chipie stanu (`StateChip`). */
  working: 'loadout-run-glow border border-live-edge bg-live-soft',
  /* Czeka na CIEBIE — sam obrys. Wypełnienie należy do „dzieje się teraz", a tu nic się nie
   * dzieje: to jest dokładnie ta chwila, w której bieg stoi i czeka na człowieka. */
  'needs you': 'border border-attend-edge',
  done: 'border border-line opacity-50',
  /* Lewa krawędź bloku, i ani jednego piksela wypełnienia. Wypełnienie znaczy „teraz". */
  failed: 'border border-line border-l-2 border-l-fail-edge',
  stopped: 'border border-line opacity-50',
};

/**
 * Glif obok słowa — dla dwóch stanów, i oba z tego samego powodu: skończyły się inaczej,
 * niż miały. `Record`, nie `if`, bo siódmy stan ma tu przestać się kompilować.
 *
 * Pusty napis znaczy „ten stan nie ma glifu", a nie „glif jest pusty": chip pomija go wtedy
 * w całości, zamiast rezerwować dla niego miejsce.
 */
const GLYPH: Readonly<Record<AgentStatus, string>> = {
  waiting: '',
  working: '',
  'needs you': '',
  done: '',
  failed: '✕',
  stopped: '⊘',
};

/**
 * Stan kroku na wierszu nazwy: SŁOWO, a przy nim to, co widać kątem oka.
 *
 * TRZY NOŚNIKI JEDNEGO FAKTU, KAŻDY DLA INNEGO CZYTELNIKA, i żaden nie jest jedyny. Napis
 * czyta każdy — także osoba, która nie odróżnia dwóch przygaszonych odcieni, i to dla niej ten
 * chip w ogóle powstał. Glif odróżnia dwa stany, które skończyły się nie tak, jak miały, bez
 * czytania. Kropka pulsuje na jednym jedynym stanie i mówi „to dzieje się TERAZ" — czwarta
 * forma z listy `--live` (DESIGN §3) i jedyny ruch, jaki ten obraz ma.
 *
 * SŁOWEM JEST SAM `status`, nie jego tłumaczenie. Druga tabela stan → napis rozjechałaby się
 * z tą, którą ten sam zbiór wartości ma w `../rail/card.ts`, a rozjazd byłby cichy: obie
 * wersje dalej rysowałyby napis. Jedyny dopisek nie tłumaczy stanu: „carried on” jest
 * osobnym, jawnym faktem schedulera dowiezionym przez krok.
 *
 * BARWA TYLKO TAM, GDZIE JEST FORMĄ TEGO STANU. `--fail` ma na tej liście glif i lewą krawędź
 * bloku, więc chip z glifem barwę bierze; pozostałe pięć stanów jej nie dostaje i to jest cała
 * teza tej naprawy — chip pomalowany na sześć sposobów byłby szóstym miejscem, w którym stan
 * mieszka w kolorze, i znowu ani jednym, w którym mieszka w słowie.
 */
function StateChip({
  status,
  carriedOn,
}: {
  status: AgentStatus;
  carriedOn: boolean;
}): ReactElement {
  const glyph = GLYPH[status];
  return (
    <span className="chip shrink-0" {...(status === 'failed' ? { 'data-tone': 'fail' } : {})}>
      {status === 'working' ? (
        <i aria-hidden className="block size-2 shrink-0 animate-blip rounded-pill bg-live" />
      ) : null}
      {glyph === '' ? null : <span aria-hidden>{glyph}</span>}
      {status}
      {carriedOn ? ' — carried on' : ''}
    </span>
  );
}

/**
 * Napis, którym ta twarz się podpisuje — czyli nazwa, KTÓRĄ TEN ZNAK IDENTYFIKUJE.
 *
 * DWIE ODPOWIEDZI, BO PLAN ZERUJE NAZWĘ WYKONAWCY, kiedy agent nazywa się tak jak krok
 * (`../index.tsx`, `planFor`) — a w prawdziwym biegu nazywa się tak ZAWSZE, bo podpis agenta
 * w strumieniu JEST nazwą kroku (`commands/run.rs` woła `forward(…, step.name)`). Zmierzone
 * 2026-08-31 w chromium na pełnej drodze produktu: `who.name` przyjechało puste na obu
 * krokach, o których strumień cokolwiek powiedział, więc twarz stała pusta.
 *
 * Pusty napis znaczy więc „ten agent nazywa się tak jak ten krok", nigdy „nie wiemy, kto to" —
 * i dlatego odpowiedzią jest nazwa kroku, a nie znak zapytania. To nie jest zgadywanie: barwa
 * tej twarzy powstaje z DOKŁADNIE tego samego napisu (`identityToken(card.id)`, a `card.id`
 * jest podpisem ze strumienia, czyli nazwą kroku), więc litery i kolor mówią o jednym napisie.
 * Litery wyliczone z czegokolwiek innego rozjechałyby się z barwą obok nich.
 */
function signedBy(step: GraphStep, who: Who): string {
  return who.name === '' ? step.name : who.name;
}

/**
 * Twarz agenta — reguła `.face` z makiety, w rozmiarze, w jakim stoi ona na karcie kroku
 * (`--fs:22px`): bok 22 px, miękki narożnik, podkład i obrys z barwy TOŻSAMOŚCI tego agenta,
 * a w środku jego inicjały.
 *
 * DLACZEGO NIE SAM KWADRACIK. Do 2026-08-31 stał tu prostokąt 11 px bez ani jednego znaku
 * w środku, więc jedynym nośnikiem „kto to robi" była BARWA — a pięć tokenów `--color-id-*`
 * to pięć przygaszonych odcieni różniących się o kilkanaście stopni. Osoba, która ich nie
 * rozdziela, nie dostawała z tego wiersza nic. To ten sam brak, na który ten katalog odpowiedział
 * już raz, dostawiając SŁOWO do chipa stanu (`./the-state-of-a-step-is-a-word.test.tsx`) —
 * i odpowiedź jest ta sama: znak, nie kolejny odcień.
 *
 * `aria-hidden`, bo inicjały są SKRÓTEM napisu, który na tej karcie i tak stoi w całości —
 * raz jako nazwa wykonawcy obok, a kiedy tej nie ma, jako nazwa kroku w nagłówku. Czytnik ekranu
 * przeczytałby „Re Reproduce". Barwa idzie przez `color-mix` na tokenie, nigdy literałem
 * [DESIGN §9].
 */
function Face({ signature, who }: { signature: string; who: Who }): ReactElement {
  const colour = `var(${who.square})`;
  return (
    <span
      aria-hidden
      className="grid size-[22px] shrink-0 place-items-center rounded-sm font-mono text-meta font-bold"
      style={{
        color: colour,
        background: `color-mix(in srgb, ${colour} 16%, transparent)`,
        border: `1px solid color-mix(in srgb, ${colour} 38%, transparent)`,
      }}
    >
      {initialsOf(signature)}
    </span>
  );
}

/**
 * Jedna linia tekstu kafelka.
 *
 * `data-card-line` niosą wszystkie cztery i tylko one — po tym atrybucie liczy się sufit
 * z ARCHITECTURE §7. Linia bez wartości nie istnieje: pusty slot dalej zajmuje wysokość
 * i dalej wygląda jak fakt, którego nie znamy, zamiast jak fakt, którego nie ma.
 */
function CardLine({
  text,
  className,
  full,
}: {
  text: string;
  className: string;
  /** Pełna wartość dla podpowiedzi, kiedy linia jest OBCINANA. Bez niej długa nazwa kroku
   *  kończy się wielokropkiem i nie ma jak jej doczytać (2026-08-31: kroki nazwane „Phase 07
   *  with a deliberately long descriptive name" na kafelku 246 px). */
  full?: string;
}): ReactElement | null {
  if (text === '') return null;
  return (
    <span data-card-line className={className} {...(full === undefined ? {} : { title: full })}>
      {text}
    </span>
  );
}

export function RunTile({ step, plan, onOpen, style }: RunTileProps): ReactElement {
  const measure = measureOf(step, plan);
  /* `data-step` PRZED klasami, i to nie jest kosmetyka: kryterium tnie markup listy na
     kafelki po tym atrybucie, a atrybut zapisany po `class` zostawia w cudzym wycinku
     komplet klas NASTĘPNEGO kafelka — czyli punkt o formie stanu sądzi wtedy sąsiada. */
  const skin = `${CARD} ${TONE[step.status]}`;
  const body = (
    <>
      <div className="flex min-w-0 items-center gap-2">
        <CardLine
          text={step.name}
          full={step.name}
          className="min-w-0 flex-1 truncate text-heading text-ink"
        />
        <StateChip
          status={step.status}
          carriedOn={step.status === 'failed' && step.carriedOn === true}
        />
      </div>

      {/* KTO GO ROBI. Twarz jest TOŻSAMOŚCIĄ i nigdy stanem — ten sam kolor, który ten agent
          ma w liście obok (DESIGN §3, „Tożsamość ≠ stan"). Nazwę tokenu podaje wołający, bo
          przydziela ją `../rail/colour.ts`; drugie odwzorowanie tutaj mogłoby rozstrzygnąć
          inaczej i pokazać przy kroku innego agenta niż lista.

          Kształt wiersza jest z makiety, reguła `.sbox .who`: odstęp 8 px, krój do CZYTANIA,
          barwa `--body`. Krój maszynowy w stopniu etykiety, który stał tu do 2026-08-31, jest
          w tej aplikacji krojem WARTOŚCI WYLICZONEJ (`.value`, ostatnia linia tej samej karty)
          — a imię nie jest pomiarem. */}
      {step.who === undefined ? null : (
        <span className="flex min-w-0 items-center gap-2">
          <Face signature={signedBy(step, step.who)} who={step.who} />
          <CardLine text={step.who.name} className="min-w-0 truncate text-note text-body" />
        </span>
      )}

      {/* CO ROBI TERAZ. Jedno zdanie, obcięte do jednej linii: kafelek ma cztery linie, więc
          zdanie, które łamie się na dwie, zjada tę czwartą i sufit przestaje być sufitem. */}
      <CardLine text={step.doing ?? ''} className="truncate text-body text-ink" />

      {/* MIARA. `.value` niesie krój maszynowy i `tabular-nums` — to są wartości wyliczone,
          nie zdania. Stopień `--text-label` z makiety (`.node .bot`, 11 px). */}
      <CardLine
        text={measure.handsOn ? `${measure.waits} · runs before ▸` : measure.waits}
        className="value truncate border-t border-line pt-[5px] text-label"
      />
    </>
  );

  if (onOpen === undefined) {
    return (
      <div data-step={step.id} className={skin} {...(style === undefined ? {} : { style })}>
        {body}
      </div>
    );
  }

  /* CAŁA POWIERZCHNIA JEST PRZYCISKIEM — kliknięcie w nazwę, w kwadrat i w zdanie robi to samo,
     bo wszystkie trzy odpowiadają na jedno pytanie („pokaż mi, kto to robi”). `text-left`, bo
     przycisk domyślnie centruje tekst, a kafelek czyta się od lewej. */
  return (
    <button
      type="button"
      data-step={step.id}
      onClick={onOpen}
      className={`${skin} text-left`}
      {...(style === undefined ? {} : { style })}
    >
      {body}
    </button>
  );
}
