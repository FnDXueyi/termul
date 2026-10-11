import { RailButton } from '@/components/ActivityRail'
import { Edit2, FolderKanban, PanelLeft, SlidersHorizontal } from '@/components/icons'
import { ProjectSwitcher } from '@/components/shell/ProjectSwitcher'
import { RailColumnIcons } from '@/components/shell/RailColumnIcons'
import { useTopRowPlatform } from '@/components/shell/top-row-context'
import { RAIL_TOGGLE_LABEL, useRailToggle } from '@/components/shell/use-rail-toggle'
import { useShortcutLabel } from '@/components/shell/use-shortcut-label'
import { useSettingsModalStore, useSettingsModalView } from '@/stores/settings-modal-store'
import type { Project } from '@/types/project'

interface RailCollapsedProps {
  projects: Project[]
  activeProject: Project | undefined
  onSelectProject: (id: string) => void
  onNewProject: () => void
  onOpenCommandPalette: () => void
  onOpenCanvas: () => void
  canOpenCanvas: boolean
}

const NEED_PROJECT = 'Open a project first'

/**
 * Collapsed Rail body (48px): an empty top band (the Rail's part of the top
 * row), panel toggle, Project switcher, the Column tab icons (Chats, Explorer,
 * Changes), Search, Canvas, and Settings pinned at the bottom. No Theme or
 * Shortcuts buttons here.
 */
export function RailCollapsed({
  projects,
  activeProject,
  onSelectProject,
  onNewProject,
  onOpenCommandPalette,
  onOpenCanvas,
  canOpenCanvas
}: RailCollapsedProps): React.JSX.Element {
  const { expanded, toggle } = useRailToggle()
  const settingsView = useSettingsModalView()
  const platform = useTopRowPlatform()
  const searchShortcut = useShortcutLabel('commandPalette')
  const settingsShortcut = useShortcutLabel('settings')

  return (
    <div className="flex h-full w-12 shrink-0 flex-col items-center gap-1 pb-2">
      {/* Top row part: empty drag space; the toggles live in the strip above
          the Side panel column. */}
      <div
        className="h-10 w-full shrink-0"
        data-testid="rail-top-band"
        data-tauri-drag-region={platform.dragRegions ? true : undefined}
      />

      <RailButton
        label={RAIL_TOGGLE_LABEL}
        aria-label={RAIL_TOGGLE_LABEL}
        pressed={expanded}
        onClick={toggle}
      >
        <PanelLeft size={16} />
      </RailButton>

      <ProjectSwitcher
        projects={projects}
        activeProject={activeProject}
        onSelectProject={onSelectProject}
        onNewProject={onNewProject}
      />

      <RailColumnIcons activeProject={activeProject} />

      <RailButton
        label="Search"
        hint={searchShortcut}
        aria-label="Search"
        onClick={onOpenCommandPalette}
      >
        <FolderKanban size={16} />
      </RailButton>

      <RailButton
        label="Open canvas"
        hint={canOpenCanvas ? undefined : NEED_PROJECT}
        aria-label="Open canvas"
        disabled={!canOpenCanvas}
        onClick={onOpenCanvas}
      >
        <Edit2 size={16} />
      </RailButton>

      <div className="mt-auto">
        <RailButton
          label="Settings"
          hint={settingsShortcut}
          aria-label="Settings"
          pressed={settingsView === 'app'}
          onClick={() => useSettingsModalStore.getState().openApp()}
        >
          <SlidersHorizontal size={16} />
        </RailButton>
      </div>
    </div>
  )
}
