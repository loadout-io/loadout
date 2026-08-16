/* Dziedziczenie, nie kopia: krok trzyma tylko RÓŻNICĘ wobec agenta.
 *
 * SZKIELET — ciała rzucają `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * To jest ta cicha porażka, przed którą użytkownik ostrzegł wprost: edytujesz krok, a zmienia się
 * AGENT, więc pięć innych workflow po cichu zaczyna działać inaczej. Wygląda dobrze i wszystkie
 * testy przechodzą, bo testy pytają „czy krok ma teraz thinking: deep?", a nigdy „czy agent jest
 * dokładnie taki, jak był?".
 *
 * Obrona jest w typach: te funkcje są CZYSTE i nie mają jak dosięgnąć pliku agenta. Nie dostają
 * `WorkflowIo`, nie importują magazynu agentów, a `agent` biorą tylko po to, żeby policzyć od
 * czego krok się różni. Zapis pliku agenta jest osobną drogą (`WorkflowIo.saveAgent`), której
 * ta ścieżka nie tyka.
 *
 * Druga kopia algebry RFC 7396 (pierwsza: `library::agents::{resolve, capture}` w Ruście) jest
 * świadoma i ma tę samą podstawę co lustro typów: to kilkanaście linii bez stanu, a panel musi
 * pokazać wartość efektywną w tej samej klatce, w której użytkownik wpisał znak. Rust zostaje
 * autorytetem — plik na dysku bywa poprawiony ręcznie i to jego czyta bieg. 2026-08-16.
 */
import type { Agent } from '../../../state/agents';
import type { AgentStep, OverridableField, Overrides } from '../../../state/workflows';

/** Dziewięć pól, które krok może zmienić — lustro `OVERRIDABLE` z `library::agents`.
 *
 * Lista jest FILTREM na wyprodukowanym patchu, nie komentarzem obok pętli: `id`, `name`
 * i `runsWith` nie mają prawa wypłynąć, choćby się różniły. */
export const OVERRIDABLE: readonly OverridableField[] = [
  'instructions',
  'model',
  'thinking',
  'fileAccess',
  'giveUpAfterMinutes',
  'tools',
  'skills',
  'connections',
  'writeResultsTo',
];

/** Agent + różnica → co naprawdę pobiegnie, plus nazwy zmienionych pól dla znacznika
 * „N changed". Nazwy biorą się z KLUCZY PATCHA, nie z porównania dwóch pełnych obiektów. */
export interface Resolved {
  agent: Agent;
  /** Posortowane. Puste, kiedy krok niczego nie zmienił. */
  changed: OverridableField[];
}

export function resolve(_agent: Agent, _overrides: Overrides): Resolved {
  throw new Error('not implemented');
}

/** Formularz pokazuje wartości efektywne; przy zapisie zostaje z nich sama różnica. */
export function capture(_agent: Agent, _edited: Agent): Overrides {
  throw new Error('not implemented');
}

/** Jedna zmiana z panelu, wyrażona wartością EFEKTYWNĄ, zapisana jako różnica.
 *
 * Oddaje NOWY krok. Ani `step`, ani `agent` nie są mutowane — mutacja `agent` jest dokładnie tym
 * błędem, o którym mówi nagłówek, tylko o jedno wywołanie wcześniej. */
export function applyPanelEdit(_step: AgentStep, _agent: Agent, _edit: Overrides): AgentStep {
  throw new Error('not implemented');
}

/** `Reset` przy jednym wierszu: kasuje JEDEN klucz patcha i zostawia resztę.
 *
 * Osobna funkcja od „Use agent's settings", które opróżnia patch w całości — dwie różne
 * kontrolki w makiecie i dwa różne zdania w słowniku [T4 §4.5]. */
export function withoutOverride(_step: AgentStep, _field: OverridableField): AgentStep {
  throw new Error('not implemented');
}
