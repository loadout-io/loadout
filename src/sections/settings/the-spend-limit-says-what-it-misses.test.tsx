/* Sufit wydatku mówi NA EKRANIE, czego nie obejmuje — a nie pod kursorem myszy.
 *
 * ZMIERZONE 2026-08-31. Jedynym nośnikiem zdania „kroki Codeksa nie liczą się do tej kwoty"
 * był natywny atrybut `title` pola „Spend at most $" (`../run/limits/budget.tsx`). Dymek
 * pojawia się po sekundzie trzymania myszy nad kontrolką i NIE ISTNIEJE dla klawiatury, dla
 * czytnika ekranu ani na dotyku. Człowiek czytał więc „Spend at most $20" jako twardy sufit,
 * podczas gdy kroki jednego z dwóch dostawców nie dokładały do tej sumy ani centa.
 *
 * SŁABA WERSJA TEGO KRYTERIUM woła `toContain` na całym markupie — i jest ZIELONA na dymku,
 * bo `title="…"` stoi w markupie dokładnie tak samo jak akapit. Dlatego markup przechodzi tu
 * przez [`onScreen`], które kasuje znaczniki RAZEM Z ATRYBUTAMI: zostaje wyłącznie to, co
 * człowiek naprawdę czyta (niezmiennik 29).
 *
 * DLACZEGO EKRAN SETTINGS, A NIE PASEK RUN. Zdanie ma stać na ekranie ZAWSZE, a nie mrugać
 * przy jednym ze stanów — a pasek Run jest widokiem domyślnym i jego gęstość jest mierzona
 * i zapadkowana (`checks/density-baseline.json`, `textElements: 26`; niezmiennik 18 pozwala
 * tej liczbie tylko maleć). Stały akapit w pasku podniósłby ją o jeden, czyli kupiłby to
 * zdanie za regres sufitu z `docs/ARCHITECTURE.md` §7. Karta sufitu w Settings jest jedynym
 * miejscem, w którym ta kwota jest USTAWIANA na stałe, ma tam miejsce na pełne zdanie i nie
 * należy do widoku domyślnego. Jeden fakt, jeden dom (niezmiennik 13).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

/* Atrapa transportu: ten ekran czyta bibliotekę agentów i zapisany sufit przez `invoke`.
 * `renderToStaticMarkup` nie uruchamia efektów, więc żadne z tych wywołań nie pada — atrapa
 * jest tu po to, żeby sam import krawędzi nie zależał od okna Tauri. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => null),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { Budget, BUDGET_HELP } = await import('../run/limits/budget');
const { default: SettingsScreen } = await import('./index');

/** Ile człowiek pozwolił wydać. Liczba bez znaczenia dla tego pytania — pole ma coś pokazywać. */
const CEILING = 20;

function noop(): void {
  // Handler jest wymagany (niezmiennik 16), ale to kryterium nie pyta, co robi.
}

/**
 * Sam tekst, który człowiek czyta: znaczniki znikają RAZEM ZE SWOIMI ATRYBUTAMI.
 *
 * To jest cała różnica między tym kryterium a jego słabą wersją. `title`, `aria-label`
 * i `placeholder` żyją w środku znacznika, więc wypadają stąd wszystkie naraz — a zostaje
 * dokładnie to, co widać bez myszy.
 */
function onScreen(markup: string): string {
  return markup.replace(/<[^>]*>/g, ' ');
}

describe('the spend limit says what it leaves out', () => {
  it('puts the missing half in words a person reads without a mouse', () => {
    expect(
      BUDGET_HELP,
      'the sentence about what this amount leaves out is empty, so the screen below would be ' +
        'judged against nothing at all',
    ).not.toBe('');

    const markup = renderToStaticMarkup(<SettingsScreen />);
    expect(
      onScreen(markup),
      'the amount is set here and nothing on this screen says Codex steps are left out of it. ' +
        'A person reads "Default spend limit $75" as a promise Loadout cannot keep: half the ' +
        'work in a run can cost money that never reaches this number',
    ).toContain(BUDGET_HELP);

    /* Zdanie ma docierać także do kogoś, kto tego ekranu nie ogląda: opis pola jedzie przez
       `aria-describedby`, bo tekst wpisany w `<label>` staje się NAZWĄ kontrolki, nie jej
       opisem. */
    const describedBy = /aria-describedby="([^"]+)"/.exec(markup)?.[1] ?? '';
    const carrier = describedBy
      .split(' ')
      .map((id) => new RegExp(`<p[^>]*\\bid="${id}"[^>]*>([\\s\\S]*?)</p>`).exec(markup)?.[1] ?? '')
      .join(' ');
    expect(
      carrier,
      'the amount field points at nothing, so a screen reader announces a number with no ' +
        'wording around it — the same silence the tooltip left behind',
    ).toContain(BUDGET_HELP);
  });

  it('stops hiding that sentence under the cursor, where a keyboard never finds it', () => {
    expect(
      renderToStaticMarkup(<Budget value={CEILING} onChange={noop} />),
      'the sentence still lives in the tooltip of the amount field. A tooltip appears after a ' +
        'second of holding a mouse still and appears for nobody else: not for a keyboard, not ' +
        'for a screen reader, not on a touch screen',
    ).not.toContain(BUDGET_HELP);
  });
});
