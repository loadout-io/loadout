/* `Advanced`: jeden wiersz, przelotka surowych argumentów do aplikacji, którą ten agent biegnie.
 *
 * ══ CZEMU OSOBNO OD `More settings` — 2026-08-31 ═════════════════════════════════════════════
 *
 * Ten wiersz stał do dziś jako piąty pod `More settings`, między `Skills` a `Connections`, czyli
 * w tej samej randze co lista umiejętności. To jest zła ranga i widać to po skutku pomyłki:
 * literówka w `Skills` daje agenta bez jednej umiejętności, a literówka TUTAJ zmienia komendę,
 * którą uruchamiamy — flaga podana bez wartości połyka następny argument jako swój, więc
 * polecenie znaczy co innego, niż wygląda. Jedno jest ustawieniem, drugie jest wpisywaniem
 * w wiersz poleceń cudzej aplikacji.
 *
 * Nazwa `Advanced` nie jest ozdobnikiem ani ostrzeżeniem: jest jedyną rzeczą, po której człowiek
 * pozna, że otwiera co innego niż listę umiejętności. Jeden przycisk na oba znaczy człowieka,
 * który otwiera jedno i znajduje drugie.
 *
 * ══ CO SIĘ NIE ZMIENIŁO ══════════════════════════════════════════════════════════════════════
 *
 * Wiersz przyszedł tu z `more-settings.tsx` w całości: te same dwie tabele kształtu pary, ten
 * sam klucz w pliku, ta sama podróż w obie strony. Powody stoją niżej, przy każdej tabeli.
 *
 * PIĄTY WIERSZ WYMAGAŁ PRAWDZIWEJ SKARGI (T3 §10, ryzyko 6) i ta jest zmierzona: przelotka
 * `vendorOptions` jest w formacie agenta od pierwszego dnia, od T-90 naprawdę dojeżdża do argv
 * — i nie dało się jej ustawić inaczej niż edycją pliku na dysku. Ustawienie, do którego nie ma
 * kontrolki, jest ustawieniem, którego nie ma.
 */
import type { ReactElement } from 'react';
import type { Agent, Vendor } from '../../state/agents';
/* NAZWA APLIKACJI PRZYCHODZI Z TABELI, KTÓRA JĄ JUŻ MA (niezmiennik 13). `VENDORS` napędza
 * wiersz `Runs with` w formularzu obok, a tutaj nazywa wiersz przelotki — druga kopia brzmienia
 * rozjechałaby się przy pierwszej zmianie i nikt by tego nie zauważył, bo nazwa z drutu
 * (`claude-code`) i tak nie ma prawa dotrzeć na ekran (niezmiennik 14).
 *
 * Import wraca do pliku, który ten importuje, i to jest świadome: obie strony czytają się
 * dopiero przy renderowaniu, a nie przy wczytywaniu modułu, więc pierścień nigdy nie sięga po
 * wartość, której jeszcze nie ma. */
import { VENDORS } from './agent-form';

export interface AdvancedProps {
  value: Agent;
  onChange: (next: Agent) => void;
}

const FIELD = 'field';

/* KLUCZ W PLIKU TO `claude`, NIE `claude-code`. Tak nazywa go plik agenta i tak pyta o niego
 * strona rustowa (`library::agents::vendor_argv`, `workflow::check::reserved`). Wpisanie tu
 * drugiej pisowni dałoby przelotkę, która zapisuje się na ekranie i nie dojeżdża do nikogo. */
const PASSTHROUGH_KEY: Record<Vendor, string> = {
  'claude-code': 'claude',
  codex: 'codex',
};

/* KSZTAŁT PARY JEST WŁASNOŚCIĄ APLIKACJI, nie tego pliku, i te dwa są tu LUSTREM strony
 * rustowej — dokładnie tak, jak typy w `src/state/agents.ts` są lustrem tamtejszych struktur.
 * Claude Code bierze `--flaga wartość`, Codex `klucz=wartość`. Jeden kształt dla obu wygląda
 * poprawnie na ekranie i wywala drugą aplikację przy pierwszym prawdziwym biegu. */
