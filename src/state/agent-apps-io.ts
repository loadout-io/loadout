import { invoke } from '@tauri-apps/api/core';

/** Jedyna krawędź ulotnej migawki lokalnych aplikacji agentów. */
export function checkAgentApps(): Promise<unknown> {
  return invoke('check_agent_apps');
}
