/* Wiersz przekazania: jeden plik, który jeden agent zostawił drugiemu.
 *
 * DLACZEGO TO NIE JEST ODNOŚNIK, CHOĆ MAKIETA RYSUJE `<a href="#">`. Makieta
 * (`docs/mockup/index.html`, ekran `memory`, blok `.ctx`) daje każdej pozycji odnośnik.
 * W tej fali nie ma komendy, która otwiera plik w edytorze człowieka, więc odnośnik byłby
 * kontrolką bez skutku — a kontrolka, która reaguje i nic nie robi, jest GORSZA niż jej brak
 * (niezmiennik 16) i jest dokładnie tym defektem, który ta fala naprawia. Zamiast niej:
 * ŚCIEŻKA na ekranie, do zaznaczenia i skopiowania (`data-copyable` z `src/styles/theme.css`
 * pozwala ją zaznaczyć — reszta interfejsu zaznaczać się nie daje). Zdanie strefy mówi
 * „open them anywhere" i po tej zmianie jest prawdziwe: człowiek dostaje adres, a nie obietnicę
 * kliknięcia.
 *
 * NUMERU POZYCJI TU NIE MA, CHOĆ MAKIETA GO MA (`01`, `02`, `03`). `HandoffWire` nie niesie
 * pola z kolejnością, a numer policzony z pozycji w tablicy byłby relacją, której nie ma
 * w danych (niezmiennik 17) — tym bardziej że `list_handoffs` nie zna zakresu i lista może
 * objąć więcej niż jeden bieg, gdzie numeracja startuje od nowa. Numer i tak jest na ekranie:
 * niesie go NAZWA PLIKU, którą składa `memory::handoff` jako `<NN>__<from>__<kind>.md`.
 * Jedno miejsce, jedna odpowiedź (niezmiennik 13).
 *
 * Czysta funkcja propsów na markup, jak `NoteRow`: bez własnego stanu i bez `invoke()`.
 */
import type { ReactElement } from 'react';
import type { Handoff } from '../../state/memory';

export interface PassedRowProps {
  passed: Handoff;
}

/* `.ctx a` z makiety: tło `--well`, obrys `--line`, krój mono, trzy kolumny w jednej linii
 * bazowej. Bez `:hover`, bo nie ma tu nic do naciśnięcia. */
const ROW =
  'grid grid-cols-[auto_1fr_auto] items-baseline gap-2 border border-line bg-well px-2 py-1';
const WHO = 'font-mono text-label text-muted';
const NAME = 'font-mono text-mono text-body';
const SIZE = 'font-mono text-mono text-muted';
const CHIP_QUIET = 'h-5 rounded-sq border border-line bg-raised px-2 text-label text-muted';

/**
 * Nazwa pliku ze ścieżki.
 *
 * Ostatni człon po `/` i nic więcej: to nie jest parser ścieżek, a odpowiedź na pytanie „jak
 * ten plik się nazywa w katalogu". Ścieżka bez separatora jest już nazwą, ścieżka kończąca się
 * separatorem nie jest plikiem i zostaje pokazana w całości — zgadywanie na niej byłoby
 * wymyślaniem nazwy, której nikt nie widział.
 */
export function fileName(path: string): string {
  const cut = path.lastIndexOf('/');
  if (cut < 0) return path;
  const tail = path.slice(cut + 1);
  return tail === '' ? path : tail;
}

/**
 * „840 B", „1.2 KB" — dokładnie jak w makiecie.
 *
 * Bajty pokazujemy do 1024, wyżej kilobajty z jednym miejscem po kropce, wyżej megabajty.
 * Jedno miejsce po kropce, bo cała rola tej liczby to „czy to jest akapit, czy raport"; trzy
 * cyfry po kropce udają dokładność, o którą nikt nie pytał.
 */
export function sizeLabel(bytes: number): string {
  if (bytes < 1024) return String(bytes) + ' B';
  const kb = bytes / 1024;
  if (kb < 1024) return kb.toFixed(1) + ' KB';
  return (kb / 1024).toFixed(1) + ' MB';
}

/**
 * „Forge → Needle, Rivet" — kto komu.
 *
 * Pusta lista odbiorców daje samo imię nadawcy. Napis „to everyone" byłby relacją, której nie
 * ma w danych (niezmiennik 17): `to: []` znaczy, że plik nie wskazuje adresata, a nie że
 * wskazuje wszystkich.
 */
export function whoToWho(from: string, to: readonly string[]): string {
  return to.length === 0 ? from : from + ' → ' + to.join(', ');
}

/** Czy ten plik został zastąpiony korektą. Przekazania są niezmienne [T6 §9]. */
function replaced(status: string): boolean {
  return status !== 'current';
}

export function PassedRow({ passed }: PassedRowProps): ReactElement {
  return (
    <li data-passed={passed.id} className="flex flex-col gap-1">
      <div className={ROW}>
        <span className={WHO}>{whoToWho(passed.from, passed.to)}</span>
        <span className={NAME}>{fileName(passed.path)}</span>
        <span className={SIZE}>{sizeLabel(passed.bytes)}</span>
      </div>
      {/* Znacznik tylko wtedy, gdy jest o czym mówić. Plik zastąpiony korektą, pokazany bez
          słowa, wygląda dokładnie jak aktualny — a różni się tym jednym, po co się go czyta. */}
      {replaced(passed.status) ? (
        <span data-replaced className={`mr-auto ${CHIP_QUIET}`}>
          Replaced by a later one
        </span>
      ) : null}
      {/* Adres pliku, do zaznaczenia. To jest cała treść zdania „open them anywhere". */}
      <span data-copyable className="font-mono text-label text-muted">
        {passed.path}
      </span>
    </li>
  );
}
