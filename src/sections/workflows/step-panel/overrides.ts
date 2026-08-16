/* Dziedziczenie, nie kopia: krok trzyma tylko RÓŻNICĘ wobec agenta.
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

/** Czy te dwie wartości pola agenta znaczą to samo.
 *
 * Rekurencyjnie, bo trzy pola są listami (`skills`, `connections`) albo obiektem (`tools`),
 * a `['a'] === ['a']` jest fałszem. Bez tego `capture` zgłaszałby zmianę przy każdym otwarciu
 * panelu i „N changed" rosłoby od samego patrzenia. */
function same(one: unknown, other: unknown): boolean {
  if (one === other) return true;

  if (Array.isArray(one) && Array.isArray(other)) {
    return one.length === other.length && one.every((item, at) => same(item, other[at]));
  }

  if (typeof one === 'object' && one !== null && typeof other === 'object' && other !== null) {
    const mine = Object.keys(one);
    const theirs = other as Record<string, unknown>;
    return (
      mine.length === Object.keys(theirs).length &&
      mine.every((key) => same((one as Record<string, unknown>)[key], theirs[key]))
    );
  }

  return false;
}

/** Jeden klucz patcha, wpisany z zachowaniem związku między nazwą pola a typem wartości.
 *
 * Bez generyka `patch[field] = agent[field]` jest dla kompilatora sumą dziewięciu typów po
 * lewej i sumą dziewięciu po prawej — czyli zgodą na wpisanie liczby minut do `model`.
 * Tu `F` wiąże obie strony i ta pomyłka przestaje się kompilować. */
function put<F extends OverridableField>(patch: Overrides, field: F, value: Agent[F]): void {
  patch[field] = value;
}

export function resolve(agent: Agent, overrides: Overrides): Resolved {
  /* Płaskie złożenie wystarcza, choć RFC 7396 schodzi w głąb obiektów: żadne z dziewięciu pól
   * nie jest obiektem, w którym scalanie po kluczach dałoby inny wynik niż podmiana. `tools`
   * jest najbliżej — i tam też podmiana jest tym, czego chce użytkownik: lista narzędzi z kroku
   * ZASTĘPUJE listę agenta, a nie dokłada się do niej [T4 §4.3, „array replacement"]. */
  const merged: Agent = { ...agent };
  const changed: OverridableField[] = [];

  for (const field of OVERRIDABLE) {
    const value = overrides[field];
    if (value === undefined) continue;
    put(merged, field, value as Agent[typeof field]);
    changed.push(field);
  }

  /* Posortowane, bo „N changed" i kolejność szarych wierszy mają być takie same przy każdym
   * otwarciu panelu — kolejność kluczy w obiekcie jest kolejnością, w jakiej ktoś je kiedyś
   * wpisał, i przy ponownym zapisie potrafi się zmienić. */
  changed.sort();
  return { agent: merged, changed };
}

/** Formularz pokazuje wartości efektywne; przy zapisie zostaje z nich sama różnica. */
export function capture(agent: Agent, edited: Agent): Overrides {
  const patch: Overrides = {};

  /* Pętla po `OVERRIDABLE`, nie po kluczach `edited`: `id`, `name` i `runsWith` nie mają prawa
   * wypłynąć do patcha, choćby się różniły. Krok, który przestawia vendora, unieważnia połowę
   * reszty [T4 §6.4], a krok, który nadpisuje `id`, nazywa innego agenta. */
  for (const field of OVERRIDABLE) {
    if (!same(agent[field], edited[field])) put(patch, field, edited[field]);
  }

  return patch;
}

/** Jedna zmiana z panelu, wyrażona wartością EFEKTYWNĄ, zapisana jako różnica.
 *
 * Oddaje NOWY krok. Ani `step`, ani `agent` nie są mutowane — mutacja `agent` jest dokładnie tym
 * błędem, o którym mówi nagłówek, tylko o jedno wywołanie wcześniej. */
export function applyPanelEdit(step: AgentStep, agent: Agent, edit: Overrides): AgentStep {
  /* Trzy kroki, w tej kolejności i nie inaczej: rozwiń krok do wartości EFEKTYWNYCH, nanieś
   * na nie to, co użytkownik wpisał, i policz różnicę wobec agenta na nowo. Dopisanie `edit`
   * wprost do `step.overrides` dawałoby ten sam wynik w dziewięciu przypadkach na dziesięć
   * i inny w dziesiątym: wpisanie z powrotem wartości agenta zostawiłoby klucz w patchu, więc
   * wiersz zostałby na zawsze „changed", a późniejsza zmiana agenta nigdy by do niego nie
   * dotarła [T4 §7]. */
  const effective = resolve(agent, step.overrides).agent;
  const edited: Agent = { ...effective };
  for (const field of OVERRIDABLE) {
    const value = edit[field];
    if (value !== undefined) put(edited, field, value as Agent[typeof field]);
  }

  return { ...step, overrides: capture(agent, edited) };
}

/** `Reset` przy jednym wierszu: kasuje JEDEN klucz patcha i zostawia resztę.
 *
 * Osobna funkcja od „Use agent's settings", które opróżnia patch w całości — dwie różne
 * kontrolki w makiecie i dwa różne zdania w słowniku [T4 §4.5]. */
export function withoutOverride(step: AgentStep, field: OverridableField): AgentStep {
  const overrides: Overrides = { ...step.overrides };
  /* `delete`, nie `= undefined`: klucz z wartością `undefined` znika przy zapisie do JSON-a,
   * ale do tego czasu `field in overrides` jest prawdą, a `Object.keys().length` liczy go
   * do „N changed". Dwie odpowiedzi na to samo pytanie w jednym obiekcie. */
  delete overrides[field];
  return { ...step, overrides };
}
