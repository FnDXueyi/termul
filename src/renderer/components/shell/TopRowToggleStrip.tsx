import { COLLAPSED_WIDTH } from '@/components/shell/ShellRail'
import { TrafficLightClearance } from '@/components/shell/TopRowChrome'
import { RailToggleButton, SidePanelToggleButton } from '@/components/shell/TopRowToggles'
import { useTopRowPlatform } from '@/components/shell/top-row-context'
import { titlebarNoDragStyle } from '@/components/TitlebarPanelToggles'

/** Width the macOS traffic lights need from the window edge. */
const TRAFFIC_LIGHT_CLEARANCE = 80
/** Gap between the Rail and the Side panel stack (`gap-2`). */
const SHELL_GAP = 8

/**
 * The traffic lights end near x=66; the collapsed Rail (48px) and the shell
 * gap (8px) already sit under them, so the strip only adds the rest.
 */
export const STRIP_CLEARANCE = TRAFFIC_LIGHT_CLEARANCE - COLLAPSED_WIDTH - SHELL_GAP

/**
 * Top row part above the Side panel column while the Rail is collapsed: the
 * Rail and Side panel toggles, after the macOS clearance. Stays when the Side
 * panel is hidden so the toggles remain reachable. The row root, the clearance
 * and the empty space are the drag region on desktop.
 */
export function TopRowToggleStrip(): React.JSX.Element {
  const platform = useTopRowPlatform()
  return (
    <div
      className="flex h-10 shrink-0 items-center"
      data-testid="top-row-toggle-strip"
      data-tauri-drag-region={platform.dragRegions ? true : undefined}
    >
      <TrafficLightClearance trafficLights={platform.trafficLights} widthPx={STRIP_CLEARANCE} />
      <div
        className={
          platform.dragRegions
            ? 'flex h-full items-center'
            : 'flex h-full items-center gap-0.5 pl-2'
        }
        style={titlebarNoDragStyle}
      >
        <RailToggleButton />
        <SidePanelToggleButton />
      </div>
      <div
        className="h-full min-w-0 flex-1"
        data-tauri-drag-region={platform.dragRegions ? true : undefined}
      />
    </div>
  )
}
