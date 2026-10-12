import { useCallback, useEffect, useRef } from 'react'
import {
  SHELL_LAYOUT_BOUNDS,
  useShellLayoutStore,
  useSidePanelWidth
} from '@/stores/shell-layout-store'

const { min: MIN_WIDTH, max: MAX_WIDTH } = SHELL_LAYOUT_BOUNDS.sidePanelWidth
const KEY_STEP = 16

interface SidePanelResize {
  width: number
  min: number
  max: number
  onResizeMouseDown: (event: React.MouseEvent<HTMLButtonElement>) => void
  onResizeKeyDown: (event: React.KeyboardEvent<HTMLButtonElement>) => void
}

/**
 * Side panel width with pointer drag and keyboard resize for a panel against
 * the left edge (dragging right widens it). The width lives in the shell-layout
 * store, which clamps to its bounds and ignores writes until hydrated.
 */
export function useSidePanelResize(): SidePanelResize {
  const width = useSidePanelWidth()
  const dragCleanupRef = useRef<(() => void) | null>(null)

  const endDrag = useCallback(() => {
    document.body.style.userSelect = ''
    dragCleanupRef.current?.()
    dragCleanupRef.current = null
  }, [])

  const onResizeMouseDown = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      event.preventDefault()
      endDrag()
      document.body.style.userSelect = 'none'
      const startX = event.clientX
      const startWidth = useShellLayoutStore.getState().sidePanelWidth

      const onMouseMove = (moveEvent: MouseEvent): void => {
        useShellLayoutStore.getState().setSidePanelWidth(startWidth + moveEvent.clientX - startX)
      }
      document.addEventListener('mousemove', onMouseMove)
      document.addEventListener('mouseup', endDrag)
      window.addEventListener('blur', endDrag)
      dragCleanupRef.current = () => {
        document.removeEventListener('mousemove', onMouseMove)
        document.removeEventListener('mouseup', endDrag)
        window.removeEventListener('blur', endDrag)
      }
    },
    [endDrag]
  )

  const onResizeKeyDown = useCallback((event: React.KeyboardEvent<HTMLButtonElement>) => {
    const { setSidePanelWidth, sidePanelWidth } = useShellLayoutStore.getState()
    switch (event.key) {
      case 'Home':
        event.preventDefault()
        setSidePanelWidth(MIN_WIDTH)
        return
      case 'End':
        event.preventDefault()
        setSidePanelWidth(MAX_WIDTH)
        return
      case 'ArrowLeft':
        event.preventDefault()
        setSidePanelWidth(sidePanelWidth - KEY_STEP)
        return
      case 'ArrowRight':
        event.preventDefault()
        setSidePanelWidth(sidePanelWidth + KEY_STEP)
        return
    }
  }, [])

  // A drag in flight when the panel unmounts must not leak its listeners.
  useEffect(() => endDrag, [endDrag])

  return { width, min: MIN_WIDTH, max: MAX_WIDTH, onResizeMouseDown, onResizeKeyDown }
}
