/* „Otwórz TEGO agenta" — intencja, którą zapisuje paleta, a wykonać ma ekran Agents.
 *
 * DROGA JEST CAŁA OD 2026-08-31. Do tego dnia stała tu połowa i było to powiedziane wprost:
 * paleta zapisywała prośbę i szła na ekran Agents, a prośby nie odbierał nikt — `askForAgent`
 * był jedynym wołanym eksportem tego modułu, więc człowiek, który wybrał agenta po nazwie,
 * lądował na liście i musiał znaleźć go wzrokiem po raz drugi. Czytelnik stoi dziś
 * w `src/sections/agents/index.tsx` (`useSyncExternalStore` na `subscribeToAsked`/`askedAgent`,
 * potem `takeAskedAgent()` i `open(agent)`), a sądzi go prawdziwe naciśnięcie klawisza
 * w chromium: `e2e/tests/the-keyboard-reaches-every-section.spec.ts`, przypadek „opens the
 * panel of the agent that was picked, not just the shelf it lies on".
 *
 * WZORZEC JEST PRZEPISANY Z `src/sections/run/requested.ts`, ŚWIADOMIE. Tamten moduł powstał
 * po dokładnie tej samej wadzie po drugiej stronie: zielony przycisk `Run` w edytorze workflow
 * robił `go('run')` i wyrzucał ścieżkę pliku, więc ekran przeskakiwał i nic nie startowało.
 * Trzy funkcje i zapadka są tym, czego brakowało tamtej drodze — nie warstwą na zapas.
 *
 * DLACZEGO PRENUMERATA, SKORO EKRAN I TAK SIĘ PRZEMONTUJE. Bo nie zawsze: powłoka trzyma
 * DOKŁADNIE JEDNĄ sekcję (`src/App.tsx`), więc wejście na Agents z innej sekcji montuje ekran
 * od nowa — ale wybór agenta z palety otwartej JUŻ NA Agents nie zmienia sekcji i nie montuje
 * niczego. Bez prenumeraty ta jedna droga byłaby cicho martwa, a to jest ta droga, którą
 * człowiek pójdzie najczęściej.
 */
import { useSectionStore } from '../shell/section-store';

/** O którego agenta poproszono i po raz który. */
export interface AgentRequest {
  /** Identyfikator agenta — ten sam, którym nazywa go biblioteka. */
  readonly id: string;
  /**
   * Numer prośby, ściśle rosnący. Bez niego druga prośba o TEGO SAMEGO agenta jest
   * nieodróżnialna od pierwszej i odbiorca nie ma jak zauważyć, że człowiek poprosił znowu.
   */
  readonly nonce: number;
}

let pending: AgentRequest | null = null;
let nonce = 0;
const listeners = new Set<() => void>();

/**
 * Poproś o otwarcie tego agenta i przejdź na ekran Agents.
 *
 * Przejście jest TUTAJ, a nie u wołającego, z tego samego powodu, co w `requested.ts`: prośba
 * bez przejścia jest prośbą, której nikt nie odbierze, dopóki człowiek sam tam nie wejdzie.
 */
export function askForAgent(id: string): void {
  nonce += 1;
  pending = { id, nonce };
  useSectionStore.getState().go('agents');
  for (const listener of listeners) listener();
}

/** Prośba, która czeka na odebranie, albo `null`. Kształt dla `useSyncExternalStore`. */
export function askedAgent(): AgentRequest | null {
  return pending;
}

/**
 * Zdejmuje prośbę, żeby nie została odebrana drugi raz.
 *
 * Odbiorca woła to, kiedy już otworzył agenta. Prośba zostawiona w module otwierałaby tego
 * samego agenta przy każdym powrocie na ekran Agents — czyli kasowałaby człowiekowi to,
 * co właśnie wpisał w formularzu.
 */
export function takeAskedAgent(): AgentRequest | null {
  const taken = pending;
  pending = null;
  return taken;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToAsked(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
