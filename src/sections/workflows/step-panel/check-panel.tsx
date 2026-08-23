/* Panel kafelka „sprawdź" — SZKIELET. Fazę implementacji T-89 czeka tu cała treść.
 *
 * Istnieje z tego samego powodu, co `serve-panel.tsx` i `checkpoint-panel.tsx`, i ten powód
 * jest niezmiennikiem 16: kafelek, którego nie da się wypełnić, jest kafelkiem, którego nie da
 * się uruchomić — a człowiek dowiaduje się o tym dopiero w środku biegu. Ten kafelek ma dwa
 * pola, bez których nie znaczy nic: komendę i to, po czym poznać, że naprawdę pobiegła.
 *
 * Dlaczego pusty fragment, a nie od razu formularz: ten plik powstaje w fazie, która ma
 * DOWIEŚĆ, że kryteria są czerwone. Panel, który już rysuje pola, zaświeciłby je na zielono,
 * zanim ktokolwiek napisał implementację, a wtedy kryterium nie poświadcza niczego
 * (`AGENTS.md` §2a punkt 5).
 *
 * Ramki tu nie ma: rysuje ją `PanelForStep`, jedną, dla wszystkich paneli.
 */
import type { ReactElement } from 'react';
import type { CheckStep } from '../../../state/workflows';

/** Cztery pola kafelka sprawdzenia. Ten kafelek nie ma agenta, więc nie dziedziczy niczego —
 * nie ma tu nadpisań ani wartości efektywnych, są same pola kroku. */
export type CheckFields = Partial<
  Pick<CheckStep, 'name' | 'command' | 'proof' | 'folder' | 'whenItFails'>
>;

export interface CheckPanelProps {
  step: CheckStep;
  onEditStep: (fields: CheckFields) => void;
}

export function CheckPanel(_props: CheckPanelProps): ReactElement {
  return <></>;
}
