/* Co import dopisał agentom — z odpowiedzi Rusta, nie z przewidywania tej strony.
 *
 * Obie drogi importu kończą się tym samym zdaniem dla człowieka („Import" w sekcji Agents
 * i „Import setup from project"), więc kształt czyta i zdanie składa JEDEN moduł
 * (niezmiennik 23). Dwa składacze rozjechałyby się przy pierwszej zmianie brzmienia,
 * a rozjazd widać dopiero na dwóch ekranach naraz.
 *
 * `invoke<T>` jest RZUTOWANIEM, nie sprawdzeniem: cokolwiek przyjedzie z drugiej strony,
 * TypeScript uwierzy. Nowe pole w odpowiedzi ma więc własną bramkę kształtu — zła odpowiedź
 * ma tu zostać niczym, a nie wyjątkiem, który zdejmuje cały ekran.
 */

/** Lustro `connections::fill::FilledAgent`. */
export interface FilledAgent {
  agent: string;
  connections: string[];
}

/** Ile nazw agentów mieści się w zdaniu, zanim reszta zostaje liczbą. */
const NAMES = 3;

/** Wpisy o dobrym kształcie z odpowiedzi komendy importu. Cokolwiek innego jest niczym. */
export function filledAgents(value: unknown): FilledAgent[] {
  if (!Array.isArray(value)) return [];
  return value.filter(isFilled).map((one) => ({ agent: one.agent, connections: one.connections }));
}

function isFilled(value: unknown): value is FilledAgent {
  if (typeof value !== 'object' || value === null) return false;
  const one = value as { agent?: unknown; connections?: unknown };
  return (
    typeof one.agent === 'string' &&
    Array.isArray(one.connections) &&
    one.connections.every((name) => typeof name === 'string')
  );
}

/**
 * Jedno zdanie o tym, kto co dostał — albo `null`, kiedy nikt nic nie dostał.
 *
 * `null`, a nie zdanie o zerze: import, który nie rozdał ani jednego połączenia, nie ma o czym
 * mówić, a linia „Gave nothing to nobody." byłaby hałasem na każdym zwykłym imporcie.
 *
 * Przyjmuje `unknown` i sam przepuszcza to przez [`filledAgents`], bo to jest MIEJSCE, w którym
 * tę wartość się czyta — a czyta ją dwoje wołających, z których jedno bierze odpowiedź wprost
 * z `invoke`. Bramka kształtu postawiona przy jednym z nich zostawiałaby drugiemu wyjątek
 * zdejmujący cały ekran.
 */
export function whatTheyGot(filled: unknown): string | null {
  const got = filledAgents(filled).filter((one) => one.connections.length > 0);
  if (got.length === 0) return null;
  /* Każdy dopełniony agent dostaje tę samą listę (nazwy włączonych połączeń biblioteki), ale
   * zdanie składamy z sumy, a nie z pierwszego wpisu: gdyby te listy kiedykolwiek się różniły,
   * zdanie ma mówić o tym, co naprawdę przyjechało. */
  const connections = [...new Set(got.flatMap((one) => one.connections))];
  const names = got.map((one) => one.agent);
  const shown =
    names.length > NAMES
      ? [...names.slice(0, NAMES), `${String(names.length - NAMES)} more`]
      : names;
  return `Gave ${andThen(connections)} to ${andThen(shown)}.`;
}

/** „a", „a and b", „a, b and c" — wyliczenie tak, jak się je wypowiada. */
function andThen(names: readonly string[]): string {
  if (names.length < 2) return names.join('');
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1] ?? ''}`;
}
