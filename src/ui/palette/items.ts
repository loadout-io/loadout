/* Co stoi na liście palety i w jakiej kolejności — funkcja czysta nad tym, co już wczytane.
 *
 * KOLEJNOŚĆ JEST TREŚCIĄ, nie porządkiem alfabetycznym. Paleta robi trzy rzeczy i mają one
 * różną wagę: skok do sekcji jest ruchem, który człowiek wykonuje kilkadziesiąt razy dziennie;
 * uruchomienie zapisanego workflow kilka razy; otwarcie zapisanego agenta rzadziej. Lista
 * posortowana po nazwie stawiałaby na pierwszej pozycji to, co akurat zaczyna się na „A",
 * i pierwsze naciśnięcie Enter trafiałoby w rzecz przypadkową. Filtrowanie tej kolejności
 * NIE rusza: to samo pytanie zadane z wpisanym słowem ma dostać tę samą hierarchię.
 *
 * SEKCJE PRZYCHODZĄ Z REJESTRU, nigdy z tablicy przepisanej tutaj (niezmiennik 13). Etykieta,
 * którą człowiek czyta w palecie, jest tą samą etykietą, którą czyta w nawigacji, bo pochodzi
 * z tego samego wiersza `src/ui/sections.tsx`.
 *
 * DLACZEGO WPISY ZAPISANE PRZYCHODZĄ ARGUMENTEM. Czytanie biblioteki jest wywołaniem do Rusta,
 * a `renderToStaticMarkup` nie odpala efektów — lista zbudowana wewnątrz tego modułu byłaby
 * listą, której żadne kryterium nie zobaczy inaczej niż pustą.
 */
import type { Section } from '../sections';
import { SECTIONS } from '../sections';
import { JUMPS } from './keys';

/** Jedna zapisana rzecz w bibliotece: czym ją nazwać i czym ją wskazać. */
export interface Saved {
  /** Nazwa pliku (workflow) albo identyfikator (agent) — to, co jedzie dalej, nie na ekran. */
  readonly id: string;
  /** Napis, który czyta człowiek. */
  readonly label: string;
}

/**
 * Pozycja listy. Unia, nie jedno pole `id` z rodzajem obok: każdy z trzech rodzajów jedzie
 * w INNE miejsce, a wspólne pole `id` znaczyłoby rzutowanie przy każdym wyborze.
 */
export type PaletteItem =
  | {
      readonly kind: 'section';
      readonly section: Section;
      readonly label: string;
      /** Litera skoku, albo `null`, kiedy ta sekcja żadnej nie dostała (patrz `./keys.ts`). */
      readonly letter: string | null;
    }
  | { readonly kind: 'workflow'; readonly path: string; readonly label: string }
  | { readonly kind: 'agent'; readonly agent: string; readonly label: string };

/** Klucz Reacta i znacznik dla kryteriów. Rodzaj w środku, bo nazwa pliku i id mogą się zbiec. */
export function keyOf(item: PaletteItem): string {
  if (item.kind === 'section') return 'section:' + item.section;
  if (item.kind === 'workflow') return 'workflow:' + item.path;
  return 'agent:' + item.agent;
}

/* Litera, którą dostała ta sekcja — odwrócenie mapy z `./keys.ts`, a nie druga jej kopia. */
function letterFor(section: Section): string | null {
  for (const [letter, id] of JUMPS) {
    if (id === section) return letter.toUpperCase();
  }
  return null;
}

/** Wszystko, co paleta umie zrobić, w kolejności ważności. */
export function paletteItems(workflows: readonly Saved[], agents: readonly Saved[]): PaletteItem[] {
  const items: PaletteItem[] = SECTIONS.map((entry) => ({
    kind: 'section',
    section: entry.id,
    label: entry.label,
    letter: letterFor(entry.id),
  }));
  for (const one of workflows) {
    items.push({ kind: 'workflow', path: one.id, label: one.label });
  }
  for (const one of agents) {
    items.push({ kind: 'agent', agent: one.id, label: one.label });
  }
  return items;
}

/**
 * Czy ten napis odpowiada temu, co człowiek wpisał.
 *
 * Podciąg, nie prefiks: nazwy workflow są zdaniami („ship a feature"), a człowiek pamięta
 * z nich zwykle słowo ze środka. Bez wpisanego słowa przechodzi wszystko — pusta paleta,
 * która niczego nie pokazuje, dopóki nie zaczniesz pisać, ukrywa to, po co się ją otwiera.
 */
export function keeps(label: string, typed: string): boolean {
  const wanted = typed.trim().toLowerCase();
  if (wanted === '') return true;
  return label.toLowerCase().includes(wanted);
}

/** To, co zostaje po wpisaniu słowa — w tej samej kolejności, w której przyszło. */
export function matching(items: readonly PaletteItem[], typed: string): PaletteItem[] {
  return items.filter((item) => keeps(item.label, typed));
}
