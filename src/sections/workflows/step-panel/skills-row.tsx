/* Wiersz Skills — obiecuje dokładnie tyle, ile potrafi CLI.
 *
 * SZKIELET — ciało rzuca `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Tryb przychodzi PROPSEM, choć w aplikacji jest jedną stałą (`SKILL_SUBSETTING`
 * w `capabilities.ts`). To nie jest nadmiarowość: dzięki temu wynik spike'u S-1 zmienia jedną
 * linię i zero testów, a oba warianty da się sprawdzić w jednym biegu.
 *
 * `'all-or-none'` znaczy, że „Only these" NIE ISTNIEJE — nie jest wyszarzone. Kontrolka
 * wyszarzona dalej obiecuje funkcję, tylko „na później"; kontrolka, która niczego nie zapisuje,
 * to niezmiennik 16 i anty-wzorzec „UI zbudowane na polu, którego nie ma" (00-SYNTHESIS §6).
 *
 * Przy agencie na Codeksie całego wiersza nie ma: Codex nie ma pojęcia umiejętności
 * [T3 §7.2, T4 fakt-check O4]. Wiersz włączony, który nic nie robi, jest gorszy niż jego brak,
 * bo wygląda tak samo jak działający.
 */
import type { ReactElement } from 'react';
import type { Vendor } from '../../../state/agents';
import type { SkillChoice, Skills } from '../../../state/workflows';
import type { SkillMode } from './capabilities';

export interface SkillsRowProps {
  mode: SkillMode;
  /** Vendor AGENTA, którego wybrano na tym kroku. Codex nie ma umiejętności. */
  runsWith: Vendor;
  /** Umiejętności, które da się wskazać. Puste w trybie `all-or-none`. */
  available: string[];
  /** Wartość efektywna kroku. */
  value: Skills;
  onChoose: (choice: SkillChoice) => void;
}

export function SkillsRow(_props: SkillsRowProps): ReactElement | null {
  throw new Error('not implemented');
}
