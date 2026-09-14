/* Połączenia agenta się WYBIERA z biblioteki tego projektu, a nie wpisuje z pamięci (2026-09-14).
 *
 * PO CO TO ISTNIEJE. Do dziś stało tu pole tekstowe z nazwami po przecinku, a nazwa z literówką
 * przechodziła zapis bez słowa: odmawiał dopiero Start rozmowy (`connections::runtime::selected`
 * oddaje `NotFound` albo `NotEnabled`), długo po tym, jak człowiek zamknął formularz. Lista
 * z dysku — jedyna rzecz, której temu pickerowi brakowało — przyjechała biegiem `conn-seed`.
 *
 * `<select multiple>`, A NIE POLA WYBORU, i to jest pomiar, nie gust. Trzy specyfikacje e2e
 * czytają to pole przez `inputValue()` na `#agent-connections` (nowa rola, pusta biblioteka,
 * gotowy agent z pierwszego ekranu). Playwright umie to na `<select>`; na `<div>` z polami
 * wyboru rzuca i wywala wszystkie trzy. Z tego samego pomiaru bierze się warunek na `value`
 * opcji: to jest GOŁA nazwa, a dopisek żyje wyłącznie w napisie opcji — inaczej nazwa niesiona
 * przez agenta, a nieobecna w bibliotece, zmieniałaby to, co tamte trzy czytają.
 *
 * DOPISEK DOPIERO PO ODCZYCIE (niezmiennik 17). Zanim lista przyjdzie, nie wiadomo, czy nazwa
 * jest w bibliotece — „not in this project" napisane wtedy byłoby zdaniem nieprawdziwym.
 * Sama kontrolka stoi w dokumencie od PIERWSZEJ klatki, bo `renderToStaticMarkup` nie odpala
 * `useEffect`, a dwie wyrocznie sądzą stamtąd etykietę wskazującą `id`
 * (`../field-is-a-well-under-its-label.test.tsx`, `./vendor-matrix.test.tsx`).
 *
 * ŻADNA KLASA `label` NIE WYCHODZI STĄD. Wiersze rozwiniętego formularza porównuje przez
 * `toEqual` `./agent-form.test.tsx` i liczy je po `class="label"` — etykietę „Connections"
 * niesie `./more-settings.tsx`, a picker stoi pod nią.
 *
 * `disabled` I `title` ZNIKŁY RAZEM Z POLEM TEKSTOWYM, i to nie jest przeoczenie: `connections`
 * jest w tabeli `native` przy OBU aplikacjach (`./capabilities.ts`, przypięte wiersz w wiersz
 * przez `./vendor-matrix.test.tsx`), więc obie gałęzie były nieosiągalne. Nieosiągalna gałąź
 * niosła tu drugą kopię zdania o Codeksie, a druga kopia zawsze w końcu mówi co innego
 * (niezmiennik 13). Dzień, w którym któraś aplikacja to pole zamknie, jest dniem zmiany tamtej
 * tabeli — i wtedy wraca tu warunek czytający JĄ, a nie własny `if vendor === …`.
 */
import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';
import type { Agent } from '../../state/agents';
import { useWorkspaces } from '../../state/workspaces';
import { why } from '../../ipc/why';
import { connectionsOf } from './io';

/** Dopisek przy nazwie, której biblioteka tego projektu nie zna. Tylko w napisie opcji. */
const NOT_HERE = ' (not in this project)';

/** Co ten dopisek znaczy — raz, pod listą, a nie przy każdej pozycji z osobna. */
const WHAT_NOT_HERE_MEANS =
  'A connection marked "not in this project" is not in this library, so a run that needs it ' +
  'will not start. Take it off, or bring it in here first.';

/** Zdanie zapasowe TEJ czynności: ogólne „coś poszło nie tak" jest gorsze niż brak. */
const COULD_NOT_READ = 'The connections of this project could not be read.';

