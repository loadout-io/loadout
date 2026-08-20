import { invoke } from '@tauri-apps/api/core';

export interface TriggerHit {
  readonly id: string;
  readonly identifier: string;
  readonly title: string;
  readonly url: string;
  readonly body: string;
  readonly updatedAt: string;
}

export function checkTrigger(slug: string): Promise<TriggerHit | null> {
  return invoke<TriggerHit | null>('check_trigger', { slug });
}
