import { useCallback, useEffect, useRef } from 'react'
import { SHELL_LAYOUT_BOUNDS, useShellLayoutStore } from '@/stores/shell-layout-store'

const KEY_STEP = 16
const { min: GRAPH_MIN_HEIGHT, max: GRAPH_MAX_HEIGHT } = SHELL_LAYOUT_BOUNDS.graphHeight

interface GraphResize {
  min: number
  max: number
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void
  onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void
}

/**
 * Pointer drag and keyboard resize of the docked Graph's top edge. The height
 * lives in the shell-layout store (which clamps it and persists it); dragging
 * up makes the Graph taller. Listeners are removed on release, blur and unmount.
 */
export function useGraphResize(height: number): GraphResize {
  const setGraphHeight = useShellLayoutStore((state) => state.setGraphHeight)
  const dragRef = useRef<{ startY: number; startHeight: number } | null>(null)
  const cleanupRef = useRef<(() => void) | null>(null)

  const finishDrag = useCallback(() => {
    dragRef.current = null
    document.body.style.userSelect = ''
    cleanupRef.current?.()
    cleanupRef.current = null
  }, [])

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      finishDrag()
      document.body.style.userSelect = 'none'
      dragRef.current = { startY: event.clientY, startHeight: height }

      const onMove = (move: PointerEvent) => {
        const drag = dragRef.current
        if (!drag) return
        setGraphHeight(drag.startHeight - (move.clientY - drag.startY))
      }

      document.addEventListener('pointermove', onMove)
      document.addEventListener('pointerup', finishDrag)
      document.addEventListener('pointercancel', finishDrag)
      window.addEventListener('blur', finishDrag)
      cleanupRef.current = () => {
        document.removeEventListener('pointermove', onMove)
        document.removeEventListener('pointerup', finishDrag)
        document.removeEventListener('pointercancel', finishDrag)
        window.removeEventListener('blur', finishDrag)
      }
    },
    [height, setGraphHeight, finishDrag]
  )

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLElement>) => {
      switch (event.key) {
        case 'ArrowUp':
          setGraphHeight(height + KEY_STEP)
          break
        case 'ArrowDown':
          setGraphHeight(height - KEY_STEP)
          break
        case 'Home':
          setGraphHeight(GRAPH_MIN_HEIGHT)
          break
        case 'End':
          setGraphHeight(GRAPH_MAX_HEIGHT)
          break
        default:
          return
      }
      event.preventDefault()
    },
    [height, setGraphHeight]
  )

  useEffect(() => finishDrag, [finishDrag])

  return { min: GRAPH_MIN_HEIGHT, max: GRAPH_MAX_HEIGHT, onPointerDown, onKeyDown }
}
