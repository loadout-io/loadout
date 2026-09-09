import type { ChangeEventHandler, ReactElement, ReactNode } from 'react';
import { Fragment } from 'react';

export interface TickProps {
  readonly label: ReactNode;
  readonly name?: string;
  readonly describedBy?: string;
  readonly checked: boolean;
  readonly onChange: ChangeEventHandler<HTMLInputElement>;
  readonly disabled?: boolean;
  readonly className?: string;
  readonly boxClass?: string;
  readonly field?: string;
  /** 2026-09-09 (UX-2) — zachowuje uchwyt płytkiej wyroczni handlera w CriteriaRow. */
  readonly 'data-field'?: string;
  readonly 'data-row'?: string;
  readonly children?: ReactNode;
  readonly id?: string;
}

/** 2026-09-09 (UX-2) — prawdziwy input zachowuje klawiaturę i semantykę; CSS zmienia rysunek. */
export function Tick({
  label,
  name,
  describedBy,
  checked,
  onChange,
  disabled = false,
  className,
  boxClass,
  field,
  'data-field': dataField,
  'data-row': row,
  children,
  id,
}: TickProps): ReactElement {
  const input = (
    <input
      id={id}
      type="checkbox"
      className={boxClass === undefined ? 'tick' : `tick ${boxClass}`}
      aria-label={name}
      aria-describedby={describedBy}
      data-field={field ?? dataField}
      checked={checked}
      disabled={disabled}
      onChange={onChange}
    />
  );
  const words = (
    <>
      {label}
      {children}
    </>
  );

  /* 2026-09-09 (UX-2) — trzy siatki potrzebują pola i etykiety jako rodzeństwa. Opakowanie
   * zmieniłoby ich kolumny, więc `id` wybiera natywną parę `input` + `label[for]`. */
  return id === undefined ? (
    <label className={className} data-row={row}>
      {input}
      {words}
    </label>
  ) : (
    <Fragment>
      {input}
      <label className={className} htmlFor={id}>
        {words}
      </label>
    </Fragment>
  );
}
