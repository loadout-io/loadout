import { create } from 'zustand';
import type { Workspace } from '../../state/workspaces';

// Import ma właściciela z chwili otwarcia. Przełączenie projektu nie przekierowuje zapisu.
export const useProjectSetup = create<{
  destination: Workspace | null;
  revisions: Record<string, number>;
  open: (destination: Workspace) => void;
  close: () => void;
  imported: (folder: string) => void;
}>((set) => ({
  destination: null,
  revisions: {},
  open: (destination) => set({ destination }),
  close: () => set({ destination: null }),
  imported: (folder) =>
    set((state) => ({
      revisions: {
        ...state.revisions,
        [folder]: (state.revisions[folder] ?? 0) + 1,
      },
    })),
}));
