/* WF-07/WF-10: Suggested is presentation, never an automatic command.
 * Start travels with a backend request ID; Stop consumes backend-bound human consent.
 * Old saved rows may still contain auto:true and must remain inert on reopening.
 */
export interface Starting {
  readonly command: string;
  readonly agent: string;
}

/** Explicit compatibility guard for old conversation streams. Manual buttons remain manual. */
export function autoStarts(_batch: readonly unknown[]): readonly Starting[] {
  return [];
}
