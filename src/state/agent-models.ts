import { invoke } from '@tauri-apps/api/core';
import type { Vendor } from './agents';
export interface ModelChoice {
  id: string;
  displayName: string;
  description: string;
  isDefault: boolean;
  hidden: boolean;
  aliases: string[];
  efforts: string[];
}
export interface ModelCatalog {
  models: ModelChoice[];
}
export async function listAgentModels(vendor: Vendor): Promise<ModelCatalog> {
  const result = await invoke<ModelCatalog>('list_agent_models', { vendor });
  if (
    !Array.isArray(result?.models) ||
    !result.models.length ||
    result.models.some(
      (m) =>
        !m ||
        typeof m.id !== 'string' ||
        !m.id ||
        typeof m.displayName !== 'string' ||
        typeof m.description !== 'string' ||
        typeof m.hidden !== 'boolean' ||
        typeof m.isDefault !== 'boolean' ||
        !Array.isArray(m.aliases) ||
        !m.aliases.every((a) => typeof a === 'string') ||
        !Array.isArray(m.efforts) ||
        !m.efforts.every((e) => typeof e === 'string'),
    )
  )
    throw new Error('The app returned an unreadable model list.');
  return result;
}
