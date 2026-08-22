import { invoke } from '@tauri-apps/api/core';
import type { ApplyRequest, ImportPreview, ImportReceipt } from './setup';

export function scanSetup(workspace: string): Promise<ImportPreview> {
  return invoke<ImportPreview>('scan_setup', { workspace });
}

export function applySetup(request: ApplyRequest): Promise<ImportReceipt> {
  return invoke<ImportReceipt>('apply_setup', { request });
}
