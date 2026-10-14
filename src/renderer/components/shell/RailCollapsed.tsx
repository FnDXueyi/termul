import { Edit2, Search, SlidersHorizontal } from '@/components/icons'
import { RailColumnIcons } from '@/components/shell/RailColumnIcons'
import { RailProjectStack } from '@/components/shell/RailProjectStack'
import { RailToggleButton } from '@/components/shell/RailToggleButton'
import { RailButton } from '@/components/shell/rail-button'
import { ShellBrandMark } from '@/components/shell/ShellBrandMark'
import { SSHPopover } from '@/components/shell/SSHPopover'
import { useTopRowPlatform } from '@/components/shell/top-row-context'
import { useTrafficLightInset } from '@/components/shell/traffic-light-inset'
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
  onSSHConnect: (profileId: string) => void
  onSelectSSHProfile: (profileId: string) => void
  activeSSHProfileId: string | null
}

const NEED_PROJECT = 'Open a project first'

/**
 * Collapsed Rail body (48px): the top band (the Rail's part of the top row)
 * and the Termul mark, the project stack with status, the Column tab icons
 * (Chats, Explorer, Changes) in a track, Search, Canvas, SSH, then Settings
 * and the Rail toggle pinned at the bottom. No Theme or Shortcuts buttons
 * here.
 */
export function RailCollapsed({
  projects,
  activeProject,
  onSelectProject,
  onNewProject,
  onOpenCommandPalette,
  onOpenCanvas,
  canOpenCanvas,
  onSSHConnect,
  onSelectSSHProfile,
  activeSSHProfileId
}: RailCollapsedProps): React.JSX.Element {
  const settingsView = useSettingsModalView()
  const platform = useTopRowPlatform()
  const underTrafficLights = useTrafficLightInset() !== null
  const searchShortcut = useShortcutLabel('commandPalette')
  const settingsShortcut = useShortcutLabel('settings')

  return (
    <div className="flex h-full w-12 shrink-0 flex-col items-center gap-1 pb-2">
      {/* Top row part, drag space. Under the macOS traffic lights it stays
          empty and the Termul mark sits just below it; elsewhere the mark sits
          in it. */}
      <div
        className="flex h-10 w-full shrink-0 items-center justify-center"
        data-testid="rail-top-band"
        data-tauri-drag-region={platform.dragRegions ? true : undefined}
      >
        {!underTrafficLights && <ShellBrandMark dragRegion={platform.dragRegions} />}
      </div>
      {underTrafficLights && <ShellBrandMark dragRegion={platform.dragRegions} />}

      {/* Projects, the Side panel tabs and the actions scroll together on a
          short window; Settings stays pinned at the bottom. */}
      <div className="flex min-h-0 w-full flex-1 flex-col items-center gap-3 overflow-y-auto pt-1 scrollbar-hide">
        <RailProjectStack
          projects={projects}
          activeProject={activeProject}
          onSelectProject={onSelectProject}
          onNewProject={onNewProject}
        />

        {/* The Column tabs sit in a track, like the tab bar of the expanded
            Side panel, so they read as tabs and not as actions. */}
        <div
          data-testid="rail-column-tab-track"
          // 10px = the 8px buttons inside + the 2px padding (concentric).
          className="flex flex-col items-center gap-0.5 rounded-[10px] border border-border bg-card p-0.5"
        >
          <RailColumnIcons activeProject={activeProject} />
        </div>

        <div className="flex flex-col items-center gap-1">
          <RailButton
            label="Search"
            hint={searchShortcut}
            aria-label="Search"
            onClick={onOpenCommandPalette}
          >
            <Search size={16} />
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

          {/* Desktop only: renders nothing on web. */}
          <SSHPopover
            onSSHConnect={onSSHConnect}
            onSelectProfile={onSelectSSHProfile}
            activeProfileId={activeSSHProfileId}
          />
        </div>
      </div>

      {/* Settings, then the Rail toggle in the bottom-left corner: the same
          point as in the expanded Rail's footer. */}
      <div className="mt-auto flex flex-col items-center gap-1">
        <RailButton
          label="Settings"
          hint={settingsShortcut}
          aria-label="Settings"
          pressed={settingsView === 'app'}
          onClick={() => useSettingsModalStore.getState().openApp()}
        >
          <SlidersHorizontal size={16} />
        </RailButton>
        <RailToggleButton />
      </div>
    </div>
  )
}
