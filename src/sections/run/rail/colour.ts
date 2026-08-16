/* Dwie palety, rozłączne z definicji [DESIGN §3 „Tożsamość ≠ stan"].
 *
 * Reguła powstała z konkretnego zdarzenia, nie z estetyki: referencyjny redesign poprzedniego prototypu
 * dał agentowi Forge `#ffb45b`, czyli dokładnie ten hex, który na sąsiednim kafelku znaczył
 * „czeka na twoją decyzję". Nikt nie zgłasza tego jako błędu — ludzie po prostu przestają
 * ufać kolorom, a wtedy cała lista agentów przestaje dawać się skanować wzrokiem.
 *
 * Rozdział jest po NASYCENIU i mieszka w tokenach, nie w tym pliku: tożsamość jest
 * przygaszona (`--color-id-1…5`), stan nasycony (`--color-accent`, `--color-attend`,
 * `--color-fail`, `--color-muted`). Tutaj rozstrzyga się tylko, kto dostaje który — i to
 * jest jedyne miejsce, w którym wolno to rozstrzygnąć.
 *
 * Oba zbiory są zamknięte i oba mają być zamknięte JAKO WARTOŚĆ, nie jako zdanie w
 * komentarzu. Implementacja z błędem zawijania, która przy szóstym agencie sięga po
 * `--color-attend`, robi dokładnie ten błąd, przez który ta reguła w ogóle powstała, i nie
 * ma jak się o tym dowiedzieć, dopóki ktoś nie policzy obrazu funkcji na pełnym zbiorze.
 *
 * `src/styles/theme.css` definiuje wszystkie dziewięć tych nazw i nie należy do tego
 * zadania. Ten plik operuje NAZWAMI, komponent pisze `var(--color-id-3)`; hex w kodzie
 * komponentu jest zakazany [DESIGN §9].
 */
import type { AgentStatus } from './card';

/** Pięć przygaszonych kolorów tożsamości. Szósty agent zawija się na pierwszy. */
export const IDENTITY: readonly string[] = [
  '--color-id-1',
  '--color-id-2',
  '--color-id-3',
  '--color-id-4',
  '--color-id-5',
];

/**
 * Cztery nasycone kolory stanu — cały słownik semantyczny aplikacji [DESIGN §3].
 *
 * `--color-muted` jest tu piątym kolorem, który nie jest piąty: rzecz skończona jest cicha,
 * więc `done` nie dostaje zieleni. Zielony znaczy „dzieje się teraz", nie „udało się".
 */
export const STATUS: readonly string[] = [
  '--color-accent',
  '--color-attend',
  '--color-fail',
  '--color-muted',
];

/**
 * Kolor tożsamości tego agenta — nazwa tokenu.
 *
 * Przydział musi być STABILNY: ten sam agent dostaje ten sam kolor niezależnie od tego,
 * w jakiej kolejności podano listę i czy ktoś do niej dołączył. Przydział liczony z pozycji
 * w tablicy wygląda poprawnie na pierwszym zrzucie ekranu i przemalowuje połowę listy
 * w chwili, w której pod-agent wejdzie do biegu w środku.
 */
export function identityToken(_agent: string): string {
  throw new Error('not implemented');
}

/** Kolor stanu — nazwa tokenu. Totalny na sześciu stanach, obraz ma cztery elementy. */
export function statusToken(_status: AgentStatus): string {
  throw new Error('not implemented');
}
