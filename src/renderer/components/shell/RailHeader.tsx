import { TrafficLightClearance } from '@/components/shell/TopRowChrome'
import { RailToggleButton, SidePanelToggleButton } from '@/components/shell/TopRowToggles'
import { useTopRowPlatform } from '@/components/shell/top-row-context'
import { titlebarNoDragStyle } from '@/components/TitlebarPanelToggles'

/**
 * Top row part of the expanded Rail: macOS traffic-light clearance, then the
 * Rail and Side panel toggles. The row root and the empty space after the
 * toggles are the window drag region on desktop.
 */
export function RailHeader(): React.JSX.Element {
  const platform = useTopRowPlatform()
  return (
    <div
      className="flex h-10 shrink-0 items-center"
      data-testid="rail-header"
      data-tauri-drag-region={platform.dragRegions ? true : undefined}
    >
      <TrafficLightClearance trafficLights={platform.trafficLights} />
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
    </div>
  )
}
