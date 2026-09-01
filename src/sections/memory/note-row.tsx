/* Wiersz notatki: dwa stany, jeden żywy region na fakt i jedna akcja.
 *
 * NIEZMIENNIK 13 RZĄDZI TYM PLIKIEM. Stan notatki ma dokładnie JEDEN żywy region w wierszu:
 * chip. Etykieta przycisku jest *akcją* („Use this"), nie powtórzonym stanem — chip
 * „Suggested", obok tekst „not in use yet", a na dole licznik „3 suggested" to trzy regiony
 * na jeden fakt i dokładnie to, przez co poprzedni prototyp pokazywał stan połączenia w sześciu
 * miejscach [ARCHITECTURE §7: żywe regiony na jeden fakt = 1].
 *
 * NIEZMIENNIK 14: w tym wierszu istnieją wyłącznie słowa `Suggested`, `In use`, `Use this`,
 * `Stop using`, `Discard`, `Discard for good`, `Keep it` i `length`. Trzeci stan (`candidate`,
 * `confirmed`, `corroborated`, `trusted`, `archived`, `replaced`) i żargon (`promote`, `token`)
 * wchodzą właśnie tędy — z makiety, z enuma z drutu albo z pola, które ktoś wypisał
 * „na wszelki wypadek".
 *
 * ODRZUCENIE PYTA, ZANIM SKASUJE (2026-08-31). „Discard" wygląda jak zdjęcie wiersza z listy,
 * a po tamtej stronie granicy zostawia TRWAŁY nagrobek w `discarded/`: notatka nie wróci nigdy,
 * także wtedy, gdy inny agent nauczy się tego samego zdania. Wiersz stawia więc pytanie
 * w miejscu akcji i mówi tę część, której nie widać. Odpowiedź „o co pytamy" przyjeżdża
 * z magazynu (`askingToDiscard`), nie z `useState` — patrz opis tego propsa.
 *
 * UZASADNIENIE JEST WIDOCZNE, NIE ZA KLIKNIĘCIEM. Człowiek jest jedyną osobą, która może
 * powiedzieć „tak, to jest prawda", a klika w to raz — bez powodu na ekranie klika w ciemno
 * i cała bramka promocji staje się rytuałem [T6 §5.1].
 *
 * DLACZEGO W WIERSZU NIE MA `title`. Tytuł jest drugą nazwą tego samego zdania, a do modelu
 * jedzie `rule` — więc pokazanie obu stawia obok siebie dwa zdania o jednej rzeczy, z których
 * człowiek czyta krótsze, a ocenia dłuższe. Sufit gęstości liczy KAŻDY element niosący tekst
 * [ARCHITECTURE §7: 60], a wiersz mnoży się przez liczbę notatek: piąty element w wierszu
 * kosztuje tu więcej niż piąty element gdziekolwiek indziej.
 *
 * Czysta funkcja propsów na markup, jak `ReviewCard`: bez własnego stanu i bez `invoke()`.
 * Odmowa i wymuszony wybór mieszkają w magazynie (`src/state/memory.ts`), nie tutaj —
 * wyłączony przycisk jest sugestią, a nie mechanizmem.
 */
import type { ReactElement } from 'react';
import type { Note, NoteAddress } from '../../state/memory';
import { EVERY_PROJECT, onlyAgent, THIS_PROJECT } from '../knowledge/reach';

export interface NoteRowProps {
  note: Note;
  /** „Use this". Handler jest wymagany, bo kontrolka bez handlera nie wchodzi do repo
   * (niezmiennik 16) — a wiersz nie zna magazynu i nie ma jak zawołać go sam. */
  onUse: (address: NoteAddress) => void;
  /** „Stop using". Ta sama reguła. */
  onStopUse: (address: NoteAddress) => void;
  /**
   * „Discard" — druga decyzja, którą człowiek może podjąć wobec KANDYDATKI (2026-08-23, T-92).
   *
   * OPCJONALNY, i to nie jest wygoda dla wołających. `note-row.test.tsx` z T-17 leży poza blokiem
   * OWNS tego zadania (`AGENTS.md` §7) i montuje ten wiersz bez tego propsa — wymagany zabrałby
   * cudzemu kryterium kompilację, a kryterium, które przestało się kompilować, niczego nie
   * uruchomiło.
   *
   * Brak handlera znaczy **brak przycisku**, nigdy przycisk, który nic nie robi: kontrolka bez
   * handlera nie wchodzi do repo (niezmiennik 16), a odmowa na kliknięcie jest z zewnątrz
   * nieodróżnialna od zepsutej aplikacji.
   */
  onDiscard?: (address: NoteAddress) => void;
  /**
   * Czy pytanie „na pewno na zawsze?" stoi w TYM wierszu (2026-08-31).
   *
   * Odpowiedź przychodzi z magazynu, nie z prywatnego stanu wiersza: pytanie ma stać przy
   * jednym wierszu naraz (niezmiennik 13), a `useState` w komponencie daje po jednym
   * niezależnym pytaniu na wiersz i nic ich nie zlicza.
   */
  askingToDiscard?: boolean;
  /** Drugie kliknięcie: notatka odchodzi i nie wróci. Bez handlera nie ma tego przycisku. */
  onDiscardForGood?: (address: NoteAddress) => void;
  /** „Keep it" — pytanie znika, notatka zostaje. */
  onKeepIt?: () => void;
  /** Jedyna akcja notatki projektowej, która nadal leży w bibliotece. */
  onMove?: (address: NoteAddress) => void;
}

