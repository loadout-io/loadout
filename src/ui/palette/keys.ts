/* Co znaczy naciśnięty klawisz — jedyne miejsce w aplikacji, które to rozstrzyga.
 *
 * PO CO TO ISTNIEJE. Do 2026-08-31 ta aplikacja nie miała ANI JEDNEGO skrótu globalnego:
 * zmierzone gerpem, cztery lokalne `onKeyDown` w całym drzewie i zero pól wyszukiwania. Każde
 * przejście między sekcjami kosztowało sięgnięcie po mysz, a ekran, który człowiek odwiedza
 * kilkadziesiąt razy dziennie, nie miał drogi z klawiatury.
 *
 * NAJCZĘSTSZA WADA TEGO WZORCA — i to jest powód, dla którego decyzja mieszka tutaj, a nie
 * w nasłuchu. Skok bez modyfikatora („G", potem litera) zapisany wprost w handlerze zaczyna
 * skakać po aplikacji w chwili, w której człowiek wpisuje słowo „grand" w zwykłe pole tekstowe:
 * `g` uzbraja skok, `r` go wykonuje, a wpisywane słowo zostaje w połowie, na ekranie, którego
 * nikt nie zamawiał. Warunek „ognisko nie jest w polu, które przyjmuje pisanie" jest więc
 * PIERWSZY i jest funkcją czystą. To repo nie ma jsdom, więc reguła zamknięta w handlerze
 * byłaby regułą, której żadne kryterium nie umie dotknąć.
 *
 * LITERY SKOKU SĄ WYPROWADZONE Z REJESTRU SEKCJI, nigdy przepisane (niezmiennik 13). Lista
 * sekcji ma dokładnie jedno miejsce zamieszkania i jest nim `src/ui/sections.tsx`; druga
 * tablica par litera→sekcja rozjechałaby się z nią przy pierwszej dopisanej sekcji, a rozjazd
 * czyta się jak martwy skrót. Reguła kolizji jest zapisana, a nie ukryta: przy dwóch sekcjach
 * na tę samą literę bierze ją ta, która stoi w rejestrze wyżej, a druga skoku nie ma. Do
 * 2026-08-31 kosztowało to Settings (`skills` było wyżej); po scaleniu Skills i Memory
 * w Knowledge sześć sekcji ma sześć różnych pierwszych liter i nie koliduje ani jedna — także
 * `S`, które wróciło do Settings. Lista skrótów pokazywana człowiekowi powstaje z TEGO SAMEGO
 * wyprowadzenia, więc nie ma jak obiecać skrótu, którego klawiatura nie zna.
 */
import type { Section } from '../sections';
import { SECTIONS } from '../sections';

