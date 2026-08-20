/* Wiersz, który okno dopisuje samo: to, co wysłałeś, i to, co ten wiersz odpowiedział.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-20: komendy nie zostawiają po sobie ani
 * jednego wiersza w strumieniu. Cicha porażka, przed którą stoi ten plik: terminal, w którym
 * wpisana komenda nie zostawia śladu, jest NIEODRÓŻNIALNY od terminala, który tej komendy nie
 * przyjął. Właściciel miał dokładnie tę wątpliwość 2026-08-19 przy prozie („a może odpisuje on,
 * ale na pewno nie widać moich wiadomości") i wtedy powstał `Line::Told` po stronie Rusta.
 * Komendy zostały z tą samą wadą, bo drut ich nie widzi nigdy: `/run`, `/open`, `/stop`
 * i odpowiedzi samego wiersza (`NOT_KNOWN`, `NOTHING_RUNS`, odmowy) obsługuje okno.
 *
 * CZEGO TEN PLIK NIE ROBI: nie dubluje prozy. Zdanie bez ukośnika ma nośnik na drucie od
 * 2026-08-19 — `Line::Told` wystawia `commands/chat.rs` i `commands/run.rs`, a widok podpisuje
 * je `You →` (`../feed/line.tsx`). Dwa wiersze o jednym zdaniu to dwa miejsca prawdy
 * (niezmiennik 13), więc [`echoOf`] oddaje wtedy `null`.
 *
 * DLACZEGO IDENTYFIKATOR JEST UJEMNY. Stempel powstaje na granicy i osobno dla każdej pompy:
 * `../io.ts` liczy od 1 w `start()` i od 1 w `openChat()`, więc dodatnie numery już dziś potrafią
 * się powtórzyć w jednym oknie. Wiersz składany tutaj wchodzi do tej samej historii, więc
 * potrzebuje przestrzeni numerów, której żadna z tych dwóch pomp nie tknie — a przy okazji
 * NIESIE swoje pochodzenie: numer ujemny nie ma prawa udawać zdarzenia biegu (niezmiennik 4).
 * Tego wiersza nie ma w `run.json` i nie przeżyje przeładowania okna.
 */
import type { Line } from '../../../ipc/types';
import type { Stamped } from '../../../state/run';

/**
 * Wiersz dopisany przez OKNO — linia strumienia, która zawsze niesie zdanie.
 *
 * `Extract`, a nie konkretny rodzaj: kryterium sprawdza podpis WOŁAJĄC `authorityOf(kind)`,
 * więc wybór rodzaju należy do implementacji, a nie do typu. Odsiane są dokładnie te dwa
 * rodzaje, które zdania nie niosą i nie wchodzą do historii (`thinking`, `stepState`) —
 * wiersz bez tekstu nie jest echem niczego.
 */
export type WindowLine = Extract<Line, { text: string }> & Stamped;

/**
 * Podpis, którym te wiersze stoją w strumieniu.
 *
 * Loadout, nie „You" i nie nazwa agenta, i rozstrzyga to ta sama polityka, którą sprawdza
 * kryterium: autorem wiersza jest ten, kto go NAPISAŁ (`authorityOf` → `loadout`), a nie ten,
 * kto wystukał linię. Podpis `agent` byłby cytatem przypisanym komuś, kto go nie wypowiedział;
 * podpis `You` pożyczałby znak, który należy do prozy naprawdę jadącej drutem (`told`).
 *
 * KOSZT TEGO PODPISU JEST ZAPISANY, bo nie da się go stąd zapłacić: `../rail/roster.ts` buduje
 * kafelek dla KAŻDEJ nazwy występującej w polu `agent` wiersza historii, więc pierwsza komenda
 * w sesji dokłada do listy agentów kafelek „Loadout" ze stanem `working`. To nie jest nowa
 * klasa wady — rozmowa z liderem robi dziś dokładnie to samo (`Line::Told` niesie `agent:
 * "Lead"`, `commands/chat.rs`) — ale jest wadą: Loadout nie jest agentem, który pracuje
 * (niezmiennik 17). Naprawa mieszka w `roster.ts`, czyli poza blokiem OWNS tego zadania.
 */