/**
 * Zdanie, które musi paść, zanim kandydatka odejdzie na zawsze.
 *
 * MÓWI TĘ POŁOWĘ, KTÓREJ NIE WIDAĆ. `discard_note` zostawia trwały nagrobek w `discarded/`
 * (`src-tauri/src/memory/notes.rs`, `was_discarded`) i skan pomija odtąd każdy plik o tym
 * slugu — więc nie chodzi o „znika z listy", tylko o „nie wróci, nawet gdy inny agent nauczy
 * się tego samego". „Are you sure?" nie mówi nic z tego i uczy klikać dalej.
 */
const NOT_COMING_BACK =
  'Discard this note? It will not come back, even if an agent learns it again.';

/**
 * „Length 137" — słowo, którym o tym mówimy [DESIGN §8]; nazwa z drutu tu nie dojeżdża.
 *
 * Eksportowane, bo okno wymuszonego wyboru pisze tę samą liczbę i musi ją pisać tak samo:
 * dwie etykiety tej samej rzeczy rozjeżdżają się przy pierwszej zmianie brzmienia, a człowiek
 * porównuje właśnie te dwie liczby, kiedy wybiera, co odstawić (niezmiennik 23).
 */
export function lengthLabel(length: number): string {
  return 'Length ' + String(length);
}

/* Dokąd wiedza dociera i skąd przyjechała — fakty, które mają po jednym miejscu w wierszu.
 *
 * ETYKIETY POWSTAJĄ WYŁĄCZNIE Z PÓL NOTATKI. Notatka niczyja nie dostaje myślnika ani słowa
 * w rodzaju „unassigned": to jest odpowiedź na pytanie, którego nikt nie zadał, a człowiek czyta
 * ją jako fakt o notatce (niezmiennik 13 — jeden żywy region na fakt, i ani jeden na fakt,
 * którego nie ma). Wiersz, który wypisuje ostatnią nazwę, jaką widział, wygląda poprawnie na
 * notatce jednego agenta i kłamie o każdej innej.
 *
 * SAME NAPISY PRZYCHODZĄ Z `knowledge/reach.ts` i od 2026-08-31 nie stoją tutaj. Umiejętność
 * w tym samym położeniu mówiła o sobie „Everywhere", a ta notatka „Every project" — jeden fakt,
 * dwa brzmienia, na jednym ekranie. Wybór słowa mieszka teraz w jednym pliku, który czytają
 * obie połowy sekcji (niezmiennik 13). */
function reachLabel(note: Note, legacy: boolean): string | null {
  if (legacy) return null;
  if (note.scope === 'everywhere') return EVERY_PROJECT;
  if (note.scope === 'this-project') return THIS_PROJECT;
  return note.agent ? onlyAgent(note.agent) : null;
}

/* Import i refleksja są dwoma pochodzeniami, nie dwiema pisowniami jednego pola. Projekt jest
 * nazwą czytelną dla człowieka, a bieg pozostaje identyfikatorem prowadzącym do jego historii. */
function importedFrom(project: string): string {
  return 'Imported from ' + project;
}

function suggestedAfter(from: string): string {
  return 'Suggested after run ' + from;
}

