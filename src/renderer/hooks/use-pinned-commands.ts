import { PersistenceKeys } from '@shared/types/persistence.types'
import { useCallback, useEffect } from 'react'
import { persistenceApi } from '@/lib/api'
import { usePinnedCommandsStore } from '@/stores/pinned-commands-store'

export function usePinnedCommandsLoader(): void {
  const setPinned = usePinnedCommandsStore((state) => state.setPinned)

  useEffect(() => {
    async function load(): Promise<void> {
      const result = await persistenceApi.read<string[]>(PersistenceKeys.pinnedCommands)
      // Guard against corrupt persisted data: only accept an array of strings.
      if (
        result.success &&
        Array.isArray(result.data) &&
        result.data.every((id) => typeof id === 'string')
      ) {
        setPinned(result.data)
      }
    }
    load()
  }, [setPinned])
}

export function useTogglePinnedCommand(): (commandId: string) => Promise<void> {
  const togglePinned = usePinnedCommandsStore((state) => state.togglePinned)

  return useCallback(
    async (commandId: string) => {
      togglePinned(commandId)
      // Persist after optimistic update
      const { pinnedCommandIds } = usePinnedCommandsStore.getState()
      await persistenceApi.write(PersistenceKeys.pinnedCommands, pinnedCommandIds)
    },
    [togglePinned]
  )
}

export function usePinnedCommandIds(): string[] {
  return usePinnedCommandsStore((state) => state.pinnedCommandIds)
}
