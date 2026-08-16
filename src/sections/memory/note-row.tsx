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
import type { Note } from '../../state/memory';

export interface NoteRowProps {
  note: Note;
  /** „Use this". Handler jest wymagany, bo kontrolka bez handlera nie wchodzi do repo
   * (niezmiennik 16) — a wiersz nie zna magazynu i nie ma jak zawołać go sam. */
  onUse: (id: string) => void;
  /** „Stop using". Ta sama reguła. */
  onStopUse: (id: string) => void;
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
  'h-5 rounded-sq border border-attend-edge bg-attend-wash px-2 text-label text-attend';
const CHIP_QUIET = 'h-5 rounded-sq border border-line bg-raised px-2 text-label text-muted';

/* `button-quiet` z DESIGN §6: przezroczyste tło, obrys `--line`, wysokość 28px. Akcja odwracalna
 * i wykonywana wielokrotnie nie ma być najgłośniejszą rzeczą w wierszu. */
const ACT = 'h-7 rounded-sq border border-line px-3 text-ui text-body';

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

export function NoteRow({ note, onUse, onStopUse }: NoteRowProps): ReactElement {
  /* Jedno pytanie zadane RAZ. Trzy osobne `note.status === 'suggested'` w trzech gałęziach to
   * trzy miejsca, w których wiersz odpowiada na to samo — i pierwsze, które ktoś zmieni,
   * rozjedzie się z dwoma pozostałymi bez śladu w typach. */
  const waiting = note.status === 'suggested';

  return (
    <li data-note={note.id} className="flex flex-col gap-1 border-b border-line px-2 py-3">
      <div className="flex items-center gap-2">
        <span data-state className={waiting ? CHIP_WAITING : CHIP_QUIET}>
          {waiting ? 'Suggested' : 'In use'}
        </span>
        <span className="text-label text-muted">{lengthLabel(note.length)}</span>
      </div>

      {/* Zdanie, które naprawdę jedzie do modelu — nie streszczenie tego zdania. */}
      <p className="text-body text-ink">{note.rule}</p>

      {/* Powód stoi pod nim, na ekranie, zawsze. To jest jedyna rzecz, po której człowiek
          poznaje, czy TO JEST PRAWDA — a bez „dlaczego" notatki nie da się później bezpiecznie
          usunąć, bo trzeba od nowa wyprowadzić jej interakcje z każdą inną [T6 §5.1]. */}
      <p className="text-body text-muted">{note.because}</p>

      <div className="flex items-center gap-2">
        <button
          type="button"
          data-act={note.id}
          className={ACT}
          onClick={() => {
            if (waiting) {
              onUse(note.id);
            } else {
              onStopUse(note.id);
            }
          }}
        >
          {waiting ? 'Use this' : 'Stop using'}
        </button>
      </div>
    </li>
  );
}