/** Opis pod listą, kiedy stoi na niej nazwa spoza biblioteki. */
const NOTE = 'agent-connections-note';

export function ConnectionPicker({
  value,
  onChange,
  describedBy,
}: {
  readonly value: Agent;
  readonly onChange: (next: Agent) => void;
  /** Cudzy opis tej kontrolki, jeśli wołający jakiś postawił. Dokładany, nie podmieniany. */
  readonly describedBy?: string | undefined;
}): ReactElement {
  const folder = useWorkspaces(
    (state) => state.all.find((one) => one.id === state.activeId)?.folder ?? null,
  );
  /* `null` znaczy „jeszcze nie wiadomo", i to jest ta sama trójka stanów, co w sekcji
     (`./index.tsx`): lista znaczy „przeczytane", brak listy plus zdanie znaczy „nie udało się". */
  const [library, setLibrary] = useState<readonly string[] | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  /* Zależność jest SAM folder, nigdy nazwy niesione przez agenta: biblioteka projektu nie zależy
     od tego, co ktoś przed chwilą kliknął, a efekt zawieszony na wyborze gasiłby listę przy
     każdym zaznaczeniu i odbierał drugie kliknięcie pod palcem. */
  useEffect(() => {
    let current = true;
    setLibrary(null);
    setSaid(null);
    void connectionsOf(folder)
      .then((names) => {
        if (current) setLibrary(names);
      })
      .catch((error: unknown) => {
        if (current) setSaid(why(error, COULD_NOT_READ));
      });
    return () => {
      current = false;
    };
  }, [folder]);

  /* Nazwy niesione przez agenta, których biblioteka nie zna — literówka, połączenie wyłączone
     albo usunięte. ZOSTAJĄ na liście zaznaczone, bo znikająca po cichu nazwa wygląda z zewnątrz
     dokładnie tak samo jak zapis, który się udał. Odznacza je człowiek, nie zapis. */
  const strange = library === null ? [] : value.connections.filter((one) => !library.includes(one));
  /* Zbiór, bo ta sama nazwa potrafi stać w agencie dwa razy: pole tekstowe, które ta lista
     zastępuje, przyjmowało „figma, figma" bez słowa. Dwie pozycje o jednym `value` to jedna
     pozycja, której nie da się odznaczyć do końca. Kolejność zostaje z biblioteki, a za nią
     to, co niesie agent — `Set` zachowuje pierwsze wystąpienie. */
  const offered = [
    ...new Set(library === null ? value.connections : [...library, ...value.connections]),
  ];
  const describes = [strange.length > 0 ? NOTE : '', describedBy ?? ''].filter((one) => one !== '');

  return (
    <>
      <select
        id="agent-connections"
        data-field="connections"
        className="field"
        multiple
        value={value.connections}
        aria-describedby={describes.length === 0 ? undefined : describes.join(' ')}
        /* Kolejność bierzemy z dokumentu, a nie z kliknięć: `selectedOptions` idzie po opcjach
           tak, jak stoją, więc dwa te same wybory dają jeden zapis niezależnie od tego, w jakiej
           kolejności ktoś je klikał. */
        onChange={(event) => {
          const connections = Array.from(event.target.selectedOptions, (one) => one.value);
          onChange({ ...value, connections });
        }}
      >
        {offered.map((name) => (
          <option key={name} value={name}>
            {strange.includes(name) ? name + NOT_HERE : name}
          </option>
        ))}
      </select>
      {strange.length === 0 ? null : (
        <p id={NOTE} className="lead">
          {WHAT_NOT_HERE_MEANS}
        </p>
      )}
      {/* POD LISTĄ, a nie zamiast niej: odmowa odczytu nie zabiera człowiekowi tego, co agent
          już niesie — tamte nazwy są w jego pliku niezależnie od tego, czy biblioteka odpowiedziała. */}
      {said === null ? null : (
        <p role="alert" className="lead">
          {said}
        </p>
      )}
    </>
  );
}
