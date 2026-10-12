import { GitHistoryPanel } from '@/components/git/GitHistoryPanel'
import { useGraphResize } from '@/components/shell/use-graph-resize'
import { cn } from '@/lib/utils'
import { useGraphHeight, useGraphOpen, useShellLayoutStore } from '@/stores/shell-layout-store'

interface GraphSectionProps {
  cwd: string
  isVisible: boolean
}

/**
 * The commit Graph docked under the Changes panel: a hairline on its top edge,
 * a resizable height kept in the shell-layout store, and a header that
 * collapses it to the header alone. It keeps its own scroll and never takes
 * more than 60% of the panel so the change list keeps room.
 */
export function GraphSection({ cwd, isVisible }: GraphSectionProps): React.JSX.Element {
  const open = useGraphOpen()
  const height = useGraphHeight()
  const setGraphOpen = useShellLayoutStore((state) => state.setGraphOpen)
  const { min, max, onPointerDown, onKeyDown } = useGraphResize(height)

  return (
    <section
      aria-label="Git graph"
      className={cn(
        'relative flex shrink-0 flex-col overflow-hidden border-t border-border',
        open && 'max-h-[60%]'
      )}
      style={open ? { height } : undefined}
    >
      {open && (
        <button
          type="button"
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize graph"
          aria-valuenow={height}
          aria-valuemin={min}
          aria-valuemax={max}
          onPointerDown={onPointerDown}
          onKeyDown={onKeyDown}
          className="absolute inset-x-0 top-0 z-10 h-1 cursor-row-resize touch-none bg-transparent hover:bg-border focus-visible:bg-border focus-visible:outline-none"
        />
      )}
      <GitHistoryPanel
        cwd={cwd}
        isVisible={isVisible}
        variant="narrow"
        collapsed={!open}
        onToggleCollapsed={() => setGraphOpen(!open)}
      />
    </section>
  )
}
