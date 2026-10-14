import { ShellBrandMark } from '@/components/shell/ShellBrandMark'
import { TrafficLightClearance } from '@/components/shell/TopRowChrome'
import { useTopRowPlatform } from '@/components/shell/top-row-context'

/**
 * Top row part of the expanded Rail: macOS traffic-light clearance, then the
 * Termul mark. The row is the window drag region on desktop. The Rail toggle
 * lives in the footer (bottom-left), at the same point in both Rail states.
 */
export function RailHeader(): React.JSX.Element {
  const platform = useTopRowPlatform()
  return (
    <div
      className={
        platform.dragRegions
          ? 'flex h-10 shrink-0 items-center'
          : 'flex h-10 shrink-0 items-center pl-2'
      }
      data-testid="rail-header"
      data-tauri-drag-region={platform.dragRegions ? true : undefined}
    >
      <TrafficLightClearance trafficLights={platform.trafficLights} />
      <ShellBrandMark dragRegion={platform.dragRegions} />
    </div>
  )
}
