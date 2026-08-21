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
}

/** The redacted part of one trigger file which is safe to cross the IPC boundary. */
export type TriggerEntry = ConfiguredTriggerEntry | BrokenTriggerEntry;

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
}

/** Skeleton for T-65's honest red: the module exists, but the new command is not wired yet. */
export function listTriggers(): Promise<TriggerEntry[]> {
  throw new Error('not implemented');
}

/** Skeleton for T-65's honest red: state must not move until this command confirms the write. */
export function setTriggerEnabled(_slug: string, _enabled: boolean): Promise<TriggerEntry> {
  throw new Error('not implemented');
}

export function checkTrigger(slug: string): Promise<TriggerPoll> {
  return invoke<TriggerPoll>('check_trigger', { slug });
}
