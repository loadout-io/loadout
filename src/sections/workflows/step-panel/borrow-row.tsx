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
 * w [`useHostMaterial`], i tylko ono ma efekt.
 *
 * DLACZEGO LISTA JEST ZWINIĘTA, A LICZBA NIE (2026-08-31). Półki przychodzą z `.claude/` cudzego
 * repozytorium, więc ich długość nie jest niczym ograniczona: ten wiersz rozwijał w panelu tyle
 * pól wyboru, ile ktoś tam położył, i robił to w kolumnie 330 px. „Ile" jest odpowiedzią,
 * której człowiek potrzebuje najczęściej, więc stoi przed listą, a nie za nią.
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

/* `ROW`, `LABEL` i `MISSING` zniknęły 2026-08-31: rolę niosą `.stack`, `.label` i `.lead`
 * z `theme.css`. Zostają dwa napisy, bo oba niosą coś PONAD rolę — wcięcie półki i klej
 * układu pola wyboru, którego prymityw celowo nie wchłania (DESIGN §6). */
const SHELF = 'label pl-1';
const CHOICE = 'flex items-baseline gap-2 text-body text-ink pl-4';

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

/** Trzy półki tego folderu, czytane OBRONNIE.
 *
 * `material?.skills ?? []` na każdej, i to nie jest ostrożność na zapas: odpowiedź przychodzi
 * z drugiej strony mostu, więc jej kształt jest obietnicą typu, a nie faktem (niezmiennik 5).
 * Zmierzone 2026-08-31: pod atrapą, która na każde `list_*` oddaje pustą LISTĘ, `material` jest
 * tablicą — `material.skills` to wtedy `undefined`, a `.length` na nim wywraca cały panel kroku
 * i kafelek przestaje się otwierać. Ta sama czytelnia dla obu czytelników, żeby jeden z nich
 * nie był twardszy od drugiego. */
function shelvesOf(
  material: HostMaterial | null,
): { kind: BorrowKind; label: string; found: string[] }[] {
  return [
    { kind: 'skills', label: 'Skills', found: material?.skills ?? [] },
    { kind: 'learnings', label: 'What this project has learned', found: material?.learnings ?? [] },
    { kind: 'subagents', label: 'Roles', found: material?.subagents ?? [] },
  ];
}

/** Ile ten folder ma do użyczenia, licząc wszystkie trzy półki razem. */
function lends(material: HostMaterial | null): number {
  return shelvesOf(material).reduce((count, shelf) => count + shelf.found.length, 0);
}

/**
 * Czy ten wiersz ma po co powstać — jedna odpowiedź, dwóch czytelników: sam wiersz i panel,
 * który liczy, ile rzeczy stoi za ujawnieniem.
 *
 * „Nie ma" znaczy nie ma (patrz nagłówek pliku). Krok, który już coś pożycza, dostaje wiersz
 * mimo pustego folderu, bo inaczej nie miałby jak tego odznaczyć.
 */
export function borrowRowStands(material: HostMaterial | null, value: Borrow): boolean {
  return lends(material) > 0 || !nothingBorrowed(value);
}

/** Zdanie zwiniętej listy: ile ten folder użycza i ile z tego krok już bierze.
 *
 * Ta lista też nie ma sufitu — przychodzi z `.claude/` cudzego repozytorium — więc rozwijała
 * w panelu tyle pól wyboru, ile ktoś tam położył. */
function saysWhenShut(offered: number, taken: number): string {
  const has = `${String(offered)} to borrow`;
  return taken === 0 ? has : `${has}, ${String(taken)} taken`;
}

export function BorrowRow({ material, value, onChoose }: BorrowRowProps): ReactElement | null {
  const shelves = shelvesOf(material);

  if (!borrowRowStands(material, value)) return null;

  const taken = shelves.reduce((count, shelf) => count + pickedOn(value, shelf.kind).length, 0);

  return (
    <div data-row="borrow" className="stack">
      <span className="label">Borrow from this project</span>

      <details className="rounded-md border border-line p-2">
        <summary className="label cursor-pointer">{saysWhenShut(lends(material), taken)}</summary>
        <div className="stack pt-2">
          {shelves.map((shelf) => {
            const picked = pickedOn(value, shelf.kind);
            /* Zaznaczone i nieobecne idą NA KONIEC swojej półki, po tym, co w folderze naprawdę
             * jest: lista zaczyna się wtedy od tego, co da się wziąć dzisiaj. */
            const stale = picked.filter((name) => !shelf.found.includes(name));
            if (shelf.found.length === 0 && stale.length === 0) return null;

            return (
              <div key={shelf.kind} className="stack">
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
                      <span className="lead">not in this folder</span>
                    ) : null}
                  </label>
                ))}
              </div>
            );
          })}
        </div>
      </details>
    </div>
  );
}

/**
 * Co folder aktywnego workspace ma do użyczenia — spytawszy o to Rusta.
 *
 * Pobranie mieszka w tym pliku, a nie w edytorze, bo to jest jedyny czytelnik tej listy. Prop
 * przeciągnięty przez dwa ekrany dawałby dwa miejsca, w których można podać folder innego
 * workspace niż ten, o którym wiersz mówi.
 *
 * 2026-08-31 — Z KOMPONENTU (`BorrowRowForThisProject`) NA HAK. Odkąd panel liczy, ile rzeczy
 * stoi za ujawnieniem, musi wiedzieć, CZY ten wiersz w ogóle powstanie — a to zależy od tej
 * odpowiedzi. Komponent, który sam ją pobierał, trzymał ją poza zasięgiem liczącego, więc
 * licznik mówiłby o wierszu, którego nie ma. Hak oddaje tę samą wartość temu, kto jej
 * potrzebuje, i nie mnoży miejsc, w których się o nią pyta.
 *
 * Odmowa z Rusta kończy się PUSTĄ listą, nie zdaniem o błędzie: „nie mam czego pożyczyć" jest
 * o tym folderze prawdą także wtedy, gdy nie dało się go przeczytać, a wiersz z komunikatem
 * o systemie plików stałby w panelu kroku, którego to nie dotyczy.
 */
export function useHostMaterial(): HostMaterial | null {
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

  return material;
}
