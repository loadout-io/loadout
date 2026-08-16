/* Kafelek listy workflow (makieta `docs/mockup/index.html:642-650`).
 *
 * Kafelek pokazuje WYŁĄCZNIE to, co jest w pliku: nazwę, jedno zdanie opisu i wiersz
 * metadanych `N steps` · `M agents`, gdzie `M` liczy RÓŻNE identyfikatory agentów w krokach.
 * `used 12×` i `~6 min` z makiety wymagają historii biegów (T-06) i wchodzą razem z nią —
 * nigdy jako `—`, `never` ani `not reported`. Pole, które nigdy nie będzie miało treści,
 * zajmuje miejsce na ekranie i tłumaczy się użytkownikowi z własnej pustki; poprzedni prototyp
 * zostawił po sobie dokładnie taką komórkę `SPEND: not reported` (00-SYNTHESIS §6).
 *
 * Ten komponent nie dostaje żadnej akcji i to jest decyzja, nie przeoczenie: kontrolka bez
 * handlera nie wchodzi do repo (niezmiennik 16), a `Duplicate` i `Delete` mają handlery
 * jedno piętro wyżej, w `workflow-list.tsx`, gdzie mieszka obiekt `actions`. Kafelek zostaje
 * funkcją pliku.
 *
 * Czego wymagają kryteria od markupu: element nośny niesie `data-tile`, opis renderuje się
 * TYLKO wtedy, gdy jest (pusty `<p></p>` jest zakazany), a odmiana słowa idzie za liczbą —
 * `1 step`, nie `1 steps`.
 */
import type { ReactElement } from 'react';
import type { WorkflowFile } from './store';

export interface WorkflowTileProps {
  wf: WorkflowFile;
}

/* Szkielet fazy kontraktu — odpowiednik `todo!()`. Rzuca, więc kryterium pada w czasie
 * wykonania, na braku ZACHOWANIA, a nie przy rozwiązywaniu importu (AGENTS.md §2a).
 * Pusty fragment byłby tu gorszy: asercje negatywne („nie ma pustego akapitu", „nie ma
 * `used`") przechodzą na pustym markupie i w warstwie `before` świeciłyby na zielono,
 * niczego nie sprawdzając. */
export function WorkflowTile(_props: WorkflowTileProps): ReactElement {
  throw new Error('not implemented');
}
