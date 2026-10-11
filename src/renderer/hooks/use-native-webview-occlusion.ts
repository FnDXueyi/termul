import { useEffect, useSyncExternalStore } from 'react'
import { isTauriContext } from '@/lib/tauri-runtime'

/**
 * Native browser webviews paint above the DOM, so a shell popover would be
 * drawn underneath them. Popovers register here while open; `useBrowserWebview`
 * reads the count and treats an occluded webview as not visible, so overlapping
 * popovers hide the webviews once and the last one to close reveals only the
 * webviews whose pane shows them.
 */

let occlusionCount = 0
const listeners = new Set<() => void>()

function emit(): void {
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): boolean {
  return occlusionCount > 0
}

/** Registers the caller as an occluder while `open` is true. No-op outside Tauri. */
export function useNativeWebviewOcclusion(open: boolean): void {
  useEffect(() => {
    if (!open || !isTauriContext()) return
    occlusionCount += 1
    emit()
    return () => {
      occlusionCount = Math.max(0, occlusionCount - 1)
      emit()
    }
  }, [open])
}

/** True while at least one occluder is open. */
export function useNativeWebviewOccluded(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

/** @internal Test-only: drop every registered occluder. */
export function resetNativeWebviewOcclusionForTests(): void {
  occlusionCount = 0
  emit()
}
