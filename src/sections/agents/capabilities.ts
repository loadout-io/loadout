/* Co która aplikacja agentowa naprawdę umie — trzy stany na pole na vendora.
 *
 * SZKIELET. `capability` rzuca, żeby kryterium 6 padło na braku zachowania, a nie na braku
 * modułu (AGENTS.md §2a p. 5).
 *
 * To ma być JEDNA TABELA, czytana przez formularz i przez Rusta, a nie `if vendor === 'codex'`
 * rozsiane po komponentach (niezmiennik 23). Tak umarło skanowanie sekretów w repo źródłowym:
 * polityka przepisana w adapterze, po jednej kopii na vendora, i po pół roku dwie z nich
 * mówiły co innego.
 *
 * Trzy stany, bo dwa kłamią [T4 §6.1]: „jest" i „nie ma" nie mają gdzie zapisać ustawienia,
 * które istnieje, ale jest przybliżeniem — a takich jest u nas najwięcej.
 */
import type { Vendor } from '../../state/agents';

export type Capability = 'native' | 'approximate' | 'unavailable';

/** Pola definicji agenta, które w ogóle trafiają do vendora. */
export type CapabilityField =
  | 'instructions'
  | 'model'
  | 'thinking'
  | 'fileAccess'
  | 'giveUpAfterMinutes'
  | 'tools'
  | 'skills'
  | 'connections';

export function capability(_field: CapabilityField, _vendor: Vendor): Capability {
  throw new Error('the table of what each agent app can do is not written yet');
}
