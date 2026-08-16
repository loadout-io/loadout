/* Kafelek na płótnie. Dwa rodzaje, cztery linie tekstu, stopka WYLICZANA ze strzałek.
 *
 * SZKIELET — ciało rzuca `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Niezmiennik 17 mieszka w kształcie propsów. Stopka (`first step` / `after Plan` /
 * `reads 3 handoffs` po lewej, `runs before ▸` po prawej) nie jest napisem podanym z zewnątrz
 * ani wpisanym na sztywno w komponent — jest liczona z `links`. Wpisana na sztywno wygląda
 * identycznie do chwili, w której ktoś przesunie strzałkę, i wtedy kłamie po cichu.
 *
 * Dlatego kafelek dostaje CAŁE `links` i `steps`, a nie gotowy podpis: `steps` są potrzebne,
 * bo stopka nazywa poprzednika NAZWĄ, a `s_plan` nie jest niczym, co użytkownik widzi.
 * Przy piętnastu kafelkach koszt jest zerowy, a alternatywa — policzenie podpisu w płótnie
 * i podanie propsem — przenosi ten sam kod o jedno piętro wyżej i wyjmuje go spod kryterium.
 *
 * Komponent jest STEROWANY: `selected` przychodzi propsem i NIE jest zapisywane do pliku
 * (T3 §3.3, kryterium `to-file`).
 */
import type { ReactElement } from 'react';
import type { Link, Step } from '../../../state/workflows';

export interface StepTileProps {
  step: Step;
  /** Wszystkie kroki — stopka nazywa poprzednika po nazwie, nie po identyfikatorze. */
  steps: Step[];
  /** Wszystkie strzałki. Stopka jest z nich wyliczana (niezmiennik 17). */
  links: Link[];
  /** Zaznaczenie jest stanem płótna, nie polem pliku. */
  selected?: boolean;
}

export function StepTile(_props: StepTileProps): ReactElement {
  throw new Error('not implemented');
}
