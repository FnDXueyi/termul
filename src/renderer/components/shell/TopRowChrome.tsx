import { useEffect, useState } from 'react'
import { Copy, Minus, Square, X } from '@/components/icons'
import { titlebarNoDragStyle } from '@/components/TitlebarPanelToggles'
import { windowApi } from '@/lib/api'

const windowControlClass =
  'h-full px-3 hover:bg-secondary/80 inline-flex items-center focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-inset cursor-pointer'

/**
 * Windows/Linux window controls (minimize, maximize or restore, close): the
 * only place the shell renders them, in whichever top row surface owns them.
 * Renders nothing unless `windowControls`.
 */
export function WindowControls({
  windowControls
}: {
  windowControls: boolean
}): React.JSX.Element | null {
  const [isMaximized, setIsMaximized] = useState(false)

  useEffect(() => {
    if (!windowControls) return
    return windowApi.onMaximizeChange((maximized) => {
      setIsMaximized(maximized)
    })
  }, [windowControls])

  if (!windowControls) return null

  return (
    <div
      className="relative z-[100] flex h-full shrink-0 items-center"
      style={titlebarNoDragStyle}
      data-testid="window-controls"
    >
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation()
          void windowApi.minimize()
        }}
        className={windowControlClass}
        title="Minimize"
        aria-label="Minimize window"
      >
        <Minus size={16} />
      </button>

      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation()
          void windowApi.toggleMaximize().then((result) => {
            if (!result.success) {
              console.error(`Failed to toggle maximize: ${result.error ?? 'unknown error'}`)
            }
          })
        }}
        className={windowControlClass}
        title={isMaximized ? 'Restore' : 'Maximize'}
        aria-label={isMaximized ? 'Restore window' : 'Maximize window'}
      >
        {isMaximized ? <Copy size={14} /> : <Square size={14} />}
      </button>

      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation()
          void windowApi.close()
        }}
        className="h-full px-3 hover:bg-destructive-fill hover:text-destructive-foreground inline-flex items-center focus:outline-none focus-visible:ring-2 focus-visible:ring-destructive cursor-pointer"
        title="Close"
        aria-label="Close window"
      >
        <X size={16} />
      </button>
    </div>
  )
}

/**
 * Spacer that clears the native macOS traffic lights (end near x=66). It is its
 * own drag handle. Renders nothing unless `trafficLights`. `widthPx` replaces
 * the default 80px when part of the clearance is already covered by a
 * neighbour (the collapsed Rail).
 */
export function TrafficLightClearance({
  trafficLights,
  widthPx
}: {
  trafficLights: boolean
  widthPx?: number
}): React.JSX.Element | null {
  if (!trafficLights) return null
  return (
    <div
      className={widthPx === undefined ? 'w-[80px] shrink-0' : 'shrink-0'}
      style={widthPx === undefined ? undefined : { width: widthPx }}
      data-testid="traffic-light-clearance"
      data-tauri-drag-region
    />
  )
}
