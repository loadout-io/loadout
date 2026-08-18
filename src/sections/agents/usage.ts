/* „used in 3 workflows" — POLICZONE, nie zmyślone.
 *
 * Makieta ma ten wiersz w każdym kafelku agenta (`docs/mockup/index.html`, sekcja `agents`,
 * `<div class="meta">`). Liczba w makiecie jest rysunkiem. Liczba w aplikacji ma być prawdą,
 * bo niezmiennik 17 zabrania UI rysować relacje, których nie ma w danych — a „3 workflows"
 * policzone z niczego jest dokładnie tym przykładem, który stoi w AGENTS.md przy tej regule.
 *
 * SKĄD DANE. Krok rodzaju `agent` niesie pole `agent` z identyfikatorem zapisanego agenta
 * (`src/state/workflows.ts`). Żeby uczciwie odpowiedzieć „w ilu workflow ten agent jest
 * używany", trzeba przeczytać pliki workflow i policzyć, ile z nich go nazywa. Czyta się to
 * jednym wywołaniem, bo `list_workflows` oddaje CAŁE pliki razem z krokami — więc ten wiersz
 * kosztuje jedno pytanie do dysku na wejście na ekran, nie jedno na agenta.
 *
 * DLACZEGO IMPORT Z `../workflows/io`, A NIE WŁASNA NAZWA KOMENDY. Nazwa `list_workflows`
 * mieszka w JEDNYM miejscu w repo i to jest adapter tamtej sekcji (niezmiennik 23). Druga
 * kopia tej nazwy tutaj byłaby drugim miejscem do poprawienia w dniu, w którym granica zmieni
 * kształt — a `invoke` na nieistniejącą komendę odmawia dopiero pod palcem użytkownika.
 *
 * DLACZEGO LICZYMY PLIKI, A NIE KROKI. Wiersz mówi `used in 3 workflows`, więc jednostką jest
 * PLIK. Workflow, który woła tego samego agenta w czterech krokach, jest jednym workflow —
 * inaczej człowiek czyta „used in 4 workflows" i szuka trzech, których nie ma.
 */
import { list } from '../workflows/io';
import type { WorkflowEntry } from '../workflows/list/store';

/**
 * Ile RÓŻNYCH plików workflow nazywa każdego agenta. Klucz to `Agent.id`.
 *
 * Agenta, którego nie nazywa nikt, w mapie NIE MA — i to jest różnica, która niesie treść:
 * `0` w mapie znaczyłoby „policzone i zero", a brak klucza po odczytaniu katalogu znaczy
 * dokładnie to samo. Ekran czyta przez `usedIn`, więc obie drogi dają zero.
 */
export function countUsage(workflows: readonly WorkflowEntry[]): Record<string, number> {
  const counted: Record<string, number> = {};

  for (const entry of workflows) {
    /* Zbiór na plik, nie licznik na krok: ten sam agent w czterech krokach jednego workflow
     * to jeden workflow. Patrz nagłówek pliku. */
    const named = new Set<string>();
    for (const step of entry.workflow.steps) {
      if (step.kind === 'agent' && step.agent !== '') named.add(step.agent);
    }
    for (const id of named) {
      counted[id] = (counted[id] ?? 0) + 1;
    }
  }

  return counted;
}

/** Ile workflow używa tego agenta. Brak klucza to zero, nigdy `undefined` na ekranie. */
export function usedIn(usage: Readonly<Record<string, number>>, id: string): number {
  return usage[id] ?? 0;
}

/**
 * `used in 3 workflows` — brzmienie z makiety, z liczbą pojedynczą, kiedy jest jeden.
 *
 * `used in 1 workflows` jest tym rodzajem drobiazgu, po którym człowiek przestaje wierzyć
 * reszcie liczb na ekranie, a kosztuje jedną gałąź.
 */
export function usageSays(count: number): string {
  return count === 1 ? 'used in 1 workflow' : `used in ${String(count)} workflows`;
}

/**
 * Przeczytaj katalog workflow i policz. Odmowa LECI DALEJ, bo ekran ma wtedy nie pokazać
 * wiersza wcale — liczba „0 workflows" wyświetlona, kiedy katalogu nie udało się przeczytać,
 * jest zdaniem nieprawdziwym, a niezmiennik 17 zabrania go bardziej niż milczenia.
 */
export async function readUsage(): Promise<Record<string, number>> {
  return countUsage(await list());
}
