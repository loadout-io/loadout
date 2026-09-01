/* Pasek loadoutu: workflow jako ciąg bloków, jeden na krok [DESIGN §2].
 *
 * Dwie rzeczy są tu wiążące i obie łamią się po cichu.
 *
 * Bloków jest DOKŁADNIE tyle, ile bieg ma kroków. Cztery na stałe, „bo makieta ma cztery",
 * to interfejs rysujący relację, której nie ma w danych (niezmiennik 17).
 *
 * Bloków `now` może być KILKA. Jeden kursor `currentIndex` przechodzi każdy test na biegu
 * sekwencyjnym i kłamie w pierwszym biegu równoległym — a równoległość jest całą przesłanką
 * tego produktu (niezmiennik 11). Stan bloku jest więc funkcją stanu kroku, nie pozycji.
 *
 * Mapowanie jest TOTALNE na siedmiu stanach [ARCHITECTURE §5] i żaden z trzech stanów
 * końcowych bez sukcesu (`failed`, `cancelled`, `skipped`) nie ma prawa dać `done`: blok
 * wypełniony to obietnica, że krok się udał, a pominięty krok pokazany jako zrobiony jest
 * kłamstwem, które użytkownik odkrywa dopiero po wyniku.
 */
import type { FeedLine, Step, StepState } from '../../../state/run';

/** Trzy stany bloku [DESIGN §2]: wypełniony, akcent, obrys. */
export type BlockState = 'done' | 'now' | 'todo';

export interface Block {
  readonly id: string;
  readonly name: string;
  readonly state: BlockState;
  /** Krok się skończył, ale nie sukcesem. Blok zostaje `todo` i mówi to osobno. */
  readonly ended: boolean;
  /**
   * Ten krok jest odpowiedzią na pytanie „co poszło źle" — w odróżnieniu od kroku, którego
   * człowiek zatrzymał, i od kroku pominiętego.
   *
   * 2026-08-23 — POWSTAŁO ZE SKARGI: „nie wiadomo które jak chodzą". Do dziś pięć z siedmiu
   * stanów kroku wyglądało na pasku IDENTYCZNIE — `pending`, `ready`, `failed`, `cancelled`
   * i `skipped` dawały ten sam pusty obrys, a jedyną różnicą była kreska przerywana na pasku
   * wysokim na 8 px.
   *
   * Osobne od `ended`, a nie zamiast niego, bo to są dwa różne fakty. `ended` mówi „ten krok
   * już się nie wydarzy" i dotyczy całej trójki. To pole mówi „i to jest miejsce, w którym coś
   * padło" — a zatrzymanie przez człowieka nie jest awarią (niezmiennik 7), więc kolor błędu
   * mu się nie należy.
   *
   * `engine::scheduler` trzyma dokładnie tę samą różnicę od dawna: maluje stożek osobno przez
   * `UpstreamFailed` i `UpstreamCancelled`, i mówi wprost dlaczego — „bez rozróżnienia na powód
   * wszystko poniżej anulowanego kroku meldowałoby `Skipped` i UI tłumaczyłoby świadomy Stop
   * jako cudzą awarię". Pasek zwijał tę różnicę z powrotem.
   */
  readonly wentWrong: boolean;
}

export interface Strip {
  /**
   * Kroki biegu, jeden do jednego z planem.
   *
   * 2026-08-31 — DŁUG ZGŁOSZONY WPROST: od dziś NIKT ich nie rysuje. Torek bloków zszedł
   * z paska (był drugim rysunkiem planu obok obrazu w kolumnie pracy, niezmiennik 13), a te
   * dane zostały, bo z nich liczy się PODPIS — i tylko on. Znaczy to, że `ended` i `wentWrong`
   * nie mają dziś ani jednego czytelnika poza własnym testem, czyli są dokładnie tą rzeczą,
   * której zabrania niezmiennik 21. Zdjęcie ich należy do zadania, które przepisze `stripFor`
   * na sam podpis; robione tutaj byłoby przepisaniem cudzego kryterium przy okazji.
   */
  readonly blocks: readonly Block[];
  /** `<nazwa> · step N of M` / `<nazwa> · N of M running` / `<nazwa> · M steps`. */
  readonly caption: string;
  /**
   * Chip z prawej: ile ten bieg zajął agentom i ile kosztował. Puste, dopóki nie wiadomo.
   *
   * TO JEST JEDYNE DOZWOLONE MIEJSCE NA TE DWIE LICZBY w całej aplikacji (niezmiennik 13) —
   * więc ich brak tutaj znaczył, że nie ma ich NIGDZIE, i tak było do 2026-08-18: makieta ma
   * chip `4m 12s · $0.31`, a pasek nie miał ani jednej cyfry.
   *
   * Puste, nie „—" i nie „0.0s · $0.00": bieg, po którym nie skończył się ani jeden krok,
   * nie ma jeszcze czego podać, a zero wygląda jak pomiar (`SPEND: not reported` z poprzedniego prototypu
   * stało w tej samej siatce, co wiersz z prawdziwą liczbą).
   */
  readonly spend: string;
}

