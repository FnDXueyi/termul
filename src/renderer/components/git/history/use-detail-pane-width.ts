import { useCallback, useEffect, useRef, useState } from 'react'

export const DETAIL_PANE_MIN_WIDTH = 320
export const DETAIL_PANE_MAX_WIDTH = 560
const DEFAULT_WIDTH = 400
const KEY_STEP = 16
const STORAGE_KEY = 'termul.git-history.detail-width'

function clampWidth(width: number): number {
  return Math.round(Math.min(DETAIL_PANE_MAX_WIDTH, Math.max(DETAIL_PANE_MIN_WIDTH, width)))
}

function loadWidth(): number {
  if (typeof localStorage === 'undefined') return DEFAULT_WIDTH
  const stored = Number(localStorage.getItem(STORAGE_KEY))
  return Number.isFinite(stored) && stored > 0 ? clampWidth(stored) : DEFAULT_WIDTH
}

function saveWidth(width: number): void {
  if (typeof localStorage === 'undefined') return
  localStorage.setItem(STORAGE_KEY, String(width))
}

interface DetailPaneWidth {
  width: number
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void
  onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void
}

/**
 * Width of the Git History detail pane (320–560, kept in localStorage). The
 * handle sits on the pane's left edge: dragging left makes the pane wider.
 * Listeners are removed on release, blur and unmount.
 */
export function useDetailPaneWidth(): DetailPaneWidth {
  const [width, setWidth] = useState(loadWidth)
  const widthRef = useRef(width)
  widthRef.current = width
  const cleanupRef = useRef<(() => void) | null>(null)

  const finishDrag = useCallback(() => {
    if (!cleanupRef.current) return
    document.body.style.userSelect = ''
    cleanupRef.current()
    cleanupRef.current = null
    saveWidth(widthRef.current)
  }, [])

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      finishDrag()
      document.body.style.userSelect = 'none'
      const startX = event.clientX
      const startWidth = widthRef.current
      const onMove = (move: PointerEvent) =>
        setWidth(clampWidth(startWidth - (move.clientX - startX)))
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
    [finishDrag]
  )

  const onKeyDown = useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    const current = widthRef.current
    const next =
      event.key === 'ArrowLeft'
        ? current + KEY_STEP
        : event.key === 'ArrowRight'
          ? current - KEY_STEP
          : event.key === 'Home'
            ? DETAIL_PANE_MIN_WIDTH
            : event.key === 'End'
              ? DETAIL_PANE_MAX_WIDTH
              : null
    if (next === null) return
    event.preventDefault()
    event.stopPropagation()
    const clamped = clampWidth(next)
    setWidth(clamped)
    saveWidth(clamped)
  }, [])

  useEffect(() => finishDrag, [finishDrag])

  return { width, onPointerDown, onKeyDown }
}
