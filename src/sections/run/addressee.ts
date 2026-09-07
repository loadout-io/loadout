/* Do kogo idzie zdanie bez ukośnika — i to jest ZMIANA POLITYKI, nie porządki.
 *
 * CO BYŁO. `sayIt` w `./index.tsx` rozstrzygało jednym warunkiem: ktoś pracuje → zdanie idzie
 * do NIEGO, nikt nie pracuje → do lidera. Skutek jest tą wadą, którą zgłosił właściciel
 * 2026-08-20: „proza w trakcie biegu znika z rozmowy z liderem, bo leci do pracującego agenta".
 * Lider znikał dokładnie wtedy, kiedy jest najbardziej potrzebny — w środku biegu, kiedy człowiek
 * chce zapytać, co się właściwie dzieje, i nie chce tego pytania wysyłać agentowi, który pisze kod.
 *
 * CO JEST. Zdanie bez ukośnika idzie do lidera ZAWSZE, a do agenta wyłącznie wtedy, gdy człowiek
 * zaadresował je jego nazwą na początku linii. Konwencja już istnieje i nie jest wymyślona tutaj:
 * tak każe adresować Rust, kiedy pracuje kilku (`RunError::SeveralAreWorking`), więc to samo
 * słowo znaczy to samo po obu stronach granicy.
 *
 * DOPASOWANIE NA CAŁEJ NAZWIE, uciętej granicą słowa, nigdy na prefiksie. `Plan` nie adresuje
 * kroku `Planner`: adres zdejmowany z treści zmienia zdanie, które pojedzie dalej, więc pomyłka
 * nie jest tu „wysłaniem do złego agenta" — jest wysłaniem do złego agenta ZDANIA, z którego
 * zniknął kawałek. Nazwa bywa WIELOWYRAZOWA (krok z kopiami nazywa się „Builder (1 of 3)"),
 * więc adresem jest cała nazwa, a nie pierwsze słowo linii — powód stoi przy `addressStanding`.
 *
 * DLACZEGO CZYSTY MODUŁ OBOK EKRANU. To repo nie ma jsdom, więc Enter jest dla kryterium
 * nieosiągalny — polityka zamknięta w `sayIt` byłaby kodem, którego nie umie dotknąć żadne
 * kryterium (ten sam powód stoi przy `./run-command.ts`). Ekran przewozi tekst i woła krawędź.
 */

/**
 * Komu doręczyć to zdanie i z jaką treścią.
 *
 * Zamknięty kształt dwóch wariantów, a nie `agent: string | null`: `null` znaczy już coś innego
 * o jedną warstwę niżej — `sayToAgent(text, null)` mówi „ten jeden, który pracuje" (`./io.ts`).
 * Dwa różne `null` w jednej ścieżce to gałąź, w którą wchodzi się przez pomyłkę.
 */
import type { StepSession } from '../../ipc/types';

export type Addressee =
  | { readonly to: 'lead'; readonly text: string }
  | { readonly to: 'agent'; readonly agent: string; readonly text: string };

/**
 * Adresat zdania bez ukośnika.
 *
 * @param typed cała linia, jak ją napisał człowiek.
 * @param working nazwy, POD KTÓRYMI KTOŚ NAPRAWDĘ SŁUCHA — czyli nazwy sesji, które bieg
 *   otworzył (`StepSession.agent`), a nie kafelki z planu. Kafelek bez otwartego kanału nie jest
 *   adresem: jego nazwa na początku linii jest wtedy zwykłym słowem i jedzie do lidera razem
 *   z resztą zdania, bo zdanie wysłane komuś, kto nie słucha, przepada bez śladu.
 */
export function addresseeOf(typed: string, working: readonly string[]): Addressee {
  const text = typed.trim();
  const addressed = addressStanding(text, working);

  /* NAZWA BEZ KANAŁU NIE JEST ADRESEM. Jest wtedy zwykłym słowem i jedzie do lidera RAZEM
   * z resztą zdania: zdjęcie jej po drodze zmieniłoby zdanie, które człowiek napisał, i nic
   * na ekranie by o tym nie powiedziało. */
  if (addressed === undefined) return { to: 'lead', text };

  return {
    to: 'agent',
    agent: addressed,
    /* Adres SCHODZI z treści: krok, do którego dojdzie „Forge use tabs", jest adresowany
     * własną nazwą — czyta się to jak ktoś, kto cytuje mu ją z powrotem. */
    text: text.slice(addressed.length).trimStart(),
  };
}

