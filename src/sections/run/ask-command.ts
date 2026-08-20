/* `/ask <agent> <zadanie>` — jeden agent i jedno zdanie, bez zakładania workflow.
 *
 * PO CO TO ISTNIEJE. Zamówienie właściciela 2026-08-20: „odpalać nasze workflows/agents".
 * Workflow ma drogę z wiersza wejścia (`/run`), agent nie ma żadnej — bo jednostką pracy jest
 * PLIK. Żeby puścić jednego agenta z jednym zdaniem, człowiek musi wejść do edytora, założyć
 * workflow, postawić jeden kafelek, zapisać go i wrócić. To jest cena płacona za najczęstszą
 * czynność dnia, a płaci się ją za każdym razem.
 *
 * DLACZEGO OBOK `run-command.ts`, A NIE W NIM. Bo to są dwa różne rozbiory jednej linii i tylko
 * jedna z tych komend ma w ogóle listę workflow: `/run` tłumaczy pierwsze słowo na plik, `/ask`
 * tłumaczy je na agenta z biblioteki. Wspólna funkcja z flagą „czego szukamy" miałaby dwa
 * znaczenia w jednym miejscu — a wspólne jest to, co naprawdę wspólne: kształt nazwy do wpisania
 * (`typable`) i kształt podpowiedzi (`Named`), oba brane stamtąd, nie przepisane tutaj.
 *
 * DLACZEGO ROZBIÓR JEST CZYSTĄ FUNKCJĄ. To repo nie ma jsdom, więc naciśnięcia Enter nie da się
 * odpalić w kryterium. Polityka zamknięta w komponencie byłaby kodem, którego nic nie sądzi —
 * ta sama rodzina, z której wzięło się siedemnaście kłamiących kontrolek.
 *
 * CZEGO TU NIE MA: DRUGIEJ POLITYKI STARTU. `startAskFromLine` robi to samo, co `launchRun`
 * dla workflow — pyta o zakres, zakłada kartę, czyta limit z `./limits/chosen` — i bierze te
 * odpowiedzi z tych samych modułów. Gdyby liczyła limit po swojemu, cicho nadpisywałaby to, co
 * człowiek przed chwilą ustawił suwakiem (niezmiennik 13 w najgorszym miejscu: w argumencie
 * decydującym, ilu agentów naprawdę ruszy).
 */
import { why } from '../../ipc/why';
import type { Agent } from '../../state/agents';
import { activeWorkspace } from '../../state/workspaces';
import { list } from '../agents/io';
import { ask } from './io';
import { NO_FOLDER } from './launch';
import { atOnce } from './limits/chosen';
import type { Named } from './run-command';
import { typable } from './run-command';
import { cardForRun } from './tabs/store';

/**
 * Co `/ask` z tej linii znaczy: albo para (agent, zadanie), albo zdanie odmowy.
 *
 * AGENT, NIE JEGO NAZWA: po tamtej stronie granicy `run_agent` bierze IDENTYFIKATOR, bo ten
 * przeżywa zmianę nazwy [T3 §3.1]. Wiersz wejścia tłumaczy więc wpisane słowo na definicję,
 * zanim cokolwiek pojedzie na drut, i robi to w jednym miejscu.
 */
export type AskLine =
  { readonly agent: Agent; readonly task: string } | { readonly refusal: string };

/**
 * Nazwy agentów do podpowiedzenia po `/ask ` — w postaci DO WPISANIA.
 *
 * Kształt `Named` jest ten sam, co przy workflow, i to nie jest oszczędność: lista pod polem
 * rysuje się jednym kodem, więc dwa rodzaje podpowiedzi nie mają jak rozjechać się wyglądem.
 * Różnica jest wyłącznie w tym, skąd pochodzą wiersze.
 */
export function agentNames(agents: readonly Agent[]): readonly Named[] {
  return agents.map((one) => ({
    name: typable(one.name),
    /* Prawdziwa nazwa i zdanie o agencie: nazwa do wpisania bywa nie do poznania
     * („note-taker" wobec „Note taker"), a `summary` jest jedyną rzeczą, która na tej liście
     * odróżnia jednego agenta od drugiego. Bez niego zostaje sama nazwa — nagłówek nad pustką
     * jest gorszy niż jego brak (DESIGN §6). */
    does: one.summary.trim() === '' ? one.name : one.name + ' — ' + one.summary.trim(),
  }));
}

/**
 * Co znaczy to, co człowiek dopisał po `/ask`.
 *
 * PIERWSZE SŁOWO JEST AGENTEM, RESZTA JEST ZADANIEM. Reszta idzie dalej CO DO ZNAKU — zdanie
 * dla agenta jest tekstem człowieka, a nie listą słów: rozbiór, który sklei wielokrotne spacje,
 * przepisuje polecenie, za które ktoś zaraz zapłaci turą.
 */
export function readAskLine(agents: readonly Agent[], rest: string): AskLine {
  if (agents.length === 0) return { refusal: NOBODY_SAVED };

  const words = rest.trim();
  /* PUSTE `/ask` NIE MA DOMYŚLNEGO AGENTA, i to jest różnica wobec `/run`, która ma nazwany
   * powód. Tam domyślny wybór jest ten sam, co pod przyciskiem Start (`firstRunnable`), więc
   * człowiek dostaje to, co widzi na ekranie. Tutaj żadna kontrolka nie wskazuje „tego jednego
   * agenta", więc wybór za człowieka byłby wybraniem kogoś na chybił trafił — i to na jego
   * rachunek. */
  if (words === '') return { refusal: whichAgent(agents) };

  const split = words.indexOf(' ');
  const head = split === -1 ? words : words.slice(0, split);
  /* CO DO ZNAKU, tylko bez spacji, które oddzielają nazwę od zdania: `trimStart` zdejmuje
   * przerwę po nazwie, a wszystko dalej zostaje takie, jak je wpisał człowiek. `trim()` na
   * całości — jak w `readRunLine` — sklejałby tu tylko końce; ale nawet to jest o jedną
   * zmianę za dużo, kiedy zdanie jedzie prosto w prompt, za który ktoś zapłaci turą. */
  const tail = split === -1 ? '' : words.slice(split).trimStart();

  const wanted = typable(head);
  const agent = agents.find((one) => typable(one.name) === wanted);
  if (agent === undefined) return { refusal: noSuchAgent(head, agents) };
  if (tail === '') return { refusal: nothingToDo(agent) };
  return { agent, task: tail };
}