/**
 * Stan bloku dla każdego z siedmiu stanów kroku [ARCHITECTURE §5].
 *
 * `Record`, nie `switch` z gałęzią `default`: mapowanie ma być TOTALNE, a gałąź domyślna
 * zamienia ósmy stan dodany kiedyś po stronie Rusta w cichy `todo` zamiast w błąd kompilacji.
 * Trzy stany końcowe bez sukcesu celowo lądują w `todo`, nie w `done` — patrz `ENDED`.
 */
const BLOCK: Readonly<Record<StepState, BlockState>> = {
  succeeded: 'done',
  running: 'now',
  pending: 'todo',
  ready: 'todo',
  failed: 'todo',
  cancelled: 'todo',
  skipped: 'todo',
};

/** Krok, który padł — jedyny z trójki `ENDED`, który odpowiada na „co poszło źle". */
const WENT_WRONG: ReadonlySet<StepState> = new Set<StepState>(['failed']);

/**
 * Kroki, które się skończyły, ale nie sukcesem.
 *
 * Blok wypełniony jest obietnicą, że krok się udał. Pominięty krok pokazany jako zrobiony jest
 * kłamstwem, które użytkownik odkrywa dopiero po wyniku całego biegu — a wtedy nie ma już czego
 * naprawić. Stąd osobna flaga zamiast czwartego stanu bloku: DESIGN §2 zna trzy i tyle ich jest.
 */
const ENDED: ReadonlySet<StepState> = new Set<StepState>(['failed', 'cancelled', 'skipped']);

/**
 * Zdanie z decyzji D7 — co do znaku, bo to jest napis, który czyta człowiek.
 *
 * „Co musi przetrwać nawet przy zerowej ceremonii": przy workflow bez sprawdzeń UI mówi to
 * wprost i nie pokazuje zieleni. Brak ceremonii ma znaczyć „nikt tego nie sprawdził", nigdy
 * „sprawdzone i dobrze" — i to jest ta sama linia, na której stoi cały produkt: co agent
 * powiedział kontra co się stało [00-SYNTHESIS §2.1].
 *
 * Kropka rozdzielająca należy do stałej, bo zdanie zawsze dokleja się do podpisu, który już
 * stoi — nowy rząd chrome nie wchodzi w grę: `docs/ARCHITECTURE.md` §7 daje 96 px nad pierwszą
 * treścią, a ekran pracy wydaje 90 na karty i ten pasek.
 */
const NO_CHECKS = ' · no checks configured';

/**
 * Czy o tym planie WIADOMO, że nikt w nim niczego nie sprawdza.
 *
 * TRZY STANY ŚWIATA, DWIE ODPOWIEDZI, i granica między nimi jest tu wszystkim. „W tym planie nie
 * ma sprawdzeń" jest zdaniem o biegu; „nie wiemy, z czego ten plan się składa" nie jest zdaniem
 * o nim wcale. Krok bez rodzaju daje więc `false`, czyli ciszę — a taki krok istnieje: plan
 * jednego kroku, który okno składa samo dla `/ask` (`../io.ts`), niesie sam identyfikator, nazwę
 * i stan. Napis „no checks configured" postawiony nad nim mówiłby o tym biegu rzecz, której
 * z danych nie widać (niezmiennik 17).
 *
 * PUNKT KONTROLNY NIE JEST SPRAWDZENIEM, i ta funkcja mówi to przez to, czego w niej nie ma:
 * kafelek „zapytaj mnie" zatrzymuje bieg i pyta człowieka, więc sam nie mierzy niczego. Sprawdza
 * wyłącznie kafelek `check` — ten, który liczy wynik z kodu wyjścia I dowodu w wyjściu
 * [D6, „Trzeci rodzaj: sprawdź"], czyli z tego, co się stało, a nie z tego, co ktoś powiedział.
 */
function nothingChecksThisPlan(plan: readonly Step[]): boolean {
  if (plan.length === 0) return false;
  return plan.every((step) => step.kind !== undefined && step.kind !== 'check');
}

