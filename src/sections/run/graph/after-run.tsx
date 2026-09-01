/* CO SIĘ STANIE, KIEDY OSTATNI KROK ZZIELENIEJE — powiedziane, ZANIM się stanie.
 *
 * PO CO TO ISTNIEJE. Człowiek patrzy na ekran, na którym czterech agentów zmienia jego kod, i ma
 * jedno pytanie, na które do 2026-08-31 nie było na tym ekranie ani jednej odpowiedzi: co się
 * stanie, kiedy oni skończą. Jedyną drogą do odpowiedzi było doczekać do końca. To jest ta sama
 * nieoczywistość, którą właściciel nazwał wprost („UX totalnie nieoczywisty"), i ta sama, którą
 * makieta rozwiązuje kartą pod ścieżką kroków (`docs/mockup/index.html`, wariant `droga`).
 *
 * KAŻDE ZDANIE JEST PRAWDZIWE I KAŻDE MA ŹRÓDŁO.
 *   nazwa ostatniego kroku  z PLANU, przez strzałki: ostatni jest ten, po którym nic nie idzie.
 *                           Nie „ostatni na liście" — bieg równoległy jest zwykłym biegiem
 *                           (niezmiennik 11), a kolejność wpisów w pliku nie jest kolejnością
 *                           wykonania.
 *   „the run stops"         `engine::scheduler` kończy bieg na ostatnim kroku i nie ma etapu
 *                           zaszytego po nim (niezmiennik 27).
 *   „on its own branch,     `commands::isolate::finish` → `Kept::OnABranch`: praca każdego kroku
 *    nothing is pushed"     ląduje commitem na gałęzi `loadout/<bieg>/<krok>`, a w całym module
 *                           nie ma ani jednego `git push`.
 *
 * CZEGO TU NIE MA I DLACZEGO. Makieta pisze w tym miejscu „Loadout opens a pull request". Tego
 * Loadout nie robi — w `src-tauri/` nie ma ani jednego wołania, które otwiera cokolwiek zdalnie.
 * Zdanie obiecujące czynność, której produkt nie wykonuje, jest gorsze niż brak zdania: człowiek
 * odkrywa różnicę dopiero wtedy, gdy jej szuka. Rozbieżność z makietą jest zgłoszona.
 *
 * KIEDY KROKÓW KOŃCOWYCH JEST KILKA, karta nie wybiera jednego. Wskazanie „ważniejszej" gałęzi
 * byłoby relacją, której w danych nie ma (niezmiennik 17) — zdanie mówi wtedy o ostatnim kroku
 * bez nazywania go.
 */
import type { ReactElement } from 'react';
import type { Plan } from './model';

/**
 * Kroki, po których nic już nie idzie — czyli te, na których ten bieg się kończy.
 *
 * Ze STRZAŁEK, nie z kolejności listy. Plan bez ani jednej strzałki nie niesie żadnej relacji
 * między krokami, więc każdy jego krok jest końcowy — i wtedy ta funkcja oddaje ich wszystkie,
 * a karta nie nazywa żadnego. To jest prawda o takim planie, nie jego zubożenie.
 */
function endsOn(plan: Plan): readonly string[] {
  return plan.steps
    .filter((step) => !plan.links.some((link) => link.from === step.id))
    .map((step) => step.name);
}

export interface AfterRunProps {
  plan: Plan;
}

export function AfterRun({ plan }: AfterRunProps): ReactElement | null {
  /* PLAN, KTÓREGO KROKÓW NIE ZNAMY, NIE MA KOŃCA, O KTÓRYM DA SIĘ COŚ POWIEDZIEĆ. Zdanie
   * o zakończeniu biegu, którego nikt nie zaczął ani nie zaplanował, jest obietnicą o niczym. */
  if (plan.steps.length === 0) return null;

  const ends = endsOn(plan);
  const last = ends.length === 1 ? ends[0] : null;

  return (
    <div
      data-after-run
      className="m-2 grid shrink-0 gap-[3px] rounded-md border border-accent-edge border-dashed bg-accent-soft px-[13px] py-3"
    >
      <b className="text-subhead text-ink">
        {last === null ? 'When the last step turns green' : `When ${last} turns green`}
      </b>
      {/* `text-body` jest tu BARWĄ, nie stopniem: w tym motywie `--color-body` i `--text-body`
          noszą tę samą nazwę, a Tailwind rozstrzyga taką kolizję na barwę (ten sam zapis
          i ten sam powód stoją w `sections/memory/note-row.tsx`).

          Trzy fakty w jednym zdaniu i ani jednego więcej: bieg się zatrzymuje, praca siedzi na
          własnej gałęzi, nic nie wyjeżdża na zewnątrz. Czwarty („i wtedy zrób to i to") byłby
          instrukcją, a ta karta odpowiada na pytanie, nie wydaje polecenia. */}
      <span className="text-body">
        The run stops there. Everything the agents changed waits on its own branch in this project —
        nothing is pushed and nothing reaches your own branch until you take it.
      </span>
    </div>
  );
}
