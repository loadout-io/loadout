/* Co się dzieje po naciśnięciu „Evaluate" — jedno miejsce na całą tę politykę.
 *
 * DLACZEGO FUNKCJA, A NIE CIAŁO `onClick`. To repo nie ma jsdom, więc kliknięcia nie da się
 * odpalić w teście, a `renderToStaticMarkup` nie uruchamia handlerów. Polityka zamknięta
 * w komponencie byłaby więc kodem, którego żadne kryterium nie umie dotknąć — dokładnie ta
 * rodzina, z której wzięło się siedemnaście kłamiących kontrolek. Tutaj test woła to, co woła
 * przycisk. Ten sam powód i ten sam kształt stoi przy `run/launch.ts`.
 *
 * DWIE RZECZY, NIE JEDNA, i obie są konieczne: zestaw musi powstać, a człowiek musi się
 * znaleźć tam, gdzie go widać. Sam zapis zostawiłby go na ekranie agenta z wrażeniem, że nic
 * się nie stało; samo przejście zostawiłoby go w Labie z pustą listą.
 */
import { useLab } from '../../state/lab';
import type { Section } from '../../ui/sections';
import { useSectionStore } from '../../ui/shell/section-store';

/** Dokąd prowadzi ten czasownik. Jedna nazwa, żeby przejście i rejestr nie rozjechały się. */
export const LAB: Section = 'lab' as Section;

/**
 * Zakłada zestaw dla tego agenta i przechodzi do Labu.
 *
 * Nazwa zestawu jest nazwą agenta i to jest wybór: zestaw, który człowiek zakłada jednym
 * kliknięciem, nie ma jak dostać własnej nazwy, a „Review rubric" obok agenta „Review rubric"
 * czyta się jak jedna rzecz — bo w tej chwili nią jest. Przemianować go da się później.
 *
 * Kandydatki pisze **ten sam agent**, którego mierzymy, i tylko dlatego, że to jest jedyny,
 * jakiego ta chwila zna. Materiał bierze się z PROJEKTU, nie z jego definicji
 * (`lab::cases::ask_for_cases`), więc tautologii to nie tworzy — a kogo innego wybrać, mówi
 * się potem, w Labie.
 */
export function evaluateAgent(id: string, name: string): Promise<void> {
  const made = useLab.getState().create(name, { kind: 'agent', id }, id);
  useSectionStore.getState().go(LAB);
  return made;
}

/**
 * To samo dla umiejętności — z dwiema różnicami, które są całym sensem tego wariantu.
 *
 * PIERWSZA: zestaw umiejętności rodzi się z **dwiema** kolumnami, bez niej i z nią
 * (`commands::lab::first_columns`), bo to jest całe pytanie, które zadaje się o umiejętność.
 * Zestaw z jedną kolumną nie umie na nie odpowiedzieć.
 *
 * DRUGA: umiejętność sama nie pracuje, więc ktoś musi ją ponieść. Bierzemy pierwszego agenta
 * biblioteki i **dowiadujemy się o nim tutaj**, a nie z ekranu umiejętności: tamten ekran nie
 * ma powodu wiedzieć, kogo człowiek zapisał, a przycisk pytający o to sam byłby drugim
 * miejscem, w którym mieszka odpowiedź „kogo mam" (niezmiennik 13).
 *
 * Bez ani jednego agenta zestawu nie zakładamy i mówimy to wprost. Ciche przejście do pustego
 * Labu jest tym samym, co przycisk, który nie robi nic.
 */
export async function evaluateSkill(name: string): Promise<void> {
  const lab = useLab.getState();
  if (lab.agents.length === 0) await lab.load();
  const carrier = useLab.getState().agents[0];
  useSectionStore.getState().go(LAB);
  if (carrier === undefined) {
    useLab.setState({
      said: 'A skill does not work on its own. Save an agent first, over in Agents.',
    });
    return;
  }
  await useLab.getState().create(name, { kind: 'skill', name }, carrier.id);
}