/**
 * Podpis paska.
 *
 * Trzy zdania, bo bieg równoległy jest zwykłym biegiem, nie wyjątkiem (niezmiennik 11):
 * „krok 2 z 4" ma sens dokładnie wtedy, kiedy biegnie jeden krok, a przy dwóch jest już
 * wyborem, który z nich nazwać ważniejszym. Bez ani jednego biegnącego kroku nie ma numeru,
 * na który można wskazać, więc podpis mówi tylko, ile kroków ma workflow.
 *
 * 2026-08-28 — ZDANIE Z D7 DOKLEJA SIĘ DO WSZYSTKICH TRZECH, nie tylko do tego ostatniego.
 * „Ten bieg nikogo nie prosi o pomiar" jest faktem o PLANIE, więc nie ma prawa zależeć od tego,
 * czy akurat coś biegnie: wersja dopisująca je wyłącznie przy postoju gasi zdanie w sekundzie,
 * w której człowiek naciska Start, i zapala z powrotem po ostatnim kroku. Fakt z migającym
 * nośnikiem czyta się jak awaria ekranu, a nie jak własność workflow — a przez cały bieg
 * ekran wracałby do milczenia, czyli do tego jednego stanu, którego D7 zabrania.
 *
 * Podpis jest JEDYNYM miejscem tego zdania (niezmiennik 13). Rodzaje kroków dojeżdżają tu
 * `planOf` → `src/state/run.ts` → `stripFor`, i ta droga jest cała: bez niej pasek widziałby
 * kafelek „sprawdź" i kafelek agenta jako to samo.
 */
function captionFor(workflow: string, blocks: readonly Block[], plan: readonly Step[]): string {
  const phrase = stepPhrase(blocks, plan);
  /* Bieg, którego nie ma, nie ma czego podpisywać. „· 0 steps" opisywałoby workflow o zerowej
   * długości, czyli rzecz, której nie da się zbudować. */
  if (phrase === '') return '';
  return `${workflow} · ${phrase}`;
}

/**
 * Sam ciąg o krokach, BEZ nazwy workflow — „step 3 of 4", „2 of 4 running", „4 steps",
 * z doklejonym wyznaniem D7, kiedy w planie nikt niczego nie mierzy.
 *
 * WYJĘTE Z `captionFor` 2026-08-31, i to jest wyjęcie, nie druga kopia. Nazwa biegu stoi od dziś
 * w TYTULE nagłówka (`./head.tsx`, `.rhead h1` z makiety), więc podpis, który dokleja ją jeszcze
 * raz, byłby drugim domem jednego faktu (niezmiennik 13). Ta funkcja jest jedynym miejscem, które
 * rozstrzyga, jak policzyć kroki — `captionFor` składa z niej to samo zdanie co dotąd, co do
 * znaku, a nagłówek bierze ją samą.
 */
export function stepPhrase(blocks: readonly Block[], plan: readonly Step[]): string {
  const total = blocks.length;
  if (total === 0) return '';

  const admission = nothingChecksThisPlan(plan) ? NO_CHECKS : '';
  const running = blocks.filter((block) => block.state === 'now').length;
  if (running === 1) {
    /* Numer kroku jest jego pozycją w grafie, nie liczbą tych, które się skończyły: przy
     * biegu, który przeskoczył krok, „step 2 of 4" i „drugi blok" muszą być tym samym blokiem. */
    const at = blocks.findIndex((block) => block.state === 'now') + 1;
    return `step ${at} of ${total}${admission}`;
  }
  if (running > 1) {
    return `${running} of ${total} running${admission}`;
  }
  return `${total} steps${admission}`;
}

/** Sekundy w milisekundzie — jedyne miejsce, w którym ta zamiana tu żyje. */
const MS_PER_SECOND = 1_000;
/** Sekund w minucie. */
const SECONDS_PER_MINUTE = 60;

/**
 * Czas tak, jak zapisuje go strona Rusta (`engine/line.rs`, `took_text`): `6.2s` pod minutą,
 * `4m 12s` powyżej.
 *
 * Przepisany kształt, nie przepisana liczba: te dwa napisy muszą się czytać identycznie, bo
 * ten sam bieg widać i w linii `done` w strumieniu, i w chipie na pasku. Dwie różne konwencje
 * na jeden pomiar to dwa różne odczyty tej samej rzeczy na jednym ekranie.
 */
function tookText(ms: number): string {
  if (ms < SECONDS_PER_MINUTE * MS_PER_SECOND) {
    const tenths = Math.round(ms / 100) / 10;
    return tenths.toFixed(1) + 's';
  }
  const seconds = Math.floor(ms / MS_PER_SECOND);
  return (
    String(Math.floor(seconds / SECONDS_PER_MINUTE)) +
    'm ' +
    String(seconds % SECONDS_PER_MINUTE) +
    's'
  );
}