const LOADOUT = 'Loadout';

/**
 * Znak, którym wiersz mówi „to zostało WPISANE", a nie „to Loadout stwierdza".
 *
 * Ten sam znak, który stoi przed polem w makiecie (`.entry .p`, `entry.tsx`), więc echo czyta
 * się jak echo terminala, a nie jak zdanie Loadouta o ukośniku. Bez niego wiersz `Loadout
 * /stop` wygląda, jakby komendę wypowiedział Loadout — a wypowiedział ją człowiek, którego
 * podpisem ten wiersz stać nie może (powód przy [`LOADOUT`]).
 */
const TYPED_HERE = '❯ ';

/**
 * Rodzaj, którym okno mówi o sobie.
 *
 * Trzy własności rozstrzygają ten wybór i wszystkie trzy sprawdza kryterium przez `appendLines`:
 * `authorityOf('run')` to `loadout`, etykieta wiersza jest CAŁYM zdaniem (rodzaje z tabeli
 * sklejania — `read`, `edit`, `search`, `memory` — zamieniłyby wpisaną komendę na „Edited
 * 2 files"), a wiersz jest rozwinięty domyślnie, czyli widać go bez klikania (`../feed/kinds.ts`).
 */
const WINDOW_KIND = 'run' as const;

/**
 * Ostatni wydany numer. Maleje, więc nigdy nie zderzy się z żadną z dwóch pomp.
 *
 * Na poziomie modułu, nie w komponencie: wiersz wejścia odmontowuje się przy każdym wyjściu
 * do innej sekcji, a licznik zerowany przy powrocie wydałby drugi raz numery, które już stoją
 * w historii — czyli dokładnie tę kolizję, przed którą to pole ma bronić.
 */
let last = 0;

/** Świeży wiersz okna z tym zdaniem. */
function windowLine(text: string): WindowLine {
  last -= 1;
  return {
    kind: WINDOW_KIND,
    agent: LOADOUT,
    text,
    id: last,
    /* Chwila, w której to się stało — tym samym zegarem, którym stempluje granica
     * (`../io.ts`), bo wiersze z obu źródeł wchodzą do jednej historii i jedno okno
     * sklejania. */
    at: Date.now(),
  };
}

/**
 * Wiersz dla linii, którą człowiek właśnie wysłał — albo `null`, kiedy okno nie ma czego
 * dopisywać.
 *
 * Rozstrzyga UKOŚNIK i tylko on: komendy oraz literówki w komendach są niewidoczne dla drutu,
 * więc ich jedynym śladem jest to, co dopisze okno. Proza wraca z drutu jako `told`.
 */
export function echoOf(typed: string): WindowLine | null {
  /* Przycięta, bo wiodąca spacja w strumieniu jest szumem — ale w środku linia zostaje
   * NIETKNIĘTA: to, co człowiek wpisał, ma dać się przepisać z tego wiersza znak w znak. */
  const line = typed.trim();
  if (!line.startsWith('/')) return null;
  return windowLine(TYPED_HERE + line);
}

/**
 * Wiersz dla tego, co odpowiedział sam wiersz wejścia — nieznana komenda, „nic nie biegnie",
 * odmowa startu.
 *
 * TEN SAM KSZTAŁT, co echo linii, i to jest cała treść przeprowadzki: rozmowa z Loadoutem jest
 * JEDNĄ historią, a nie połową pod polem. Do 2026-08-20 te zdania lądowały w `data-entry-said`,
 * czyli w drugim, znikającym miejscu na to samo — a przy trzech zdaniach z rzędu widać było
 * ostatnie.
 */
export function saidOf(said: string): WindowLine {
  /* Zdanie idzie SŁOWO W SŁOWO, bez znaku echa: te napisy są już napisane dla człowieka —
   * tutaj i w odmowach, które przysyła Rust — więc cokolwiek doklejonego jest copy, którego
   * nikt nie napisał. */
  return windowLine(said);
}
