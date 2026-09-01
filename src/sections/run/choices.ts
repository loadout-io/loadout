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
import type { Step as FileStep, WorkflowFile } from '../../state/workflows';

/** Pozycja listy: nazwa pliku, to, jak workflow nazywa sam siebie, i jego plan kroków. */
export interface Choice {
  /** Nazwa pliku w katalogu workflow. To ona jedzie do Rusta [T3 §8.3]. */
  readonly path: string;
  /** Jak workflow nazywa SAM SIEBIE — napis, który widzi człowiek. */
  readonly name: string;
  readonly steps: readonly RunStep[];
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
 * `instructions` dalej NIE jedzie i to jest osobny, zapisany brak (`./session/layout.ts`).
 */
export function planOf(steps: readonly FileStep[]): readonly RunStep[] {
  return steps.map((step) => ({
    id: step.id,
    name: step.name,
    state: 'pending' as const,
    kind: step.kind,
  }));
}

/** Pozycje listy z tego, co leży w katalogu workflow. */
export function toChoices(entries: readonly Listed[]): readonly Choice[] {
  return entries.map((entry) => ({
    path: entry.path,
    name: entry.workflow.name,
    steps: planOf(entry.workflow.steps),
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
