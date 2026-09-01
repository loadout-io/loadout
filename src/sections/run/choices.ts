/* Co da się uruchomić — lista wyboru ekranu pracy i, co ważniejsze, KTÓRA pozycja jest domyślna.
 *
 * DLACZEGO OSOBNY PLIK, ZMIERZONE 2026-08-18. Wybór domyślny stał w `start.tsx` jako
 * `picked === '' ? (choices[0]?.path ?? '') : picked`, a lista przychodzi z `workflows.rs:122`
 * posortowana BAJTOWO. `new-workflow-2.json` (znak `-`, 0x2D) wypada przed `new-workflow.json`
 * (znak `.`, 0x2E) — i to pierwsze ma `"steps": []`. Skutek dla człowieka: klikasz Run na
 * workflow z dwoma krokami, na ekranie Run stoi „New workflow 2", naciskasz Start i czytasz
 * „There are no steps yet." o czymś, co przed chwilą miało dwa kroki. Ironia jest zapisana
 * w `docs/STATUS.md:19`, który używa właśnie tego pliku jako dowodu, że to nie atrapa.
 *
 * Polityka „co jest domyślne" mieszka więc w JEDNYM miejscu i jest funkcją czystą, dającą się
 * osądzić bez okna (niezmiennik 15) — bo tego defektu nie da się zobaczyć w renderze: dwie
 * implementacje wyglądają identycznie, dopóki nie zajrzysz, CO poleciało do Rusta.
 */
import type { Step as RunStep } from '../../state/run';
import type { Link, Step as FileStep, WorkflowFile } from '../../state/workflows';

/** Pozycja listy: nazwa pliku, to, jak workflow nazywa sam siebie, i jego plan kroków. */
export interface Choice {
  /** Nazwa pliku w katalogu workflow. To ona jedzie do Rusta [T3 §8.3]. */
  readonly path: string;
  /** Jak workflow nazywa SAM SIEBIE — napis, który widzi człowiek. */
  readonly name: string;
  readonly steps: readonly RunStep[];
  /**
   * Strzałki „po" z tego pliku — brak pola znaczy „nie wiemy".
   *
   * 2026-08-31 — POZYCJA I STRZAŁKA SĄ JEDNYM FAKTEM O KSZTAŁCIE, ale mieszkają w pliku
   * osobno: pozycja przy kafelku, strzałka w `links`. Kroki jadą tędy od początku, strzałki
   * nie jechały wcale — więc widok biegu znał listę kroków i nie znał ani jednej relacji
   * między nimi. Rysunek zbudowany na takim stanie rysowałby kolejność, której nikt nie
   * zapisał (niezmiennik 17).
   *
   * Opcjonalne, bo cudze kryteria stawiają pozycję listy z trzech pól (`run-command.test.ts`)
   * i mają dalej się kompilować. `toChoices` wypełnia je ZAWSZE — także pustą listą, bo „ten
   * plik nie ma ani jednej strzałki" jest odpowiedzią, a nie brakiem odpowiedzi.
   */
  readonly links?: readonly Link[];
}

/** Tyle o pliku, ile potrzebuje lista wyboru. Węższe niż `WorkflowEntry`, bo tyle wystarcza. */
export interface Listed {
  readonly path: string;
  readonly workflow: WorkflowFile;
}