export function NoteRow({
  note,
  onUse,
  onStopUse,
  onDiscard,
  askingToDiscard,
  onDiscardForGood,
  onKeepIt,
  onMove,
}: NoteRowProps): ReactElement {
  /* Jedno pytanie zadane RAZ. Trzy osobne `note.status === 'suggested'` w trzech gałęziach to
   * trzy miejsca, w których wiersz odpowiada na to samo — i pierwsze, które ktoś zmieni,
   * rozjedzie się z dwoma pozostałymi bez śladu w typach. */
  const waiting = note.status === 'suggested';
  const address: NoteAddress = { place: note.place, id: note.id };
  const legacy = note.place === 'library' && note.scope === 'this-project';
  const reach = reachLabel(note, legacy);
  /* Pytanie rysuje się dopiero wtedy, gdy ma OBIE odpowiedzi. Wołający, który poda samą flagę,
   * dostałby pytanie bez wyjścia — a to jest gorsze niż jego brak (niezmiennik 16). */
  const asking =
    askingToDiscard === true && onDiscardForGood !== undefined && onKeepIt !== undefined;

  return (
    <li
      data-note={note.id}
      data-note-address={`${note.place}:${note.id}`}
      className="stack border-b border-line px-2 py-3"
    >
      <div className="flex items-center gap-2">
        {/* Ton idzie ATRYBUTEM, nie drugą klasą (warstwa prymitywów w `theme.css`): dwa napisy
            na jedną pigułkę trzeba było trzymać zgodnie ręcznie.

            Kolor jest wybrany, nie odziedziczony po makiecie. `--attend` odpowiada na pytanie
            „co czeka na moją decyzję?" [DESIGN §3] i kandydatka jest dokładnie tym. Notatka
            w użyciu nie chce niczego, więc zostaje przy wariancie neutralnym: gdyby i ona była
            nasycona, kolor przestałby znaczyć „twoja kolej". `--accent` odpada osobno — znaczy
            „teraz", a notatka niczego nie robi. */}
        {legacy ? null : (
          <span data-state className="chip" data-tone={waiting ? 'attend' : undefined}>
            {waiting ? 'Suggested' : 'In use'}
          </span>
        )}
        <span className="label">{lengthLabel(note.length)}</span>
        {/* Zasięg wynika ze scope, a nazwa agenta doprecyzowuje wyłącznie `this-agent`.
            Biblioteczne legacy nie udaje „This project" przed jawnym Move. */}
        {reach ? <span className="label">{reach}</span> : null}
        {note.project ? <span className="label">{importedFrom(note.project)}</span> : null}
        {note.from ? <span className="label">{suggestedAfter(note.from)}</span> : null}
      </div>

      {/* Zdanie, które naprawdę jedzie do modelu — nie streszczenie tego zdania.

          `text-body` stało tu obok `text-ink` i było bez skutku: w tym motywie `--color-body`
          i `--text-body` noszą tę samą nazwę, a Tailwind rozstrzyga `text-body` na BARWĘ, nie
          na stopień. Dwie barwy na jednym napisie — wygrywała druga. Stopień prozy niesie i tak
          `body` z arkusza (DESIGN §6), więc zostaje sama barwa, i to ta, która działała. */}
      <p className="text-ink">{note.rule}</p>

      {/* Powód stoi pod nim, na ekranie, zawsze. To jest jedyna rzecz, po której człowiek
          poznaje, czy TO JEST PRAWDA — a bez „dlaczego" notatki nie da się później bezpiecznie
          usunąć, bo trzeba od nowa wyprowadzić jej interakcje z każdą inną [T6 §5.1]. */}
      <p className="lead">{note.because}</p>

      {note.leftOut ? (
        <p className="lead" data-tone="attend">
          Not in prompts right now because it exceeds the length limit.
        </p>
      ) : null}

      {asking ? (
        /* POTWIERDZENIE JEST PRAWDZIWYM RENDEREM, nie `window.confirm` — ten sam wybór i ten
           sam powód, co przy usuwaniu agenta (`src/sections/agents/index.tsx`): dialog
           przeglądarki blokuje webview i zabiera całą sesję pracy, a przy oknie Tauri nie ma
           go czym odblokować.

           PYTANIE STOI W MIEJSCU AKCJI, nie obok nich. Zdanie o nieodwracalności obok wciąż
           czynnego „Discard" zostawia człowieka z dwoma przyciskami o tej samej nazwie
           i z pytaniem, na które można nie odpowiedzieć. Sprężyna mówi „to jest nowe",
           zamiast pozwolić dwóm różnym rzeczom mrugnąć w jednym miejscu; drugiego regionu to
           zdarzenie nie rusza (ARCHITECTURE §7). */
        <div className="stack" data-gap="2">
          <p data-confirm-drop className="enter text-ink">
            {NOT_COMING_BACK}
          </p>
          <div className="flex items-center gap-2">
            <button
              type="button"
              data-forever={note.id}
              className="btn-danger"
              onClick={() => {
                onDiscardForGood?.(address);
              }}
            >
              Discard for good
            </button>
            <button
              type="button"
              className="btn-quiet"
              onClick={() => {
                onKeepIt?.();
              }}
            >
              Keep it
            </button>
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          {legacy ? (
            onMove ? (
              <button
                type="button"
                data-move={note.id}
                className="btn-quiet"
                onClick={() => {
                  onMove(address);
                }}
              >
                Move to this project
              </button>
            ) : null
          ) : (
            <button
              type="button"
              data-act={note.id}
              className="btn-quiet"
              onClick={() => {
                if (waiting) {
                  onUse(address);
                } else {
                  onStopUse(address);
                }
              }}
            >
              {waiting ? 'Use this' : 'Stop using'}
            </button>
          )}

          {/* Druga decyzja — i WYŁĄCZNIE przy kandydatce. Odrzucenie notatki, która właśnie jedzie
            do promptu, jest drugim pytaniem w ubraniu pierwszego: znika w jednym kliknięciu
            z miejsca, w którym człowiek jej szukał, a on prosił o jedno. Najpierw „Stop using",
            potem decyzja, czy to zdanie ma odejść.

            Warunek pyta też o handler, bo wiersz montuje się i bez niego (patrz `onDiscard`):
            przycisk narysowany bez handlera odmawia każdemu kliknięciu, a to jest z zewnątrz
            nieodróżnialne od zepsutej aplikacji (niezmiennik 16). */}
          {!legacy && waiting && onDiscard ? (
            <button
              type="button"
              data-drop={note.id}
              className="btn-quiet"
              onClick={() => {
                onDiscard(address);
              }}
            >
              Discard
            </button>
          ) : null}
        </div>
      )}
    </li>
  );
}