/** Tyle o naciśnięciu, ile potrzeba do decyzji — i ani pola więcej, żeby dało się je napisać. */
export interface Pressed {
  readonly key: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/** Tyle o elemencie z ogniskiem, ile potrzeba, żeby wiedzieć, czy człowiek właśnie pisze. */
export interface Focused {
  readonly tagName: string;
  readonly isContentEditable: boolean;
}

/**
 * Co ma się stać. `wait` znaczy „G naciśnięte, czekam na literę" i jest wartością, nie stanem
 * ukrytym w module: nasłuch trzyma tę jedną zapadkę u siebie i oddaje ją z powrotem argumentem,
 * więc cała reguła da się osądzić bez okna.
 */
export type Move =
  | { readonly move: 'open' }
  | { readonly move: 'shortcuts' }
  | { readonly move: 'jump'; readonly section: Section }
  | { readonly move: 'wait' }
  | { readonly move: 'none' };

/** Co dalej, kiedy ognisko JEST już w palecie. Osobne pytanie, bo inna odpowiedź. */
export type Inside =
  | { readonly move: 'step'; readonly by: number }
  | { readonly move: 'choose' }
  | { readonly move: 'close' }
  | { readonly move: 'none' };

/* Elementy, które przyjmują pisanie. `SELECT` jest na liście, choć nie przyjmuje liter do
 * treści: przyjmuje je jako skok po pozycjach listy, więc `g` w rozwiniętym wyborze należy
 * do niego, nie do nawigacji. Pole wyboru bez tekstu (`checkbox`) też tu wpada i to jest
 * świadome — węższa reguła kosztowałaby drugie pytanie o typ kontrolki, a zysk z tego, żeby
 * `G R` działało nad zaznaczonym polem wyboru, jest zerowy. */
const TAKES_TYPING = new Set(['INPUT', 'TEXTAREA', 'SELECT']);

/** Czy to, co ma ognisko, przyjmuje pisanie. `null` znaczy „nic", czyli wolno skakać. */
export function takesTyping(focused: Focused | null): boolean {
  if (focused === null) return false;
  if (focused.isContentEditable) return true;
  return TAKES_TYPING.has(focused.tagName.toUpperCase());
}

/**
 * Element DOM sprowadzony do dwóch pól, których potrzebuje [`takesTyping`].
 *
 * Bez `instanceof HTMLElement`: testy tego repo biegną w node, gdzie tej nazwy nie ma w ogóle,
 * więc sprawdzenie typu przez konstruktor zamieniłoby regułę w wyjątek środowiska.
 */
export function focusedShape(element: Element | null): Focused | null {
  if (element === null) return null;
  const editable = (element as { isContentEditable?: boolean }).isContentEditable;
  return { tagName: element.tagName, isContentEditable: editable === true };
}

/* Wyprowadzenie, nie tablica. Liczone RAZ, przy wczytaniu modułu: odpowiedź zależy wyłącznie
 * od rejestru sekcji, a ten w trakcie życia okna nie zmienia się ani razu. */
function firstLetters(): ReadonlyMap<string, Section> {
  const letters = new Map<string, Section>();
  for (const entry of SECTIONS) {
    const letter = entry.id.slice(0, 1);
    /* Pierwsza sekcja z tą literą ją bierze. Kolejność rejestru jest częścią kontraktu
     * (`src/ui/sections.tsx`), więc to jest reguła, a nie wynik przypadku. */
    if (!letters.has(letter)) letters.set(letter, entry.id);
  }
  return letters;
}

/** Litera → sekcja, wyprowadzone z rejestru. Czyta to i klawiatura, i lista skrótów. */
export const JUMPS: ReadonlyMap<string, Section> = firstLetters();

/** Sekcja pod tą literą, albo `null`. Wielkość liter nie ma znaczenia. */
export function jumpFor(letter: string): Section | null {
  return JUMPS.get(letter.toLowerCase()) ?? null;
}

/**
 * Co ma zrobić okno, kiedy ktoś nacisnął klawisz i paleta jeszcze nie ma ogniska.
 *
 * @param waiting czy poprzednim naciśnięciem było `G`, czyli czy czekamy na literę skoku.
 */
export function moveFor(pressed: Pressed, focused: Focused | null, waiting: boolean): Move {
  const key = pressed.key.toLowerCase();
  const held = pressed.metaKey || pressed.ctrlKey;

  /* Paleta otwiera się TAKŻE spod pola tekstowego, i to nie jest wyjątek od reguły niżej:
   * modyfikator jest tym, co odróżnia skrót od pisania, więc `⌘K` nie ma jak wpaść do treści.
   * To jedyne naciśnięcie w tym pliku, które przechodzi nad ogniskiem. */
  if (held && !pressed.altKey && key === 'k') return { move: 'open' };

  /* CAŁA REGUŁA, o którą chodzi. Wszystko poniżej to skróty BEZ modyfikatora, więc każde z nich
   * jest jednocześnie znakiem, który człowiek może chcieć wpisać. */
  if (takesTyping(focused)) return { move: 'none' };
  if (held || pressed.altKey) return { move: 'none' };

  if (pressed.key === '?') return { move: 'shortcuts' };

  if (waiting) {
    const section = jumpFor(key);
    /* Litera, która nie jest skokiem, ROZBRAJA i nic nie robi. Bez tego `G`, po którym człowiek
     * rozmyślił się i nacisnął cokolwiek, zostawałoby uzbrojone do końca życia okna. */
    return section === null ? { move: 'none' } : { move: 'jump', section };
  }

  return key === 'g' ? { move: 'wait' } : { move: 'none' };
}

/** Co ma zrobić paleta, kiedy ognisko jest w niej. Strzałki chodzą, Enter wybiera, Escape zamyka. */
export function insideMove(pressed: Pressed): Inside {
  if (pressed.key === 'Escape') return { move: 'close' };
  if (pressed.key === 'Enter') return { move: 'choose' };
  if (pressed.key === 'ArrowDown') return { move: 'step', by: 1 };
  if (pressed.key === 'ArrowUp') return { move: 'step', by: -1 };
  return { move: 'none' };
}

/**
 * Podświetlona pozycja po ruchu strzałką — zawinięta, nigdy poza listą.
 *
 * Zawijanie, a nie zatrzymanie na krańcu: lista ma zwykle trzy pozycje, a droga w dół do
 * ostatniej i z powrotem to dwa naciśnięcia zamiast siedmiu. Pusta lista oddaje zero, żeby
 * podświetlenie nie wskazywało pozycji, której nie ma.
 */
export function stepped(at: number, by: number, howMany: number): number {
  if (howMany <= 0) return 0;
  return (((at + by) % howMany) + howMany) % howMany;
}