/**
 * Plan biegu z pliku workflow: kafelki grafu w kolejności wstawiania, wszystkie jeszcze czekają.
 *
 * `pending` dla każdego, bo w chwili kliknięcia Start żaden krok nie ruszył. Blok `todo` jest
 * obrysem, nie obietnicą — to blok wypełniony obiecuje, że krok się udał [DESIGN §2], więc plan
 * pokazany od pierwszej sekundy nie mówi nic nieprawdziwego o tym, co się już wydarzyło.
 * Dalsze stany dowozi rodzaj `stepState` z drutu, przez `src/state/run.ts`.
 *
 * 2026-08-28 — RODZAJ KAFELKA JEDZIE RAZEM Z NIMI, i to jest jedyna krawędź, którą ten rodzaj
 * ma do widoku biegu. Bez tej jednej linii zdanie z decyzji D7 („no checks configured") nie ma
 * z czego powstać: pasek loadoutu widzi wyłącznie to, co przepisze ta funkcja, więc kafelek
 * „sprawdź" i kafelek agenta były dla niego tym samym. Przepisujemy `kind` surowo — pytanie
 * „czy w tym planie ktokolwiek cokolwiek sprawdza" należy do paska, a nie do tej funkcji,
 * bo to on ma na to jedno zdanie w podpisie (`./strip/model.ts`, niezmiennik 13).
 *
 * 2026-08-31 — POZYCJA KAFELKA JEDZIE RAZEM Z NIMI, z tego samego powodu, co rodzaj. Widok
 * biegu ma prawo narysować graf wyłącznie wtedy, gdy współrzędne PRZYJECHAŁY (niezmiennik 17);
 * bez tej linii jedynym sposobem na rysunek byłoby wymyślenie ich w komponencie, czyli ozdobna
 * krzywa między punktami, których nikt nie zapisał. Przepisujemy `at` surowo — pytanie „jak to
 * ułożyć na ekranie" należy do widoku, nie do tej funkcji.
 *
 * `instructions` dalej NIE jedzie i to jest osobny, zapisany brak (`./session/layout.ts`).
 */
export function planOf(steps: readonly FileStep[]): readonly RunStep[] {
  return steps.map((step) => ({
    id: step.id,
    name: step.name,
    state: 'pending' as const,
    kind: step.kind,
    at: step.at,
  }));
}

/** Pozycje listy z tego, co leży w katalogu workflow. */
export function toChoices(entries: readonly Listed[]): readonly Choice[] {
  return entries.map((entry) => ({
    path: entry.path,
    name: entry.workflow.name,
    steps: planOf(entry.workflow.steps),
    /* Surowo, bez przepisywania kluczy: `Link` po stronie Rusta nie ma `rename_all`, więc
     * `max_turns` jedzie dosłownie tak, jak stoi w pliku (`state/workflows.ts`). */
    links: entry.workflow.links,
  }));
}

/**
 * Pozycja o tej nazwie pliku, albo `null`.
 *
 * `null` znaczy „katalog zmienił się między odczytem a kliknięciem" i jest odpowiedzią, nie
 * awarią: plik jest prawdą (niezmiennik 4), a lista w pamięci jest jego widokiem sprzed chwili.
 */
export function choiceFor(choices: readonly Choice[], path: string): Choice | null {
  return choices.find((choice) => choice.path === path) ?? null;
}

/**
 * Co ma być wybrane, kiedy człowiek jeszcze nie wybierał — albo `null`, kiedy nic.
 *
 * PIERWSZY WORKFLOW, KTÓRY MA KROKI, i to jest cała treść tej funkcji. Nie `choices[0]`:
 * kolejność listy jest kolejnością bajtów nazw plików, czyli faktem o systemie plików, a nie
 * o pracy człowieka — a pierwszy bajtowo bywa świeżo utworzonym szkicem bez ani jednego kroku.
 * Bieg takiego pliku odmawia po stronie Rusta („There are no steps yet."), więc domyślny wybór,
 * który go wskazuje, jest domyślnym wyborem gwarantującym odmowę.
 *
 * `null`, kiedy żaden nie ma kroków: wtedy NIE MA domyślnego i kontrolka startu ma to powiedzieć
 * wprost, zamiast wskazywać cokolwiek. Wybór wskazujący na plik, którego nie da się uruchomić,
 * jest tą wersją, która wygląda na gotową.
 */
export function firstRunnable(choices: readonly Choice[]): Choice | null {
  return choices.find((choice) => choice.steps.length > 0) ?? null;
}

/**
 * Nazwa kontrolki wyboru workflow.
 *
 * EKSPORTOWANA, żeby kryterium mogło ją CZYTAĆ, a nie przepisywać — ten sam powód, dla którego
 * `LEAD_LABEL` mieszka w `../run/lead.ts`, a `TASK_LABEL` w `./start.tsx`. Napis przepisany do
 * testu przestaje pilnować czegokolwiek w dniu, w którym ktoś zmieni brzmienie na ekranie
 * i nie tknie kryterium.
 *
 * NIE JEST W `<label>`. Tekst `<label>` staje się NAZWĄ DOSTĘPNĄ kontrolki, więc zdanie
 * obok wyboru wpisane w `<label>` przemianowałoby go na siebie. Zdanie stoi w zwykłym napisie,
 * a nazwa jedzie przez `aria-label`.
 */
