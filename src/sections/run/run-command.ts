/* `/run <workflow> <co zbudować>` — jedna droga z wiersza wejścia do biegu.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-19: „ten terminal nie ma sensu teraz xD, no bo
 * jak ja mam np puścić jakieś workflow i przekazać prompta?". Miał rację dwa razy. Wiersz wejścia
 * rozumiał `/open` i `/stop`, czyli dwie czynności pomocnicze, a makieta obiecuje w tym samym
 * miejscu `/plan · /run · or just say what you want` — więc kontrolka, która wygląda na główny
 * sposób pracy, nie umiała uruchomić pracy. Drugi raz miał rację co do PROMPTA: bieg brał dotąd
 * wyłącznie to, co stało w pliku, więc sześciu agentów ustawionych raz umiało zbudować dokładnie
 * jedną rzecz — tę, którą ktoś wcześniej wpisał w `instructions` każdego kroku. Kształt pracy
 * i treść pracy to dwie różne rzeczy, a plik trzymał je zlepione.
 *
 * DLACZEGO NIE W `entry.tsx`. Bo to jest polityka startu, a ona ma jedno miejsce: `./launch`
 * (niezmiennik 23). Ten plik nie decyduje ani ile agentów naraz, ani w jakim folderze — bierze
 * limit z `./limits/chosen`, czyli z tego samego modułu, z którego czyta go suwak obok Startu,
 * i oddaje decyzję `launchRun`. Gdyby `/run` liczyło limit po swojemu, cicho nadpisywałoby to,
 * co człowiek przed chwilą ustawił suwakiem — a to jest ta wersja złamania niezmiennika 13,
 * która nie zostawia śladu w żadnym diffie: liczba jest wczytywana, logowana i inna.
 *
 * DLACZEGO ROZBIÓR JEST OSOBNĄ, CZYSTĄ FUNKCJĄ. To repo nie ma jsdom, więc naciśnięcia Enter nie
 * da się odpalić w teście. Polityka zamknięta w komponencie byłaby kodem, którego żadne kryterium
 * nie umie dotknąć — czyli tą samą rodziną, z której wzięło się siedemnaście kłamiących kontrolek.
 * [`readRunLine`] jest funkcją od napisu do decyzji i sądzi się bez okna.
 */
import { atOnce as atOnceNow } from './limits/chosen';
import type { Choice } from './choices';
import { firstRunnable, toChoices } from './choices';
import { launchRun } from './launch';
import { list } from '../workflows/io';
import { why } from '../../ipc/why';

/**
 * Nazwa workflow do wpisania z klawiatury.
 *
 * Workflow nazywa sam siebie zdaniem („Three loose steps"), a wiersz wejścia przyjmuje SŁOWA —
 * więc nazwa musi mieć postać, którą da się wpisać jednym tokenem i przewidzieć bez patrzenia
 * na listę. Małe litery i łącznik zamiast wszystkiego, co nie jest literą ani cyfrą.
 *
 * Ta sama funkcja liczy klucz dla nazwy pliku, żeby `three-loose-steps.json`, `Three loose steps`
 * i `three-loose-steps` prowadziły do jednego workflow, a nie do trzech odpowiedzi na jedno
 * pytanie.
 */
