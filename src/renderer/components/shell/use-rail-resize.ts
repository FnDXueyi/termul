import { useCallback, useEffect, useRef } from 'react'
import { SHELL_LAYOUT_BOUNDS, useRailWidth, useShellLayoutStore } from '@/stores/shell-layout-store'

/** Keyboard resize step (px). */
export const RAIL_RESIZE_STEP = 16

interface RailResize {
  width: number
  onResizeMouseDown: (event: React.MouseEvent<HTMLElement>) => void
  onResizeKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void
}

/**
 * Pointer drag and keyboard resize of the expanded Rail. The width lives in
 * the shell-layout store, which clamps it to 180-360 and persists it.
 */
export function useRailResize(): RailResize {
  const width = useRailWidth()
  const cleanupRef = useRef<(() => void) | null>(null)

  const finishDrag = useCallback(() => {
    document.body.style.userSelect = ''
    document.body.style.cursor = ''
    cleanupRef.current?.()
    cleanupRef.current = null
  }, [])

  const onResizeMouseDown = useCallback(
    (event: React.MouseEvent<HTMLElement>) => {
      event.preventDefault()
      finishDrag()
      const startX = event.clientX
      const startWidth = useShellLayoutStore.getState().railWidth
      document.body.style.userSelect = 'none'
      document.body.style.cursor = 'col-resize'

      const onMouseMove = (moveEvent: MouseEvent): void => {
        useShellLayoutStore.getState().setRailWidth(startWidth + moveEvent.clientX - startX)
      }

      document.addEventListener('mousemove', onMouseMove)
      document.addEventListener('mouseup', finishDrag)
      window.addEventListener('blur', finishDrag)
      cleanupRef.current = () => {
        document.removeEventListener('mousemove', onMouseMove)
        document.removeEventListener('mouseup', finishDrag)
        window.removeEventListener('blur', finishDrag)
      }
    },
    [finishDrag]
  )

  const onResizeKeyDown = useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    const { setRailWidth, railWidth } = useShellLayoutStore.getState()
    switch (event.key) {
      case 'ArrowLeft':
        setRailWidth(railWidth - RAIL_RESIZE_STEP)
        break
      case 'ArrowRight':
        setRailWidth(railWidth + RAIL_RESIZE_STEP)
        break
      case 'Home':
        setRailWidth(SHELL_LAYOUT_BOUNDS.railWidth.min)
        break
      case 'End':
        setRailWidth(SHELL_LAYOUT_BOUNDS.railWidth.max)
        break
      default:
        return
    }
    event.preventDefault()
  }, [])

  // A drag still in flight when the Rail goes away must not leave listeners.
  useEffect(() => finishDrag, [finishDrag])

  return { width, onResizeMouseDown, onResizeKeyDown }
}