/** Co powiedzieć, kiedy w bibliotece nie ma ani jednego agenta. */
export const NOBODY_SAVED =
  'Nobody to ask yet: there is no agent saved. Open Agents and create one first.';

/**
 * Co powiedzieć, kiedy po `/ask` nie stoi nic.
 *
 * WYMIENIA NAZWY, bo pytanie „którego" bez listy jest zagadką: lista powstaje z plików
 * w bibliotece, więc nie ma jej jak zgadnąć (DESIGN §8).
 */
export function whichAgent(agents: readonly Agent[]): string {
  return 'Say which agent, then what it should do. These are the ones you have: ' + names(agents);
}

/** Co powiedzieć, kiedy pierwsze słowo nie jest nazwą żadnego agenta. */
export function noSuchAgent(typed: string, agents: readonly Agent[]): string {
  /* Ta sama treść i ten sam kształt, co przy `/run` (`run-command.ts`, `noSuchWorkflow`):
   * „Unknown agent" zostawia człowieka dokładnie tam, gdzie był. W postaci DO WPISANIA, nie
   * w tej z pliku — lista, z której nie da się przepisać, jest ozdobą. */
  return 'There is no agent called "' + typed + '". These are the ones you have: ' + names(agents);
}

/**
 * Co powiedzieć, kiedy agent jest wskazany, a nie powiedziano mu, co robić.
 *
 * OSOBNE ZDANIE OD [`noSuchAgent`], i to nie jest kosmetyka: te dwa problemy naprawia się
 * dwoma różnymi ruchami (DESIGN §8). Jedno zdanie na oba znaczy, że człowiek, który zapomniał
 * zadania, dostaje listę nazw, a człowiek z literówką w nazwie dostaje radę, żeby dopisać
 * zadanie — czyli obaj dostają odpowiedź na cudze pytanie.
 */
export function nothingToDo(agent: Agent): string {
  return (
    'Nothing was asked: write what ' +
    agent.name +
    ' should do after the name, like "/ask ' +
    typable(agent.name) +
    ' read the notes and say what is missing".'
  );
}

/** Nazwy do wpisania, przecinkami — jeden kształt listy na wszystkie trzy odmowy. */
function names(agents: readonly Agent[]): string {
  return agents.map((one) => typable(one.name)).join(', ') + '.';
}

/**
 * Uruchamia jednego agenta z wiersza wejścia i oddaje zdanie na ekran — albo `null`, gdy poszło.
 *
 * # Dlaczego to stoi obok rozbioru, a nie w komponencie
 *
 * Bo to jest polityka startu, a ona ma jedno miejsce (niezmiennik 23) i musi dać się osądzić
 * bez okna: to repo nie ma jsdom, więc naciśnięcia Enter nie da się odpalić w kryterium.
 * Ta funkcja robi dokładnie to, co `launchRun` dla workflow, i bierze każdą swoją odpowiedź
 * z tego samego modułu, z którego bierze ją tamta droga — zakres z `activeWorkspace`, limit
 * z `./limits/chosen`, kartę z `./tabs/store`.
 *
 * BIBLIOTEKĘ CZYTAMY TERAZ, przy naciśnięciu, a nie z listy zapamiętanej przy renderze: plik
 * jest prawdą (niezmiennik 4), a człowiek mógł zapisać agenta w sekcji Agenci sekundę temu.
 */
export async function startAskFromLine(rest: string): Promise<string | null> {
  let saved: readonly Agent[];
  try {
    saved = await list();
  } catch (error: unknown) {
    return why(error, 'Loadout could not read your agents.');
  }

  const read = readAskLine(saved, rest);
  if ('refusal' in read) return read.refusal;

  /* ZAKRES CZYTANY W CHWILI NACIŚNIĘCIA, nie zapamiętany: człowiek mógł go przełączyć między
   * jednym zdaniem a drugim, a agent ma pracować tam, gdzie stoi teraz. Odmowa jest tym samym
   * zdaniem, co przy Starcie — jeden brak, jedna odpowiedź (niezmiennik 13). */
  const folder = activeWorkspace()?.folder ?? null;
  if (folder === null) return NO_FOLDER;

  // Karta powstaje PRZED biegiem, tak jak przy Starcie: `ask` rozwiązuje się dopiero z końcem
  // biegu, więc karta założona po nim pojawiałaby się w chwili, w której bieg właśnie zszedł.
  cardForRun(read.agent.name, folder);
  try {
    /* LIMIT Z MODUŁU, nie ze stałej: bieg jednokrokowy bierze miejsce z TEJ SAMEJ puli, co bieg
     * z pliku (niezmiennik 11). Cicha porażka wyglądałaby jak wygoda („to tylko jeden agent")
     * i znaczyłaby, że człowiek ustawia trzech, a pracuje piątka. */
    await ask(read.agent, read.task, atOnce(), folder);
    return null;
  } catch (error: unknown) {
    return why(error, 'Loadout could not start that agent.');
  }
}