export function typable(name: string): string {
  return name
    .replace(/\.json$/i, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/**
 * Workflow, które da się podpowiedzieć pod `/run` — nazwa do wpisania plus zdanie o nim.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-19: „powinno podpowiadać jakie workflow, tam
 * podpowiadajka powinna być". I makieta obiecuje dokładnie to w drugiej linii wiersza wejścia:
 * „Tab completes a workflow or a repo Loadout has seen". Komenda, która wymaga nazwy, a nie mówi
 * jakiej, jest zagadką — a listy nazw nie ma jak zgadnąć, bo powstaje z plików na dysku.
 *
 * Kształt jest TEN SAM, co wpis w `KNOWN` w wierszu wejścia (nazwa plus `does`), żeby lista pod
 * polem rysowała się jednym kodem. Dwa renderery dla dwóch rodzajów podpowiedzi rozjechałyby się
 * pierwszego dnia, a różnica jest wyłącznie w tym, skąd pochodzą wiersze.
 */
export interface Named {
  readonly name: string;
  readonly does: string;
}

/**
 * Nazwy workflow do podpowiedzenia — tylko te, które da się uruchomić.
 *
 * BEZ WORKFLOW BEZ KROKÓW, i to jest ta sama reguła, co przy domyślnym wyborze: podpowiedzieć
 * nazwę, która po Enterze odmówi („There are no steps yet."), znaczy zaprosić człowieka do
 * odmowy. Świeży szkic pojawi się na tej liście w chwili, w której dostanie pierwszy krok.
 */
export function workflowNames(choices: readonly Choice[]): readonly Named[] {
  return choices
    .filter((choice) => choice.steps.length > 0)
    .map((choice) => ({
      name: typable(choice.name),
      /* Prawdziwa nazwa i liczba kroków: nazwa do wpisania bywa nie do poznania („ship-a-feature"
       * wobec „Ship a feature"), a liczba kroków jest jedyną rzeczą, która na tej liście odróżnia
       * mały workflow od takiego, który uruchomi sześciu agentów i zacznie płacić. */
      does:
        choice.name +
        ' — ' +
        String(choice.steps.length) +
        (choice.steps.length === 1 ? ' step' : ' steps'),
    }));
}

/** Co `/run` z tej linii znaczy: albo bieg z zadaniem, albo zdanie odmowy. */
export type RunLine =
  { readonly go: Choice; readonly task: string | null } | { readonly refusal: string };

/** Co powiedzieć, kiedy nie ma ani jednego workflow z krokami. */
export const NOTHING_SAVED =
  'Nothing to run: there is no workflow with steps in it yet. Open Workflows and build one first.';

/** Co powiedzieć, kiedy pierwsze słowo nie jest nazwą żadnego workflow. */
export function noSuchWorkflow(typed: string, choices: readonly Choice[]): string {
  /* WYMIENIA NAZWY, i to jest cała treść tej odmowy. „Unknown workflow" zostawia człowieka
   * dokładnie tam, gdzie był — a nazwy, których nie widzi, nie ma jak zgadnąć (DESIGN §8).
   * W postaci DO WPISANIA, nie w tej z pliku: lista, z której nie da się przepisać, jest ozdobą. */
  const names = choices.map((choice) => typable(choice.name)).join(', ');
  return 'There is no workflow called "' + typed + '". These are the ones you have: ' + names + '.';
}

/**
 * Co znaczy to, co człowiek dopisał po `/run`.
 *
 * PIERWSZE SŁOWO JEST NAZWĄ, RESZTA JEST ZADANIEM, i ta reguła jest sztywna z rozmysłem.
 * Wersja, która zgaduje („jeśli pierwsze słowo nie pasuje, to całość jest zadaniem"), wysyła
 * literówkę w nazwie workflow jako polecenie dla agentów — czyli uruchamia CUDZY workflow
 * z Twoim promptem i wygląda przy tym na sukces. Odmowa jest tu tańsza od domysłu.
 *
 * Puste `/run` uruchamia to samo, co przycisk Start bez wybierania: pierwszy workflow, który ma
 * kroki (`firstRunnable`, ta sama funkcja). Nie `choices[0]`: lista przychodzi posortowana
 * bajtowo, więc pierwszy bywa świeżym szkicem bez ani jednego kroku, a taki bieg odmawia.
 */
export function readRunLine(choices: readonly Choice[], rest: string): RunLine {
  const words = rest.trim();
  if (words === '') {
    const first = firstRunnable(choices);
    return first === null ? { refusal: NOTHING_SAVED } : { go: first, task: null };
  }

  const split = words.indexOf(' ');
  const head = split === -1 ? words : words.slice(0, split);
  const tail = split === -1 ? '' : words.slice(split + 1).trim();

  const wanted = typable(head);
  const go = choices.find(
    (choice) => typable(choice.name) === wanted || typable(choice.path) === wanted,
  );
  if (go === undefined) return { refusal: noSuchWorkflow(head, choices) };
  /* Puste zadanie jedzie jako `null`, nie jako pusty napis: po tamtej stronie `None` znaczy
   * „prompt kroku co do bajtu", a `Some("")` byłoby zadaniem, które istnieje i nic nie mówi. */
  return { go, task: tail === '' ? null : tail };
}

/**
 * Uruchamia bieg z wiersza wejścia i oddaje zdanie na ekran — albo `null`, kiedy poszło.
 *
 * Katalog czytamy TERAZ, przy naciśnięciu, a nie z listy zapamiętanej przy renderze: plik jest
 * prawdą (niezmiennik 4), a człowiek mógł zapisać workflow w drugim oknie sekcji sekundę temu.
 */
export async function startFromLine(
  rest: string,
  reflectionEnabled = true,
): Promise<string | null> {
  let choices: readonly Choice[];
  try {
    choices = toChoices(await list());
  } catch (error: unknown) {
    return why(error, 'Loadout could not read the workflows folder.');
  }

  const read = readRunLine(choices, rest);
  if ('refusal' in read) return read.refusal;
  /* Limit z modułu, nie z argumentu i nie ze stałej: jedna pula na okno (niezmiennik 11), a jej
   * wartość ustawia suwak obok Startu. Czytany w chwili naciśnięcia Enter, bo to wtedy jest
   * prawdziwy. */
  return launchRun(read.go, atOnceNow(), read.task, null, reflectionEnabled);
}