export const WORKFLOW_LABEL = 'Which workflow Run starts';

/**
 * KTÓRY WORKFLOW RUSZY — jedyna odpowiedź na to pytanie w całej aplikacji.
 *
 * `picked` to nazwa pliku, którą WSKAZAŁ CZŁOWIEK, albo `null`, kiedy nikt jeszcze nie wskazywał.
 * Wskazanie bije politykę: dopóki ten plik leży w katalogu, to on rusza, także wtedy, gdy nie ma
 * ani jednego kroku — bo odmowa Rusta na plik, który człowiek wybrał świadomie, jest odpowiedzią
 * na jego decyzję, a podmiana na inny plik byłaby uruchomieniem czegoś, o co nie prosił.
 *
 * `null` po drugiej stronie znaczy „katalog zmienił się między odczytem a kliknięciem" i wtedy
 * wraca [`firstRunnable`] — plik jest prawdą (niezmiennik 4), a wskazanie na nieistniejący plik
 * nie jest wskazaniem.
 *
 * DLACZEGO TO JEST FUNKCJA, A NIE POLE W MAGAZYNIE. Trzy miejsca na ekranie mówią o tej samej
 * rzeczy — nagłówek biegu, obraz planu i przycisk startu — i do 2026-08-31 dwa z nich pytały
 * o nią DWA RAZY, każde z własnego odczytu katalogu. Zmierzone w prawdziwym chromium: po
 * powrocie na sekcję Run wychodzą dwa niezależne `list_workflows`, a w scenie, w której granica
 * odpowiada na każde z nich inną listą, ekran po pełnym ustaniu pokazuje w nagłówku jeden plik,
 * a na przycisku drugi. Jedna funkcja nad jednym nośnikiem nie ma jak się rozjechać
 * (niezmiennik 13).
 */
export function willRun(choices: readonly Choice[], picked: string | null): Choice | null {
  const wanted = picked === null ? null : choiceFor(choices, picked);
  return wanted ?? firstRunnable(choices);
}

/**
 * Jak ta pozycja nazywa się na liście wyboru.
 *
 * Sama nazwa, a przy pliku BEZ KROKÓW także powód, dla którego nie da się go stąd puścić. Rust
 * odmawia mu zdaniem „There are no steps yet.", więc pozycja wyglądająca dokładnie jak pozostałe
 * jest zaproszeniem do odmowy (niezmiennik 16). Plik zostaje na liście, bo leży w katalogu:
 * lista, która go po cichu pomija, jest listą, na której człowiek go nie znajdzie.
 */
export function offerFor(choice: Choice): string {
  return choice.steps.length === 0 ? choice.name + ' — no steps yet' : choice.name;
}

/**
 * KTO WYBRAŁ TEN WORKFLOW — jedno zdanie pod nagłówkiem biegu.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-08-31: „czemu mi się ten deep reaserch pojawia, przecież nie
 * wybrałem żadnego workflow". Ekran ogłaszał „READY TO RUN" nad plikiem wybranym przez
 * [`firstRunnable`] i nie mówił ani słowa o tym, że to był wybór — a decyzja podjęta za
 * człowieka, bez śladu, że była decyzją, czyta się jak jedyna możliwość.
 *
 * TRZY ZDANIA, BO TO SĄ TRZY RÓŻNE STANY ŚWIATA i żaden nie brzmi jak pozostałe: człowiek
 * wskazał sam; nie wskazywał, a było z czego wybierać; nie wskazywał, bo nie było z czego.
 * Jedno zdanie na trzy stany kłamałoby w dwóch z nich.
 *
 * Zdanie stoi zawsze, także po wyborze — region, który znika po pierwszej zmianie, przesuwa
 * pod sobą nagłówek i obraz planu.
 */
export function whoChoseIt(choices: readonly Choice[], picked: string | null): string {
  if (picked !== null && choiceFor(choices, picked) !== null) return 'You picked this one.';
  const runnable = choices.filter((choice) => choice.steps.length > 0).length;
  if (runnable > 1) return 'Loadout picked this one for you — change it here.';
  return 'The only workflow with steps in this folder.';
}
