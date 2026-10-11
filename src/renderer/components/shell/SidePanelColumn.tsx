import { lazy, Suspense } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Plus } from '@/components/icons'
import { PANEL_HEADER_CLASS, PANEL_ICON_BUTTON_CLASS } from '@/components/ui/panel-styles'
import { ShellSkeleton } from '@/layouts/workspace-layout/ShellSkeleton'
import { cn } from '@/lib/utils'
import { selectChangedFileCount, useGitStatusStore } from '@/stores/git-status-store'
import {
  type ShellLastColumnTab,
  useLastColumnTab,
  useShellLayoutStore
} from '@/stores/shell-layout-store'
import { useSidebarVisible } from '@/stores/sidebar-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import type { Project } from '@/types/project'
import { ChangesTabBody } from './ChangesTabBody'
import { ChatsList } from './ChatsList'
import { ColumnTabs, columnPanelId, columnTabId } from './ColumnTabs'
import { useColumnWorkingCopy } from './use-column-working-copy'
import { useSidePanelResize } from './use-side-panel-resize'
import { WorkspaceSwitcher } from './WorkspaceSwitcher'

const FileExplorer = lazy(() =>
  import('@/components/file-explorer/FileExplorer').then((m) => ({ default: m.FileExplorer }))
)

interface SidePanelColumnProps {
  activeProject: Project
  /**
   * The header is part of the window top row (expanded Rail on desktop): it
   * becomes a drag region. Defaults to the classic, non-draggable header.
   */
  dragRegion?: boolean
}

const TAB_LABELS: Record<ShellLastColumnTab, string> = {
  chats: 'Chats',
  explorer: 'Explorer',
  changes: 'Changes'
}

/**
 * Shell Side panel (flagged shell only): a header with the New chat button,
 * the Chats / Explorer / Changes Column tabs and the selected tab's body. The
 * last tab is remembered per project. The tabs render only while the Rail is
 * expanded; collapsed, the Rail's own icons select the tab. Mounted through
 * `ExplorerColumn`'s `sidePanel` slot, never in SSH mode.
 */
export function SidePanelColumn({
  activeProject,
  dragRegion = false
}: SidePanelColumnProps): React.JSX.Element {
  const projectId = activeProject.id
  const { cwd } = useColumnWorkingCopy(activeProject)
  const railExpanded = useSidebarVisible()
  const dragAttr = dragRegion ? true : undefined

  const selected: ShellLastColumnTab = useLastColumnTab(projectId) ?? 'chats'
  const changedCount = useGitStatusStore(selectChangedFileCount(cwd))
  const { width, min, max, onResizeMouseDown, onResizeKeyDown } = useSidePanelResize()
  const pathname = useLocation().pathname
  const navigate = useNavigate()

  // Same as the Rail's New chat: the launcher overlay only renders on the
  // workspace route, so leave any child route first.
  const handleNewChat = (): void => {
    const open = (): void => {
      const paneId = useWorkspaceStore.getState().activePaneId
      if (paneId) useWorkspaceStore.getState().showAgentLauncher(paneId)
    }
    if (pathname !== '/') {
      navigate('/')
      requestAnimationFrame(open)
    } else {
      open()
    }
  }

  const handleSelectTab = (tab: ShellLastColumnTab): void => {
    useShellLayoutStore.getState().setLastColumnTab(projectId, tab)
  }

  const handleOpenGitChanges = (path: string): void => {
    useWorkspaceStore.getState().addGitTab(path, useWorkspaceStore.getState().activePaneId)
  }

  return (
    <div
      className={cn(
        'relative flex h-full min-w-0 flex-shrink-0 flex-col overflow-hidden rounded-xl',
        'bg-background text-foreground'
      )}
      style={{ width }}
    >
      <div className={PANEL_HEADER_CLASS} data-tauri-drag-region={dragAttr}>
        <WorkspaceSwitcher project={activeProject} />
        <button
          type="button"
          onClick={handleNewChat}
          className={PANEL_ICON_BUTTON_CLASS}
          title="New chat"
          aria-label="New chat"
        >
          <Plus size={14} aria-hidden="true" />
        </button>
      </div>

      {railExpanded && (
        <ColumnTabs selected={selected} onSelect={handleSelectTab} changedCount={changedCount} />
      )}

      <div
        role="tabpanel"
        id={columnPanelId(selected)}
        {...(railExpanded
          ? { 'aria-labelledby': columnTabId(selected) }
          : { 'aria-label': TAB_LABELS[selected] })}
        className="flex min-h-0 flex-1 flex-col"
      >
        {selected === 'chats' && <ChatsList project={activeProject} />}
        {selected === 'explorer' && (
          <Suspense fallback={<ShellSkeleton />}>
            <FileExplorer side="left" embedded />
          </Suspense>
        )}
        {selected === 'changes' && (
          <ChangesTabBody cwd={cwd} onOpenGitChanges={handleOpenGitChanges} />
        )}
      </div>

      <button
        type="button"
        onMouseDown={onResizeMouseDown}
        onKeyDown={onResizeKeyDown}
        className="absolute right-0 top-0 h-full w-1 cursor-col-resize bg-transparent transition-colors duration-150 ease-out hover:bg-border focus-visible:bg-muted-foreground/40 focus-visible:outline-none active:bg-muted-foreground/40"
        title="Drag to resize side panel"
        aria-label="Resize side panel"
        role="separator"
        aria-valuenow={width}
        aria-valuemin={min}
        aria-valuemax={max}
      />
    </div>
  )
}
