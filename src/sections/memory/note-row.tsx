/* Wiersz notatki: dwa stany, jeden żywy region na fakt i jedna akcja.
 *
 * NIEZMIENNIK 13 RZĄDZI TYM PLIKIEM. Stan notatki ma dokładnie JEDEN żywy region w wierszu:
 * chip. Etykieta przycisku jest *akcją* („Use this"), nie powtórzonym stanem — chip
 * „Suggested", obok tekst „not in use yet", a na dole licznik „3 suggested" to trzy regiony
 * na jeden fakt i dokładnie to, przez co poprzedni prototyp pokazywał stan połączenia w sześciu
 * miejscach [ARCHITECTURE §7: żywe regiony na jeden fakt = 1].
 *
 * NIEZMIENNIK 14: w tym wierszu istnieją wyłącznie słowa `Suggested`, `In use`, `Use this`,
 * `Stop using` i `length`. Trzeci stan (`candidate`, `confirmed`, `corroborated`, `trusted`,
 * `archived`, `replaced`) i żargon (`promote`, `token`) wchodzą właśnie tędy — z makiety,
 * z enuma z drutu albo z pola, które ktoś wypisał „na wszelki wypadek".
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
  /** Jedyna akcja notatki projektowej, która nadal leży w bibliotece. */
  onMove?: (address: NoteAddress) => void;
}

/* `chip` z DESIGN §6: wysokość 20px, `--t-label`, obrys `{stan}-edge`, tło `{stan}-wash`.
 *
 * Kolor jest wybrany, nie odziedziczony po makiecie. `--attend` odpowiada na pytanie „co czeka
 * na moją decyzję?" [DESIGN §3] i kandydatka jest dokładnie tym — jedyną rzeczą w tej sekcji,
 * która czegoś od człowieka chce. Notatka w użyciu nie chce niczego, więc dostaje wariant
 * neutralny: gdyby i ona była nasycona, kolor przestałby oznaczać „twoja kolej" i zostałby
 * ozdobą, po której nie da się przebiec wzrokiem. `--accent` odpada osobno — znaczy „teraz",
 * czyli coś, co się dzieje w tej chwili, a notatka niczego nie robi. */
const CHIP_WAITING =
  'h-5 rounded-pill border border-attend-edge bg-attend-soft px-2 text-label text-attend';
const CHIP_QUIET = 'h-5 rounded-pill border border-line bg-raised px-2 text-label text-muted';

/* `button-quiet` z DESIGN §6: przezroczyste tło, obrys `--line`, wysokość 28px. Akcja odwracalna
 * i wykonywana wielokrotnie nie ma być najgłośniejszą rzeczą w wierszu. */
const ACT = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

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

/* Czyja to wiedza i skąd przyjechała — dwa fakty, które do 2026-08-22 (T-80) nie miały na
 * ekranie ani jednego miejsca.
 *
 * OBA WIERSZE POWSTAJĄ WYŁĄCZNIE Z POLA NOTATKI. Notatka niczyja nie dostaje myślnika ani słowa
 * w rodzaju „unassigned": to jest odpowiedź na pytanie, którego nikt nie zadał, a człowiek czyta
 * ją jako fakt o notatce (niezmiennik 13 — jeden żywy region na fakt, i ani jeden na fakt,
 * którego nie ma). Wiersz, który wypisuje ostatnią nazwę, jaką widział, wygląda poprawnie na
 * notatce jednego agenta i kłamie o każdej innej.
 *
 * „Only", bo to jest CAŁA treść zakresu jednego agenta: ta notatka nie dociera do reszty kroków.
 * Samo `backend-dev` obok `Length 137` jest nazwą bez zdania — człowiek nie ma jak zgadnąć,
 * czy to autor, czy adresat. */
function ownerLabel(agent: string): string {
  return 'Only ' + agent;
}

/* Skąd ta notatka przyjechała. To samo zdanie może zostać przywiezione z dwóch projektów,
 * a wiersz, który nigdy nie pokazuje pochodzenia, czyta drugą kopię jako drugi fakt — i wtedy
 * nikt nie umie powiedzieć, którą z dwóch właśnie odstawia. */
function originLabel(from: string): string {
  return 'From ' + from;
}

export function NoteRow({ note, onUse, onStopUse, onDiscard, onMove }: NoteRowProps): ReactElement {
  /* Jedno pytanie zadane RAZ. Trzy osobne `note.status === 'suggested'` w trzech gałęziach to
   * trzy miejsca, w których wiersz odpowiada na to samo — i pierwsze, które ktoś zmieni,
   * rozjedzie się z dwoma pozostałymi bez śladu w typach. */
  const waiting = note.status === 'suggested';
  const address: NoteAddress = { place: note.place, id: note.id };
  const legacy = note.place === 'library' && note.scope === 'this-project';

  return (
    <li
      data-note={note.id}
      data-note-address={`${note.place}:${note.id}`}
      className="flex flex-col gap-1 border-b border-line px-2 py-3"
    >
      <div className="flex items-center gap-2">
        {legacy ? null : (
          <span data-state className={waiting ? CHIP_WAITING : CHIP_QUIET}>
            {waiting ? 'Suggested' : 'In use'}
          </span>
        )}
        <span className="text-label text-muted">{lengthLabel(note.length)}</span>
        {/* Pytamy o WARTOŚĆ, nie o obecność klucza: Rust przysyła `agent: null` dla notatki
            niczyjej (`NoteWire`), a `note.agent === undefined` wypisałoby wtedy słowo `null`
            w miejscu właściciela — czyli dokładnie tę zmyśloną odpowiedź, której tu nie ma.

            I pytamy NAJPIERW o zakres. Nazwa agenta w pliku notatki, której zakres sięga całego
            projektu, jest śladem po autorze, a nie odpowiedzią na pytanie „do kogo to dojedzie":
            taka notatka jedzie do KAŻDEGO kroku. Wiersz, który wypisuje wtedy „Only backend-dev",
            mówi o zasięgu coś, co jest nieprawdą — a człowiek czyta to jako fakt o notatce
            i zostawia w użyciu zdanie, o którym myśli, że dotyczy jednego agenta. */}
        {note.scope === 'this-agent' && note.agent ? (
          <span className="text-label text-muted">{ownerLabel(note.agent)}</span>
        ) : null}
        {note.from ? <span className="text-label text-muted">{originLabel(note.from)}</span> : null}
      </div>

      {/* Zdanie, które naprawdę jedzie do modelu — nie streszczenie tego zdania. */}
      <p className="text-body text-ink">{note.rule}</p>

      {/* Powód stoi pod nim, na ekranie, zawsze. To jest jedyna rzecz, po której człowiek
          poznaje, czy TO JEST PRAWDA — a bez „dlaczego" notatki nie da się później bezpiecznie
          usunąć, bo trzeba od nowa wyprowadzić jej interakcje z każdą inną [T6 §5.1]. */}
      <p className="text-body text-muted">{note.because}</p>

      <div className="flex items-center gap-2">
        {legacy ? (
          onMove ? (
            <button
              type="button"
              data-move={note.id}
              className={ACT}
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
            className={ACT}
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
            className={ACT}
            onClick={() => {
              onDiscard(address);
            }}
          >
            Discard
          </button>
        ) : null}
      </div>
    </li>
  );
}
