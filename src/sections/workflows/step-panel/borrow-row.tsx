/* Wiersz „Borrow from this project" — jedyna droga, którą `AgentStep.borrow` powstaje.
 *
 * DLACZEGO WIERSZ, A NIE USTAWIENIE BIEGU. Wybór „pożycz rolę `backend-dev` z tego repozytorium"
 * jest własnością kafelka dokładnie tak samo, jak wybór agenta: dwa kafelki jednego biegu mogą
 * chcieć dwóch różnych rzeczy, a jedno pole na bieg zamieniłoby ten wybór w przełącznik
 * „to repozytorium: tak albo nie".
 *
 * DLACZEGO WIERSZA NIE MA, KIEDY NIE MA CO POŻYCZAĆ. Folder bez `.claude/` to WIĘKSZOŚĆ
 * folderów. Nagłówek nad zerem pozycji obiecuje funkcję, po której nikt nie przyjdzie —
 * a kontrolka bez skutku wygląda dokładnie jak działająca (niezmiennik 16). Wiersz powstaje
 * więc wtedy, gdy jest co zaznaczyć ALBO gdy krok już coś pożycza.
 *
 * DLACZEGO ZAPISANA NAZWA, KTÓREJ TU NIE MA, ZOSTAJE NA EKRANIE. Ciche jej zdjęcie znaczy, że
 * krok przestaje pożyczać rolę przy pierwszym otwarciu panelu w innym projekcie i nic o tym nie
 * mówi. Pokazujemy ją więc z „not in this folder" — a przed odpowiedzią z Rusta (`material ===
 * null`) bez tej etykiety, bo wtedy jeszcze nie wiemy, czy jej nie ma.
 *
 * Wiersz jest STEROWANY, tak jak reszta panelu: w repo nie ma `jsdom` (`package.json` jest na
 * liście DENIED w `checks/quick-scope.sh`), więc sprawdza się go przez `renderToStaticMarkup`,
 * a stan trzymany w środku byłby dla takiego testu niewidoczny. Pobranie listy siedzi obok,
 * w [`BorrowRowForThisProject`], i tylko ono ma efekt.
 */
import { useEffect, useState } from 'react';
import type { ReactElement } from 'react';

import { activeWorkspace } from '../../../state/workspaces';
import type { Borrow, HostMaterial } from '../../../state/workflows';
import { listHostMaterial } from '../io';

/** Trzy półki gospodarza, po nazwach z drutu (`inherit::Lendable`). */
export type BorrowKind = 'skills' | 'learnings' | 'subagents';

export interface BorrowRowProps {
  /** Co ten folder ma do pożyczenia — albo `null`, kiedy jeszcze nie zapytaliśmy. */
  material: HostMaterial | null;
  /** Co ten krok pożycza dzisiaj. */
  value: Borrow;
  onChoose: (borrow: Borrow) => void;
}

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
const SHELF = 'text-label text-muted pl-1';
const CHOICE = 'flex items-baseline gap-2 text-body text-ink pl-4';
const MISSING = 'text-label text-muted';

/** Czy krok nie pożycza niczego. Jedna odpowiedź, dwóch czytelników: ten wiersz i panel. */
export function nothingBorrowed(borrow: Borrow): boolean {
  return (
    (borrow.skills ?? []).length === 0 &&
    borrow.learnings === undefined &&
    borrow.agent === undefined
  );
}

/**
 * Wybór po kliknięciu w jedno pole — ta sama funkcja, którą woła `onChange` niżej.
 *
 * Osobna i eksportowana, bo to jest JEDYNE zachowanie tego wiersza, a `renderToStaticMarkup`
 * nie umie kliknąć. Kontrolka, która się rysuje i niczego nie zapisuje, wygląda dokładnie jak
 * działająca (niezmiennik 16), więc zapis musi dać się wykonać wprost.
 *
 * Umiejętności są listą i zaznaczają się niezależnie. Rola i podagent to POJEDYNCZE pola
 * (`inherit::wire::Chosen`), więc zaznaczenie drugiej pozycji zastępuje pierwszą, a ponowne
 * kliknięcie w zaznaczoną — zdejmuje ją. Wybór, którego nie da się cofnąć z okna, zostawia
 * człowieka z plikiem do otwarcia w edytorze tekstu.
 */
