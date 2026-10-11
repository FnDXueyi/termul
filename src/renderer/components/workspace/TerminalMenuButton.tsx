import type { ShellInfo } from '@shared/types/ipc.types'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Terminal as TerminalIcon } from '@/components/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { browserTabHide, browserTabShow } from '@/lib/browser-api'
import { cn } from '@/lib/utils'
import type { WorkspaceTab } from '@/stores/workspace-store'
import { useAvailableShells } from './use-available-shells'

interface TerminalMenuButtonProps {
  tabs: WorkspaceTab[]
  activeTabId: string | null
  onAddTerminal?: (shell?: ShellInfo) => void
  defaultShell?: string
}

export function TerminalMenuButton({
  tabs,
  activeTabId,
  onAddTerminal,
  defaultShell
}: TerminalMenuButtonProps): React.JSX.Element | null {
  const [isTerminalMenuOpen, setIsTerminalMenuOpen] = useState(false)
  const terminalMenuRef = useRef<HTMLDivElement>(null)
  const { loading, sortedShells } = useAvailableShells(defaultShell)

  useEffect(() => {
    const handleClickOutside = (e: globalThis.MouseEvent): void => {
      if (terminalMenuRef.current && !terminalMenuRef.current.contains(e.target as Node)) {
        setIsTerminalMenuOpen(false)
      }
    }
    if (isTerminalMenuOpen) {
      document.addEventListener('mousedown', handleClickOutside)
    }
    return () => {
      document.removeEventListener('mousedown', handleClickOutside)
    }
  }, [isTerminalMenuOpen])

  // Native child webviews paint above the DOM, so the terminal popover would be
  // obscured unless we temporarily hide browser webviews while the menu is open.
  useEffect(() => {
    const browserTabs = tabs.filter(
      (tab): tab is WorkspaceTab & { type: 'browser'; browserTabId: string } =>
        tab.type === 'browser'
    )
    if (browserTabs.length === 0) return

    const hideAll = (tabsToHide: Array<{ browserTabId: string }>): void => {
      for (const tab of tabsToHide) {
        void browserTabHide(tab.browserTabId).catch(console.error)
      }
    }

    const showActive = (activeBrowserTab?: { browserTabId: string }): void => {
      if (activeBrowserTab) {
        void browserTabShow(activeBrowserTab.browserTabId).catch(console.error)
      }
    }

    if (isTerminalMenuOpen) {
      hideAll(browserTabs)
      return
    }

    const activeBrowserTab = browserTabs.find((tab) => tab.id === activeTabId)
    showActive(activeBrowserTab)
  }, [isTerminalMenuOpen, tabs, activeTabId])

  const handleSelectShell = useCallback(
    (shell: ShellInfo) => {
      if (onAddTerminal) {
        onAddTerminal(shell)
      }
      setIsTerminalMenuOpen(false)
    },
    [onAddTerminal]
  )

  if (!onAddTerminal) return null

  return (
    <div ref={terminalMenuRef} className="relative flex items-center h-full">
      <button
        onClick={() => setIsTerminalMenuOpen((open) => !open)}
        className="h-7 w-7 flex items-center justify-center rounded-md hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors duration-150 ease-out"
        title="Open terminal menu"
        aria-label="Open terminal menu"
        aria-expanded={isTerminalMenuOpen}
      >
        <TerminalIcon size={12} />
      </button>

      {isTerminalMenuOpen && (
        <div className="absolute top-full right-0 mt-1 w-44 bg-popover border border-border rounded-md shadow-lg z-50 overflow-hidden">
          <div className="px-2.5 py-1 text-2xs font-medium text-muted-foreground bg-secondary/30">
            Terminal
          </div>
          {loading ? (
            <div className="py-1 px-2.5 space-y-1.5">
              <Skeleton className="h-6 w-full" />
              <Skeleton className="h-6 w-full" />
            </div>
          ) : sortedShells && sortedShells.length > 0 ? (
            <div className="py-1">
              {sortedShells.map((shell) => (
                <button
                  key={shell.name}
                  onClick={() => handleSelectShell(shell)}
                  className={cn(
                    'w-full px-2.5 py-1.5 text-left text-2xs hover:bg-secondary flex items-center gap-2 leading-none',
                    shell.name === defaultShell && 'text-primary'
                  )}
                >
                  <TerminalIcon size={11} />
                  <span className="truncate">{shell.displayName}</span>
                  {shell.name === defaultShell && (
                    <span className="ml-auto text-3xs text-muted-foreground">(default)</span>
                  )}
                </button>
              ))}
            </div>
          ) : (
            <div className="px-2.5 py-1.5 text-2xs text-muted-foreground">No shells detected</div>
          )}
        </div>
      )}
    </div>
  )
}