/**
 * Najdłuższa nazwa STOJĄCA NA POCZĄTKU linii — albo `undefined`, kiedy żadna tam nie stoi.
 *
 * CAŁA NAZWA, NIE PIERWSZE SŁOWO, i to jest naprawa wady, którą widać przy kroku z kopiami.
 * Bieg rejestruje sesję pod nazwą z sufiksem („Builder (1 of 3)") i dokładnie taką nazwę wiersz
 * pod polem każe postawić na początku linii (`./entry/entry.tsx`, `whereItGoes`). Parser czytający
 * pierwszy token widział „Builder", żadna sesja tak się nie nazywała i człowiek dostawał odmowę
 * o niedostępnym kanale — ekran reklamował jako jedyny adres ten, który jako jedyny NIE działał.
 *
 * GRANICA SŁOWA Z OBU STRON, bo gołe porównanie prefiksu wysłałoby zdanie o `Plannerze` do kroku
 * `Plan` ZE ZDJĘTYM kawałkiem pierwszego słowa: zły czytelnik i zdanie, które nie mówi już tego,
 * co mówiło. Kiedy stoją tam dwie pasujące nazwy, wygrywa DŁUŻSZA — adres bardziej szczegółowy
 * jest tym, który człowiek przepisał z ekranu.
 */
function addressStanding(text: string, names: readonly string[]): string | undefined {
  let found: string | undefined;
  for (const name of names) {
    if (name === '' || !text.startsWith(name)) continue;
    const after = text.slice(name.length);
    if (after !== '' && !/^\s/.test(after)) continue;
    if (found === undefined || name.length > found.length) found = name;
  }
  return found;
}

export type SessionAddressee =
  | { readonly to: 'lead'; readonly text: string }
  | { readonly to: 'agent'; readonly text: string; readonly target: StepSession }
  | { readonly to: 'refused'; readonly said: string };

/**
 * WF-08: jawny adres nigdy nie staje się po odmowie prozą do Leada.
 *
 * ADRESY BIERZEMY Z REJESTRU SESJI, NIE Z PLANU — i to nie jest porządek w argumentach, tylko
 * naprawa wady, która odwracała rozstrzygnięcie właściciela z 2026-08-20. Wołający podawał tu
 * nazwy WSZYSTKICH kafelków planu, więc zdanie zaczynające się nazwą kroku, który nigdy nie
 * otworzył kanału, nie szło do lidera: schodziło na ścieżkę sesji i wracało odmową „That step
 * has not opened a message channel in this run.". Najgorzej było, kiedy nic nie biegło — koniec
 * biegu zeruje sesje, ale ZOSTAWIA kroki, a okno odtwarza plan ostatniego biegu przy każdym
 * otwarciu, więc kafelki nieżyjącego biegu przechwytywały prozę, podczas gdy wiersz pod polem
 * obiecywał w tej samej chwili lidera.
 *
 * REJESTR, A NIE LISTA SŁUCHAJĄCYCH: sesja SKOŃCZONA dalej jest adresem, żeby zdanie do niej
 * dostało uczciwe `recipientFinished` z Rusta, zamiast po cichu pojechać do lidera.
 */
export function sessionAddresseeOf(
  typed: string,
  sessions: readonly StepSession[],
): SessionAddressee {
  const text = typed.trim();
  const first = text.split(/\s+/)[0] ?? '';
  if (first.startsWith('@'))
    return sessionCalled(first.slice(1), text.slice(first.length).trimStart(), sessions);
  const byName = addresseeOf(
    text,
    sessions.map((one) => one.agent),
  );
  if (byName.to === 'lead') return byName;
  return sessionCalled(byName.agent, byName.text, sessions);
}

/** Która sesja nosi ten adres — i co powiedzieć, kiedy żadna albo kilka. */
function sessionCalled(
  addressed: string,
  rest: string,
  sessions: readonly StepSession[],
): SessionAddressee {
  const direct = sessions.find((session) => session.nodeKey === addressed);
  const named = sessions.filter((session) => session.agent === addressed && !session.finished);
  if (named.length > 1 && direct === undefined)
    return {
      to: 'refused',
      said:
        'More than one agent uses this name. Address one with ' +
        named.map((one) => '@' + one.nodeKey).join(' or ') +
        '.',
    };
  /* Sesja skończona jest ostatnią deską: nazwa dalej jest adresem, a zdanie o zakończonym
   * odbiorcy wypisuje Rust, który jeden wie, czy tura zdążyła się domknąć. */
  const target = direct ?? named[0] ?? sessions.find((session) => session.agent === addressed);
  if (target === undefined)
    return { to: 'refused', said: 'That step has not opened a message channel in this run.' };
  return { to: 'agent', target, text: rest };
}
