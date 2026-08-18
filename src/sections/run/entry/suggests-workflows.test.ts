/* Wiersz wejścia podpowiada NAZWY WORKFLOW po `/run` — i nie miesza ich z komendami.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-19: „powinno podpowiadać jakie workflow, tam
 * podpowiadajka powinna być". Makieta obiecuje to samo w drugiej linii tego wiersza („Tab completes
 * a workflow or a repo Loadout has seen"), a `/run` bez podpowiedzi jest komendą, która wymaga
 * nazwy i nie mówi jakiej — przy nazwach powstających z plików na dysku nie ma ich jak zgadnąć.
 *
 * SŁABA WERSJA: sprawdzić, że `suggestions('/run ', names)` nie jest puste. Przechodzi dla funkcji,
 * która oddaje wszystko na każde wejście — czyli dla listy workflow wiszącej pod `/open` i pod
 * zdaniem do agenta. Rozstrzygają cztery rzeczy naraz: zawężanie po prefiksie, MILCZENIE po
 * wybraniu nazwy, brak nazw workflow przy samym ukośniku i pominięcie workflow bez kroków.
 */
import { describe, expect, it } from 'vitest';

import { KNOWN, suggestions } from './entry';
import type { Choice } from '../choices';
import { workflowNames } from '../run-command';

/** Krok planu w kształcie, którego chce `Choice`. */
function step(id: string) {
  return { id, name: id, state: 'pending' as const };
}

const SHIP: Choice = {
  path: 'ship-a-feature.json',
  name: 'Ship a feature',
  steps: [step('s_plan'), step('s_build')],
};

const TODO: Choice = { path: 'todo-list.json', name: 'Todo list', steps: [step('s_lead')] };

/** Szkic bez kroków. Uruchomiony odmawia, więc podpowiadanie go zaprasza do odmowy. */
const DRAFT: Choice = { path: 'new-workflow.json', name: 'New workflow', steps: [] };

const NAMES = workflowNames([SHIP, TODO, DRAFT]);

describe('/run suggests the workflows there are', () => {
  it('offers every runnable workflow once the command is typed', () => {
    expect(
      suggestions('/run ', NAMES).map((one) => one.name),
      'a person who typed /run and a space is choosing a workflow, and the only place that list ' +
        'exists is on disk — so the row has to show it rather than leave them guessing',
    ).toEqual(['ship-a-feature', 'todo-list']);
  });

  it('leaves out a workflow that has no steps', () => {
    /* Podpowiedziana nazwa, która po Enterze odmawia („There are no steps yet."), jest
     * zaproszeniem do odmowy. Szkic wróci na listę, kiedy dostanie pierwszy krok. */
    expect(
      NAMES.map((one) => one.name),
      'an empty draft must not be offered: pressing Enter on it refuses on the Rust side, so the ' +
        'suggestion would be a control that leads nowhere (invariant 16)',
    ).not.toContain('new-workflow');
  });

  it('narrows to what was typed after the command', () => {
    expect(
      suggestions('/run to', NAMES).map((one) => one.name),
      'typing narrows this list exactly like it narrows the command list; a filter that ignores ' +
        'the partial name offers every workflow to somebody half way through one of them',
    ).toEqual(['todo-list']);
    expect(
      suggestions('/run zzz', NAMES),
      'a name nobody has leaves the list empty here — the sentence that names what DOES exist ' +
        'belongs to Enter, which has room to print them',
    ).toEqual([]);
  });

  it('says nothing more once the name is settled and the task is being typed', () => {
    expect(
      suggestions('/run todo-list build me a pretty list', NAMES),
      'past the second space the person is writing the task, and a workflow list hanging under ' +
        'that sentence is noise that also implies Tab will still complete something',
    ).toEqual([]);
  });

  it('never mixes workflow names into the command list', () => {
    /* Dwie listy, jeden renderer, ale nigdy naraz: `/` pyta „jakie komendy", a `/run ` pyta
     * „jakie workflow". Zlanie ich dałoby wiersz, w którym `todo-list` wygląda na komendę. */
    expect(
      suggestions('/', NAMES).map((one) => one.name),
      'a bare slash asks which COMMANDS exist. Offering workflow names here would make them look ' +
        'like commands, and the row does not understand them as commands.',
    ).toEqual(KNOWN.map((one) => one.name));
  });

  it('still keeps a command suggested while its own argument is being typed', () => {
    /* Kryterium regresyjne: gałąź `/run` wpięła się w tę samą funkcję, więc `/open <ścieżka>`
     * musi zachować się dokładnie jak przedtem. */
    expect(
      suggestions('/open ~/Projects/x', NAMES).map((one) => one.name),
      'losing the suggestion half way through a path would tell the person their command stopped ' +
        'being valid, which is not true',
    ).toEqual(['/open']);
  });

  it('describes each workflow by the name it gives itself and how big it is', () => {
    const todo = NAMES.find((one) => one.name === 'todo-list');
    expect(todo?.does, 'the typable name can be unrecognisable next to the real one').toContain(
      'Todo list',
    );
    /* Liczba kroków jest jedyną rzeczą na tej liście, która odróżnia mały workflow od takiego,
     * który uruchomi sześciu agentów i zacznie płacić. Liczba pojedyncza, bo „1 steps" na ekranie
     * czyta się jak błąd (niezmiennik 14 w duchu). */
    expect(todo?.does, 'one step is a step, not steps').toContain('1 step');
    expect(todo?.does, 'and it must not read as a plural of one').not.toContain('1 steps');
  });
});
