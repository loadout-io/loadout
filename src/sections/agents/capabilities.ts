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
import type { FileAccess, Vendor } from '../../state/agents';

export type Capability = 'native' | 'approximate' | 'unavailable';

/** Pola definicji agenta, które w ogóle trafiają do vendora. */
export type CapabilityField =
  | 'instructions'
  | 'model'
  | 'thinking'
  | 'fileAccess'
  | 'giveUpAfterMinutes'
  | 'tools'
  | 'reachesTheWeb'
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
  /* NATIVE U OBU, i to jest cały powód, dla którego sieć jest osobnym polem, a nie pozycją na
   * liście narzędzi: `tools` jest u Codeksa `unavailable`, więc kontrolka „wpisz WebSearch"
   * działałaby dla połowy agentów i milczała dla drugiej. Dostęp do internetu umieją wyrazić
   * obaj — Claude dwoma czasownikami, Codex ustawieniem piaskownicy. */
  reachesTheWeb: { 'claude-code': 'native', codex: 'native' },
  skills: { 'claude-code': 'native', codex: 'approximate' },
  connections: { 'claude-code': 'native', codex: 'native' },
};

export function capability(field: CapabilityField, vendor: Vendor): Capability {
  return CAPABILITIES[field][vendor];
}

/* Na których pozycjach diala TA aplikacja naprawdę sięga do sieci.
 *
 * `null` znaczy „na każdej" i jest osobną wartością, nie listą trzech: lista wymieniająca
 * wszystkie pozycje przestałaby być prawdziwa w dniu, w którym dojdzie czwarta, i nikt by tego
 * nie zauważył — bo wyglądałaby dokładnie tak samo jak dziś.
 *
 * TABELA, A NIE `if vendor === 'codex'` W FORMULARZU, i to jest ten sam powód, dla którego stoi
 * tu `CAPABILITIES` (niezmiennik 23). `reachesTheWeb` jest u obu `native` i to jest prawda: obaj
 * umieją wyrazić dostęp do sieci. Ta tabela odpowiada na drugie, węższe pytanie — CZYM go
 * wyrażają. Claude dwoma czasownikami, więc dostaje je na każdym dialu. Codex ustawieniem
 * piaskownicy (`network_access`), a ta otwiera się dopiero przy `workspace-write`, czyli przy
 * „ask first" i „work freely" [T4 §6.3; `engine/drivers/codex.rs`, `build_exec_argv`].
 *
 * Zmierzone 2026-08-23 w bibliotece właściciela: 18 agentów, ani jeden z siecią. Agent Codeksa
 * na „look only" z włączonym przełącznikiem sieci nie ma sieci — i do tego dnia nic mu tego nie
 * mówiło, więc z zewnątrz wyglądał jak agent, który nie chciał poszukać. */
const WEB_NEEDS_THESE_DIALS: Record<Vendor, readonly FileAccess[] | null> = {
  'claude-code': null,
  codex: ['ask-first', 'work-freely'],
};

/**
 * Czy ta aplikacja na tej pozycji diala do sieci NIE sięgnie, choćby przełącznik był włączony.
 */
export function webIsOutOfReach(vendor: Vendor, fileAccess: FileAccess): boolean {
  const dials = WEB_NEEDS_THESE_DIALS[vendor];
  return dials !== null && !dials.includes(fileAccess);
}
