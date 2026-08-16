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

/**
 * Pięć przygaszonych kolorów tożsamości — i pięć znaczy pięć.
 *
 * Przy szóstym agencie kolory zaczynają się powtarzać i tak ma być: kwadrat nie jest
 * identyfikatorem, tylko pomocą dla oka. Nazwa stoi obok niego na tym samym kafelku, więc
 * dwaj agenci w tym samym kolorze są nadal rozróżnialni; szósty przygaszony kolor byłby
 * nieodróżnialny od sąsiednich, a szósty NASYCONY jest tym błędem, przez który cała ta
 * reguła powstała.
 *
 * `as const`, bo tylko krotka daje `PALETTE[0]` typ pewny zamiast `string | undefined`
 * (`noUncheckedIndexedAccess`), a bez tego wybór niżej musiałby mieć gałąź awaryjną
 * z literałem przepisanym obok tej listy — czyli drugie miejsce, w którym stoi nazwa koloru.
 */
const PALETTE = [
  '--color-id-1',
  '--color-id-2',
  '--color-id-3',
  '--color-id-4',
  '--color-id-5',
] as const;

/** Ta sama paleta jako zwykła lista — do sprawdzenia „czy ta nazwa jest tożsamością". */
export const IDENTITY: readonly string[] = PALETTE;

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
 * Stan → nazwa tokenu. `Record`, nie `switch` z gałęzią domyślną, i to jest cała totalność:
 * siódmy stan dopisany do `AgentStatus` przestaje TU się kompilować, zamiast po cichu wpaść
 * do gałęzi „reszta" i dostać kolor, którego nikt mu nie przydzielił.
 *
 * Trzy stany wychodzą na `--color-muted` i żaden z nich nie jest tam z lenistwa:
 *   `done`     rzecz skończona jest cicha — zielony znaczy „dzieje się teraz" [DESIGN §3],
 *   `stopped`  agent, którego krok odwołano, też się już nie dzieje,
 *   `waiting`  czeka na innego agenta, nie na CIEBIE. `--color-attend` odpowiada na jedno
 *              pytanie — „co czeka na moją decyzję" — a agent czekający na kolegę nie jest
 *              odpowiedzią na nie. Pomarańczowy przy każdym bezczynnym agencie to dokładnie
 *              ten sposób, w jaki kolor przestaje cokolwiek znaczyć.
 */
const OF_STATUS: Readonly<Record<AgentStatus, string>> = {
  working: '--color-accent',
  waiting: '--color-muted',
  'needs you': '--color-attend',
  failed: '--color-fail',
  done: '--color-muted',
  stopped: '--color-muted',
};

/**
 * Odcisk podpisu agenta — FNV-1a, 32 bity.
 *
 * `Math.imul`, bo zwykłe `*` wychodzi poza zakres, w którym `number` trzyma liczby całkowite
 * dokładnie, i po kilku znakach odcisk przestaje być odciskiem.
 */
function fingerprint(agent: string): number {
  let hash = 2_166_136_261;
  for (let at = 0; at !== agent.length; at += 1) {
    hash ^= agent.charCodeAt(at);
    hash = Math.imul(hash, 16_777_619);
  }
  return hash >>> 0;
}

/**
 * Kolor tożsamości tego agenta — nazwa tokenu.
 *
 * Przydział musi być STABILNY: ten sam agent dostaje ten sam kolor niezależnie od tego,
 * w jakiej kolejności podano listę i czy ktoś do niej dołączył. Przydział liczony z pozycji
 * w tablicy wygląda poprawnie na pierwszym zrzucie ekranu i przemalowuje połowę listy
 * w chwili, w której pod-agent wejdzie do biegu w środku.
 *
 * Stąd odcisk podpisu zamiast licznika: ta funkcja nie wie, że lista agentów istnieje, więc
 * nie ma jak od niej zależeć — ani od jej kolejności, ani od jej długości.
 */
export function identityToken(agent: string): string {
  const slot = fingerprint(agent) % PALETTE.length;
  /* `?? PALETTE[0]` jest nieosiągalne — `slot` jest resztą z dzielenia przez długość tej
   * samej krotki — i stoi tu tylko dlatego, że `noUncheckedIndexedAccess` nie umie tego
   * udowodnić. Ważne jest, DOKĄD prowadzi: z powrotem do palety tożsamości. Gałąź awaryjna
   * sięgająca po cokolwiek spoza niej byłaby tym samym błędem, co złe zawijanie. */
  return PALETTE[slot] ?? PALETTE[0];
}

/** Kolor stanu — nazwa tokenu. Totalny na sześciu stanach, obraz ma cztery elementy. */
export function statusToken(status: AgentStatus): string {
  return OF_STATUS[status];
}
