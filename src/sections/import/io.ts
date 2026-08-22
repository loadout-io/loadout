import { invoke } from '@tauri-apps/api/core';
import type { AnalysisRequest, ApplyRequest, ImportPreview, ImportReceipt } from './setup';

export function scanSetup(workspace: string): Promise<ImportPreview> {
  return invoke<ImportPreview>('scan_setup', { workspace });
}

export function applySetup(request: ApplyRequest): Promise<ImportReceipt> {
  return invoke<ImportReceipt>('apply_setup', { request });
}

export function analyzeSetup(request: AnalysisRequest): Promise<ImportPreview | null> {
  return invoke<ImportPreview | null>('analyze_setup', { request });
}

export function stopSetupAnalysis(): Promise<void> {
  return invoke<void>('stop_setup_analysis');
}
