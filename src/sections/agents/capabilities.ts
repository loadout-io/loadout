/* Co która aplikacja agentowa naprawdę umie — trzy stany na pole na vendora.
 *
 * To jest JEDNA TABELA, czytana przez formularz, a nie `if vendor === 'codex'` rozsiane po
 * komponentach (niezmiennik 23). Tak umarło skanowanie sekretów w repo źródłowym: polityka
 * przepisana w adapterze, po jednej kopii na vendora, i po pół roku dwie z nich mówiły co
 * innego.
 *
 * Trzy stany, bo dwa kłamią [T4 §6.1]: „jest" i „nie ma" nie mają gdzie zapisać ustawienia,
 * które istnieje, ale jest przybliżeniem — a takich jest u nas najwięcej.
 *
 * Ten plik jest LUSTREM `CAPABILITIES` z `src-tauri/src/library/agents.rs`, dokładnie tak jak
 * typy w `src/state/agents.ts` są lustrem tamtejszych struktur. Dopóki nie ma generatora
 * (`ts-rs` albo `specta` — T4 §7.2), obie kopie stoją obok siebie z tym samym datowanym
 * źródłem: T4 §6.3, zweryfikowane 2026-08-15 przez `--help` obu aplikacji.
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

/* Macierz z T4 §6.3. Pole odpowiada NAJSŁABSZYM ze swoich tłumaczeń — stąd `fileAccess`
 * przybliżone u obu: u Claude'a `look-only` to tryb planowania, u Codeksa `ask-first`
 * i `work-freely` to ta sama piaskownica. „Native" znaczyłoby wtedy „część działa dokładnie",
 * a tego zdania nie chcemy mówić o dialu bezpieczeństwa.
 *
 * Kształt jest `Record<pole, Record<vendor, stan>>`, a nie lista par: para bez odpowiedzi
 * przestaje się wtedy kompilować, zamiast oddawać `undefined` kontrolce, która nie wie,
 * jak się narysować. */
const CAPABILITIES: Record<CapabilityField, Record<Vendor, Capability>> = {
  instructions: { 'claude-code': 'native', codex: 'native' },
  model: { 'claude-code': 'native', codex: 'native' },
  thinking: { 'claude-code': 'native', codex: 'native' },
  fileAccess: { 'claude-code': 'approximate', codex: 'approximate' },
  giveUpAfterMinutes: { 'claude-code': 'native', codex: 'native' },
  tools: { 'claude-code': 'native', codex: 'unavailable' },
  skills: { 'claude-code': 'native', codex: 'approximate' },
  connections: { 'claude-code': 'native', codex: 'native' },
};

export function capability(field: CapabilityField, vendor: Vendor): Capability {
  return CAPABILITIES[field][vendor];
}
