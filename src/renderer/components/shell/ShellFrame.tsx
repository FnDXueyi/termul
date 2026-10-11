import { type ComponentProps, useMemo } from 'react'
import { useLocation } from 'react-router-dom'
import { ResizeEdges } from '@/components/ResizeEdges'
import { ShellRail } from '@/components/shell/ShellRail'
import { ShellSidePanel } from '@/components/shell/ShellSidePanel'
import { TopRowFallbackBar } from '@/components/shell/TopRowFallbackBar'
import { TopRowProvider } from '@/components/shell/top-row-context'
import { WorkspaceConflictBanner } from '@/components/workspace/WorkspaceConflictBanner'
import { useNativeWebviewOcclusion } from '@/hooks/use-native-webview-occlusion'
import { PaneDndProvider } from '@/hooks/use-pane-dnd'
import type { DesktopWorkspaceShell } from '@/layouts/workspace-layout/DesktopWorkspaceShell'
import { getTopRowPlatform, selectTopRowRoles, type TopRowMode } from '@/lib/top-row'
import { isWorkspaceRoutePath } from '@/lib/workspace-route'
import { useThemePickerOpen } from '@/stores/theme-picker-store'
import { useFullscreenPaneId, usePaneRoot } from '@/stores/workspace-store'

export type ShellFrameProps = ComponentProps<typeof DesktopWorkspaceShell>

/** 8px drag gutter above the shell row (the row has no title strip). */
function TopGutter({ dragRegion }: { dragRegion: boolean }): React.JSX.Element {
  return (
    <div
      className="h-2 shrink-0"
      data-testid="top-gutter"
      data-tauri-drag-region={dragRegion ? true : undefined}
    />
  )
}

/**
 * Opted-in desktop frame (`shellLayout=revamp`): Rail | Side panel | main area.
 * The Rail replaces both the classic activity rail and the project sidebar;
 * the explorer column moves to the left of the main area. The window has no
 * title strip: the Rail header, the Side panel header and the top-edge pane tab
 * bars share one top row (roles come from `selectTopRowRoles`).
 */
export function ShellFrame({
  projects,
  activeProject,
  activeProjectId,
  ssh,
  isShortcutMenuOpen,
  setIsShortcutMenuOpen,
  setIsCommandPaletteOpen,
  setIsNewProjectModalOpen,
  onSelectProject,
  handleToggleThemePicker,
  handleOpenCanvas,
  workspaceMain,
  appModals
}: ShellFrameProps): React.JSX.Element {
  const isThemePickerOpen = useThemePickerOpen()
  const { activeSSHProfileId, handleSSHConnect, handleSelectSSHProfile } = ssh

  const paneRoot = usePaneRoot()
  const fullscreenPaneId = useFullscreenPaneId()
  const pathname = useLocation().pathname
  const mode: TopRowMode = ssh.activeSSHProfile
    ? 'ssh'
    : projects.length === 0 || !isWorkspaceRoutePath(pathname)
      ? 'fallback'
      : 'panes'
  const roles = useMemo(
    () => selectTopRowRoles({ root: paneRoot, fullscreenPaneId, mode }),
    [paneRoot, fullscreenPaneId, mode]
  )
  const platform = useMemo(() => getTopRowPlatform(), [])

  // Native browser webviews paint above the DOM: hide them while a shell
  // popover is open so it is never drawn underneath.
  useNativeWebviewOcclusion(isShortcutMenuOpen)
  useNativeWebviewOcclusion(isThemePickerOpen)

  return (
    <TopRowProvider roles={roles} platform={platform}>
      <div className="h-screen flex flex-col overflow-hidden bg-background">
        <ResizeEdges />
        <div className="flex-1 flex flex-col overflow-hidden min-h-0 h-full">
          <TopGutter dragRegion={platform.dragRegions} />

          {/* Shell row: Rail | Side panel | main card. The columns set their
              own surface; the 8px gutter above replaces the top padding. */}
          <div className="flex-1 flex gap-2 overflow-hidden min-h-0 h-full pb-2 pr-2">
            <ShellRail
              projects={projects}
              activeProject={activeProject}
              activeProjectId={activeProjectId}
              onSelectProject={onSelectProject}
              onNewProject={() => setIsNewProjectModalOpen(true)}
              onSSHConnect={handleSSHConnect}
              onSelectSSHProfile={handleSelectSSHProfile}
              activeSSHProfileId={activeSSHProfileId}
              onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
              onOpenCanvas={handleOpenCanvas}
              canOpenCanvas={Boolean(activeProject?.path)}
              isShortcutsOpen={isShortcutMenuOpen}
              onShortcutsOpenChange={setIsShortcutMenuOpen}
              isThemePickerOpen={isThemePickerOpen}
              onToggleThemePicker={handleToggleThemePicker}
            />

            <PaneDndProvider>
              {/* No gap here: the Side panel owns its 8px gutter so its resize handle
                  sits on the main container border. */}
              <div className="flex-1 flex min-h-0 h-full overflow-hidden min-w-0">
                <ShellSidePanel activeProject={activeProject} ssh={ssh} />

                <main className="flex-1 flex flex-col min-w-0 rounded-xl border border-border bg-card overflow-clip">
                  <WorkspaceConflictBanner />
                  {roles.controlsOwner === 'fallback-bar' && <TopRowFallbackBar />}
                  {workspaceMain}
                </main>
              </div>
            </PaneDndProvider>
          </div>
        </div>

        {/* Modals */}
        {appModals}
      </div>
    </TopRowProvider>
  )
}