export function ticked(value: Borrow, kind: BorrowKind, name: string): Borrow {
  if (kind === 'skills') {
    const had = value.skills ?? [];
    const next = had.includes(name) ? had.filter((one) => one !== name) : [...had, name];
    /* Klucz znika razem z ostatnią pozycją: pusta lista i brak klucza znaczą tu to samo,
     * a pusta lista dopisana do pliku jest wierszem szumu w każdym kroku. */
    return { ...value, skills: next.length === 0 ? undefined : next };
  }
  if (kind === 'learnings') {
    return { ...value, learnings: value.learnings === name ? undefined : name };
  }
  return { ...value, agent: value.agent === name ? undefined : name };
}

/** Nazwy zaznaczone na tej półce — jedna lub żadna dla pól pojedynczych. */
function pickedOn(borrow: Borrow, kind: BorrowKind): string[] {
  if (kind === 'skills') return borrow.skills ?? [];
  const one = kind === 'learnings' ? borrow.learnings : borrow.agent;
  return one === undefined ? [] : [one];
}

export function BorrowRow({ material, value, onChoose }: BorrowRowProps): ReactElement | null {
  const shelves: { kind: BorrowKind; label: string; found: string[] }[] = [
    { kind: 'skills', label: 'Skills', found: material?.skills ?? [] },
    { kind: 'learnings', label: 'What this project has learned', found: material?.learnings ?? [] },
    { kind: 'subagents', label: 'Roles', found: material?.subagents ?? [] },
  ];

  const anythingToLend = shelves.some((shelf) => shelf.found.length > 0);
  /* „Nie ma" znaczy nie ma — patrz nagłówek pliku. Krok, który już coś pożycza, dostaje wiersz
   * mimo pustego folderu, bo inaczej nie miałby jak tego odznaczyć. */
  if (!anythingToLend && nothingBorrowed(value)) return null;

  return (
    <div className={ROW}>
      <span className={LABEL}>Borrow from this project</span>

      {shelves.map((shelf) => {
        const picked = pickedOn(value, shelf.kind);
        /* Zaznaczone i nieobecne idą NA KONIEC swojej półki, po tym, co w folderze naprawdę
         * jest: lista zaczyna się wtedy od tego, co da się wziąć dzisiaj. */
        const stale = picked.filter((name) => !shelf.found.includes(name));
        if (shelf.found.length === 0 && stale.length === 0) return null;

        return (
          <div key={shelf.kind} className={ROW}>
            <span className={SHELF}>{shelf.label}</span>
            {[...shelf.found, ...stale].map((name) => (
              <label key={name} className={CHOICE}>
                <input
                  type="checkbox"
                  checked={picked.includes(name)}
                  onChange={() => {
                    onChoose(ticked(value, shelf.kind, name));
                  }}
                />
                {name}
                {/* Etykieta dopiero wtedy, gdy WIEMY, czego w folderze nie ma. Przed odpowiedzią
                    z Rusta zdanie „not in this folder" byłoby zgadywaniem o cudzym katalogu. */}
                {material !== null && stale.includes(name) ? (
                  <span className={MISSING}>not in this folder</span>
                ) : null}
              </label>
            ))}
          </div>
        );
      })}
    </div>
  );
}

/**
 * Ten sam wiersz, spytawszy Rusta o folder aktywnego workspace.
 *
 * Pobranie stoi TUTAJ, a nie w panelu i nie w edytorze, bo to jest jedyny czytelnik tej listy.
 * Prop przeciągnięty przez dwa ekrany dawałby dwa miejsca, w których można podać folder innego
 * workspace niż ten, o którym wiersz mówi.
 *
 * Odmowa z Rusta kończy się PUSTĄ listą, nie zdaniem o błędzie: „nie mam czego pożyczyć" jest
 * o tym folderze prawdą także wtedy, gdy nie dało się go przeczytać, a wiersz z komunikatem
 * o systemie plików stałby w panelu kroku, którego to nie dotyczy.
 */
export function BorrowRowForThisProject({
  value,
  onChoose,
}: Omit<BorrowRowProps, 'material'>): ReactElement | null {
  const [material, setMaterial] = useState<HostMaterial | null>(null);

  useEffect(() => {
    let listening = true;
    const done = (found: HostMaterial) => {
      if (listening) setMaterial(found);
    };
    void listHostMaterial(activeWorkspace()?.folder ?? null)
      .then(done)
      .catch(() => {
        done({ skills: [], learnings: [], subagents: [] });
      });
    return () => {
      listening = false;
    };
  }, []);

  return <BorrowRow material={material} value={value} onChoose={onChoose} />;
}
