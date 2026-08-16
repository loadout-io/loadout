/* „Tidy up" — jeden przycisk, który układa kafelki z góry na dół.
 *
 * Dlaczego to nie jest kosmetyka: układ zwraca ZMIENNOPRZECINKOWE środki węzłów, więc płótno,
 * które przycina pozycje tylko w handlerze przeciągania, po każdym „Tidy up" zapisuje plik
 * z innym dziesiątym miejscem po przecinku — diff bez treści, przy każdym kliknięciu [T3 §8.2].
 * Dlatego wynik przechodzi przez tę samą siatkę co przeciąganie, a kryterium sprawdza to pętlą
 * po WSZYSTKICH krokach, nie na jednej pozycji.
 */
import type { Step, WorkflowFile } from '../../../state/workflows';
import { GRID } from '../../../state/workflows';
import { snap } from './map';

/* Odstępy w skokach siatki, nie w pikselach z oka: `GRID` jest tu mnożnikiem, więc każda
 * pozycja wychodzi z tej arytmetyki już przyciągnięta, a nie przycięta po fakcie.
 *
 * Kolumna to szerokość kafelka (280 px, DESIGN §6) zaokrąglona w górę do siatki plus jeden
 * skok przerwy; wiersz to ta sama odległość, którą mają kafelki w makiecie (linie 505-521). */
const ROW = 6 * GRID;
const COLUMN = 13 * GRID;
const MARGIN = GRID;

/** Ile kroków stoi przed tym krokiem w najdłuższej ścieżce do niego.
 *
 * To jest ta sama liczba, którą dagre nazywa `rank` przy `rankdir: 'TB'`, policzona wprost:
 * krok bez wejść ma 0, każdy inny ma o jeden więcej niż jego najgłębszy poprzednik. Własność,
 * dla której to tu stoi: dla KAŻDEJ strzałki `rank(to) > rank(from)`, więc następnik zawsze
 * ląduje niżej, a nie „zwykle niżej".
 *
 * `busy` łamie koło, którego w pliku nie powinno być, ale może być: `isValidConnection` broni
 * płótna, a plik bywa poprawiony ręcznie albo zmergowany gitem. Koło zgłasza walidator (T-12);
 * zadaniem tej funkcji jest ułożyć kafelki, a nie zawiesić się na cudzym błędzie. */
function depths(file: WorkflowFile): Map<string, number> {
  const before = new Map<string, string[]>();
  for (const link of file.links) {
    before.set(link.to, [...(before.get(link.to) ?? []), link.from]);
  }

  const depth = new Map<string, number>();
  const busy = new Set<string>();

  const of = (id: string): number => {
    const known = depth.get(id);
    if (known !== undefined) return known;
    if (busy.has(id)) return 0;

    busy.add(id);
    let deepest = 0;
    for (const from of before.get(id) ?? []) deepest = Math.max(deepest, of(from) + 1);
    busy.delete(id);

    depth.set(id, deepest);
    return deepest;
  };

  for (const step of file.steps) of(step.id);
  return depth;
}

/** Ten sam krok, postawiony gdzie indziej — patrz `movedTo` w `map.ts`, ten sam powód. */
function movedTo<S extends Step>(step: S, x: number, y: number): S {
  /* Przez `snap`, choć arytmetyka wyżej i tak trafia w siatkę. To nie jest asekuracja, tylko
   * jedyna droga zapisu pozycji: gdyby kiedyś `ROW` przestał być wielokrotnością skoku, plik
   * ma dostać liczbę z siatki, a nie tę, którą wyliczył układ [T3 §8.2 reguła 1]. */
  return { ...step, at: snap({ x, y }) };
}

/** Układa kroki z góry na dół: następnik stoi zawsze niżej niż jego poprzednik, a każda
 * pozycja jest całkowitą wielokrotnością `GRID`. */
export function tidyUp(file: WorkflowFile): WorkflowFile {
  const depth = depths(file);
  /* Ile kafelków stoi już w tym wierszu. Kolejność w wierszu jest kolejnością WSTAWIANIA
   * (`steps`), więc dwa „Tidy up" pod rząd dają ten sam plik. */
  const taken = new Map<number, number>();

  return {
    ...file,
    steps: file.steps.map((step) => {
      const row = depth.get(step.id) ?? 0;
      const column = taken.get(row) ?? 0;
      taken.set(row, column + 1);
      return movedTo(step, MARGIN + column * COLUMN, MARGIN + row * ROW);
    }),
  };
}