/**
 * Chip paska: ile czasu zebrało się na agentach i ile to kosztowało — z tego, co PRZYSZŁO.
 *
 * SKĄD TE LICZBY. Wiersz `done` zamyka turę agenta i niesie `durationMs` oraz `costUsd`
 * (`engine/line.rs`, `done_line`) — surowo, nie zaokrąglone do wyświetlenia, właśnie po to,
 * żeby suma biegu dała się policzyć. Sumujemy więc to, co dostaliśmy, i ani jednej liczby
 * więcej: bieg bez ani jednej skończonej tury daje pusty napis, a nie zero.
 *
 * CZEGO TA FUNKCJA NIE UDAJE. Suma czasów tur NIE JEST czasem zegarowym biegu — przy dwóch
 * agentach pracujących równolegle jest większa. Zegara ściennego biegu okno dziś nie ma
 * (`run_workflow` oddaje `()`, a `RunReport` nie jest `Serialize`), więc chip mówi to, co
 * naprawdę wiemy, a podpowiedź nad nim nazywa tę wielkość słowami. Wpisanie tu „elapsed"
 * byłoby liczbą, która wygląda na zegar i nim nie jest.
 *
 * Koszt pomijany, kiedy ŻADNA tura go nie podała: `costUsd` jest `Option<f64>` i dostawca bez
 * cenniku (albo tryb, w którym go nie ma) oddaje `null`. `$0.00` przy biegu, który kosztował
 * nieznane pieniądze, jest gorsze niż brak liczby.
 */
export function spendFor(lines: readonly FeedLine[], budgetUsd: number | null = null): string {
  let ms = 0;
  let cost = 0;
  let turns = 0;
  let priced = false;
  for (const line of lines) {
    if (line.kind !== 'done') continue;
    turns += 1;
    ms += line.durationMs;
    if (line.costUsd !== null) {
      cost += line.costUsd;
      priced = true;
    }
  }
  if (turns === 0) return '';
  if (!priced) return tookText(ms);
  return tookText(ms) + ' · $' + cost.toFixed(2) + outOf(budgetUsd);
}

/**
 * ` of $20` — druga połowa chipu, kiedy ten bieg ma sufit wydatku.
 *
 * Pusto, kiedy sufitu nie ma, i to jest cała reguła: liczba „z ilu" wpisana biegowi, którego
 * nikt nie ograniczył, byłaby limitem wymyślonym przez ekran.
 *
 * Sufit bez groszy, kiedy jest okrągły: człowiek wpisał `20`, więc `$20` jest tym, co postawił.
 * `$20.00` obok `$3.41` czyta się jak druga wartość zmierzona, a to jest jego decyzja.
 */
function outOf(budgetUsd: number | null): string {
  if (budgetUsd === null) return '';
  const ceiling = Number.isInteger(budgetUsd) ? String(budgetUsd) : budgetUsd.toFixed(2);
  return ' of $' + ceiling;
}

/**
 * Pasek dla tego workflow i tych kroków, w kolejności grafu.
 *
 * `spend` wchodzi gotowe, a nie liczy się tutaj z linii: pasek jest funkcją PLANU, a wydatek
 * jest funkcją STRUMIENIA, i wołający ma pod ręką jedno i drugie. Trzeci argument jest
 * opcjonalny, bo cudze kryterium (`strip/strip.test.ts`) woła tę funkcję dwoma i nie wolno
 * go tknąć — a bieg bez ani jednej skończonej tury i tak nie ma czego pokazać.
 */
export function stripFor(workflow: string, steps: readonly Step[], spend = ''): Strip {
  /* Jeden blok na jeden krok, w kolejności grafu — długość bierze się z danych. Cztery bloki
   * „bo makieta ma cztery" to interfejs rysujący relację, której nie ma (niezmiennik 17). */
  const blocks: Block[] = steps.map((step) => ({
    id: step.id,
    name: step.name,
    /* Stan bloku jest funkcją stanu KROKU, nigdy jego pozycji. Jeden kursor `currentIndex`
     * przechodzi każdy bieg sekwencyjny i kłamie w pierwszym równoległym. */
    state: BLOCK[step.state],
    ended: ENDED.has(step.state),
    wentWrong: WENT_WRONG.has(step.state),
  }));

  /* Podpis dostaje KROKI, nie same bloki: rodzaj kafelka jest faktem o planie, a blok mówi
   * wyłącznie o tym, jak się rysuje. Przewiezienie `kind` przez `Block` dołożyłoby polu tylko
   * jednego czytelnika — ten podpis — a `strip.tsx` niósłby wtedy w propsach wartość, której
   * nie rysuje. Bloki i kroki są tu jeden do jednego, więc podpis nic na tym nie traci. */
  return { blocks, caption: captionFor(workflow, blocks, steps), spend };
}
