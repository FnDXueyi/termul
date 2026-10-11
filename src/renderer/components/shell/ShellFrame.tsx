import type { ComponentProps } from 'react'
import { ActivityRail } from '@/components/ActivityRail'
import { ResizeEdges } from '@/components/ResizeEdges'
import { ShellRail } from '@/components/shell/ShellRail'
import { ShellSidePanel } from '@/components/shell/ShellSidePanel'
import { TitleBar } from '@/components/TitleBar'
import { WorkspaceConflictBanner } from '@/components/workspace/WorkspaceConflictBanner'
import { useNativeWebviewOcclusion } from '@/hooks/use-native-webview-occlusion'
import { PaneDndProvider } from '@/hooks/use-pane-dnd'
import type { DesktopWorkspaceShell } from '@/layouts/workspace-layout/DesktopWorkspaceShell'
import { MacOsTitlebarStrip } from '@/layouts/workspace-layout/MacOsTitlebarStrip'
import { useThemePickerOpen } from '@/stores/theme-picker-store'

export type ShellFrameProps = ComponentProps<typeof DesktopWorkspaceShell>

/**
 * Opted-in desktop frame (`shellLayout=revamp`): Rail | Side panel | main area.
 * Composes the same components as the classic shell; only the order differs
 * (the explorer column moves to the left of the main area).
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
  handleAddGitTab,
  handleAddGitHistoryTab,
  handleToggleThemePicker,
  handleOpenAgentChat,
  handleOpenCanvas,
  workspaceMain,
  appModals
}: ShellFrameProps): React.JSX.Element {
  const isThemePickerOpen = useThemePickerOpen()
  const { activeSSHProfileId, handleSSHConnect, handleSelectSSHProfile } = ssh

  // Native browser webviews paint above the DOM: hide them while a shell
  // popover is open so it is never drawn underneath.
  useNativeWebviewOcclusion(isShortcutMenuOpen)
  useNativeWebviewOcclusion(isThemePickerOpen)

  return (
    <div className="h-screen flex flex-col overflow-hidden bg-background">
      <ResizeEdges />
      <div className="flex-1 flex flex-col overflow-hidden min-h-0 h-full">
        <MacOsTitlebarStrip />
        <div className="flex-1 flex overflow-hidden min-h-0">
          <ActivityRail
            isShortcutsOpen={isShortcutMenuOpen}
            onShortcutsOpenChange={setIsShortcutMenuOpen}
            onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
            onOpenGitChanges={() => handleAddGitTab()}
            canOpenGitChanges={Boolean(activeProject?.path)}
            onOpenGitHistory={() => handleAddGitHistoryTab()}
            canOpenGitHistory={Boolean(activeProject?.path)}
            isThemePickerOpen={isThemePickerOpen}
            onToggleThemePicker={handleToggleThemePicker}
            onOpenAgentChat={handleOpenAgentChat}
            canOpenAgentChat={Boolean(activeProject?.path)}
            onOpenCanvas={handleOpenCanvas}
            canOpenCanvas={Boolean(activeProject?.path)}
          />
          <div className="flex-1 flex flex-col min-w-0">
            <TitleBar />

            {/* Shell row: Rail | Side panel | main card. Same gaps and
                padding as the classic row; the columns set their own surface. */}
            <div className="flex-1 flex gap-2 overflow-hidden min-h-0 h-full py-2 pr-2">
              <ShellRail
                projects={projects}
                activeProjectId={activeProjectId}
                onSelectProject={onSelectProject}
                onNewProject={() => setIsNewProjectModalOpen(true)}
                onSSHConnect={handleSSHConnect}
                onSelectSSHProfile={handleSelectSSHProfile}
                activeSSHProfileId={activeSSHProfileId}
              />

              <PaneDndProvider>
                <div className="flex-1 flex gap-2 min-h-0 h-full overflow-hidden min-w-0">
                  <ShellSidePanel activeProject={activeProject} ssh={ssh} />

                  <main className="flex-1 flex flex-col min-w-0 rounded-xl border border-border bg-card overflow-clip">
                    <WorkspaceConflictBanner />
                    {workspaceMain}
                  </main>
                </div>
              </PaneDndProvider>
            </div>
          </div>
        </div>
      </div>

      {/* Modals */}
      {appModals}
    </div>
  )
}