const SEPARATOR: Record<Vendor, string> = {
  'claude-code': ' ',
  codex: '=',
};

/** Przykład, którym pole mówi, czego od człowieka chce, zamiast opisywać to zdaniem. */
const LIKE: Record<Vendor, string> = {
  'claude-code': '--fallback-model sonnet',
  codex: 'model_verbosity=high',
};

/** Wiersze z pola → pary do zapisania w pliku. Wiersz bez wartości zostaje z pustą wartością:
 * to on jest odmową zapisu, a wykasowanie go tutaj zabrałoby zdaniu, które ją tłumaczy,
 * wiersz, o którym mówi (`missingForSave` w `src/state/agents.ts`). */
function pairsFrom(text: string, vendor: Vendor): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const one = line.trim();
    if (one.length === 0) continue;
    const at = one.indexOf(SEPARATOR[vendor]);
    const name = at === -1 ? one : one.slice(0, at).trim();
    const value = at === -1 ? '' : one.slice(at + 1).trim();
    if (name.length > 0) out[name] = value;
  }
  return out;
}

/** I z powrotem: pary z pliku → wiersze, w kolejności, w jakiej je zapisano. */
function textFrom(pairs: Record<string, string>, vendor: Vendor): string {
  return Object.entries(pairs)
    .map(([name, value]) => (value === '' ? name : name + SEPARATOR[vendor] + value))
    .join('\n');
}

/** Nazwa aplikacji, którą ten agent biegnie — z tabeli, która ją już ma. */
function appName(vendor: Vendor): string {
  return VENDORS.find((one) => one.value === vendor)?.label ?? vendor;
}

export function Advanced({ value, onChange }: AdvancedProps): ReactElement {
  return (
    /* WEJŚCIE SPRĘŻYNĄ, 2026-08-31 (DESIGN §7): tego wiersza NIE MA w dokumencie, dopóki
       człowiek nie naciśnie `Advanced` — jest poza drzewem, nie schowany stylem. Powierzchnia,
       która pojawia się skokiem pod przyciskiem, czyta się jak przeskok widoku; dorastanie do
       miejsca mówi „przyszedłem stamtąd" i kosztuje 200 ms. */
    <div className="stack enter border-t border-line pt-3" data-gap="3">
      {/* WIERSZ NALEŻY DO APLIKACJI Z `Runs with` i mówi to swoją etykietą. Jedna nazwa, nie
          obie: dwie zamieniłyby jeden wiersz w dwa pytania, a człowiek odpowiedziałby na
          niewłaściwe — kształt pary jest u tych dwóch inny.

          PRZEŁĄCZENIE APLIKACJI CHOWA WPISY DRUGIEJ, NIE KASUJE ICH. Plik trzyma obie mapy,
          bo porównywanie tych samych instrukcji na dwóch aplikacjach jest zwykłym dniem pracy,
          a utrata ustawień tej, od której się odeszło, jest cicha: człowiek dowiaduje się
          o niej dopiero przy następnym biegu tamtej. */}
      <div className="stack">
        <label htmlFor="agent-vendor-options" className="label">
          {`Extra options for ${appName(value.runsWith)}`}
        </label>
        <textarea
          id="agent-vendor-options"
          data-field="vendorOptions"
          className={FIELD}
          value={textFrom(
            value.vendorOptions?.[PASSTHROUGH_KEY[value.runsWith]] ?? {},
            value.runsWith,
          )}
          placeholder={LIKE[value.runsWith]}
          onChange={(event) =>
            onChange({
              ...value,
              vendorOptions: {
                ...value.vendorOptions,
                [PASSTHROUGH_KEY[value.runsWith]]: pairsFrom(event.target.value, value.runsWith),
              },
            })
          }
        />
        <p className="lead">{`One pair per line, like ${LIKE[value.runsWith]}.`}</p>
      </div>
    </div>
  );
}
