import { invoke } from '@tauri-apps/api/core';

export interface TriggerIssue {
  readonly id: string;
  readonly identifier: string;
  readonly title: string;
  readonly url: string;
  readonly body: string;
  readonly updatedAt: string;
}

/** The one-use token which lets Rust bind a pending delivery to its preallocated run. */
export interface TriggerClaim {
  readonly slug: string;
  readonly deliveryId: string;
  readonly workflow: string;
  readonly runId: string;
}

/** A durable issue delivery. Its timestamp and run id were minted before the webview saw it. */
export interface TriggerDelivery {
  readonly claim: TriggerClaim;
  readonly issue: TriggerIssue;
  readonly createdAt: number;
}

/** A valid redacted trigger. All four config fields exist together or not at all. */
export interface ConfiguredTriggerEntry {
  readonly slug: string;
  readonly source: string;
  readonly condition: string;
  readonly workflow: string;
  readonly enabled: boolean;
  readonly pollEveryMinutes: TriggerCadence;
  readonly hasApiKey: boolean;
  readonly problem?: never;
}

/** A malformed file has a name and a problem, never values invented for unreadable config. */
export interface BrokenTriggerEntry {
  readonly slug: string;
  readonly problem: string;
  readonly source?: never;
  readonly condition?: never;
  readonly workflow?: never;
  readonly enabled?: never;
  readonly pollEveryMinutes?: never;
  readonly hasApiKey?: never;
}

/** The redacted part of one trigger file which is safe to cross the IPC boundary. */
export type TriggerEntry = ConfiguredTriggerEntry | BrokenTriggerEntry;

/** The only polling cadences which the Linear form can honestly schedule. */
export type TriggerCadence = 1 | 5 | 15 | 60;

/** Everything Rust needs to create or update a trigger, including a one-way optional secret. */
export interface TriggerDraft {
  readonly source: string;
  readonly condition: string;
  readonly workflow: string;
  readonly pollEveryMinutes: TriggerCadence;
  readonly apiKey: string | null;
}

/** A redacted optimistic snapshot. The secret itself never returns to the webview. */
export interface TriggerSnapshot {
  readonly slug: string;
  readonly source: string;
  readonly condition: string;
  readonly workflow: string;
  readonly enabled: boolean;
  readonly pollEveryMinutes: TriggerCadence;
  readonly hasApiKey: boolean;
}

/** Rust, rather than window state, is authoritative for every polling decision. */
export type TriggerPoll =
  | { readonly status: 'busy' }
  | { readonly status: 'armed' }
  | { readonly status: 'pending'; readonly delivery: TriggerDelivery }
  | { readonly status: 'accepted'; readonly workflow: string; readonly receiptAt: number };

export interface TriggerIo {
  listTriggers(): Promise<TriggerEntry[]>;
  setTriggerEnabled(slug: string, enabled: boolean): Promise<TriggerEntry>;
  checkTrigger(slug: string): Promise<TriggerPoll>;
  createTrigger(draft: TriggerDraft): Promise<ConfiguredTriggerEntry>;
  updateTrigger(
    slug: string,
    expected: TriggerSnapshot,
    draft: TriggerDraft,
  ): Promise<ConfiguredTriggerEntry>;
  deleteTrigger(slug: string, expected: TriggerSnapshot): Promise<void>;
  testLinearConnection(slug: string | null, apiKey: string | null): Promise<void>;
}

/** The whole redacted library. Secrets remain in Rust and never enter this type. */
export function listTriggers(): Promise<TriggerEntry[]> {
  return invoke<TriggerEntry[]>('list_triggers');
}

/** Persist one switch. The returned entry is the file after its atomic rewrite. */
export function setTriggerEnabled(slug: string, enabled: boolean): Promise<TriggerEntry> {
  return invoke<TriggerEntry>('set_trigger_enabled', { slug, enabled });
}

export function checkTrigger(slug: string): Promise<TriggerPoll> {
  return invoke<TriggerPoll>('check_trigger', { slug });
}

/** Create is one request; the secret travels only inside the explicitly submitted draft. */
export function createTrigger(draft: TriggerDraft): Promise<ConfiguredTriggerEntry> {
  return invoke<ConfiguredTriggerEntry>('create_trigger', { draft });
}

/** The redacted snapshot lets Rust refuse a stale editor without returning the saved key. */
export function updateTrigger(
  slug: string,
  expected: TriggerSnapshot,
  draft: TriggerDraft,
): Promise<ConfiguredTriggerEntry> {
  return invoke<ConfiguredTriggerEntry>('update_trigger', { slug, expected, draft });
}

/** Rust confirms ledger cancellation and disk removal before the window removes its row. */
export function deleteTrigger(slug: string, expected: TriggerSnapshot): Promise<void> {
  return invoke<void>('delete_trigger', { slug, expected });
}

/** A dedicated viewer probe: it neither polls a trigger nor arms durable delivery state. */
export function testLinearConnection(slug: string | null, apiKey: string | null): Promise<void> {
  return invoke<void>('test_linear_connection', { slug, apiKey });
}
