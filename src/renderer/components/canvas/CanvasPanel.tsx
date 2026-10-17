/**
 * The `canvas` tab's pane: a header (doc name, unsaved marker, mount status)
 * over the figcraft iframe. The mount itself is driven by the canvas store's
 * phase; this panel re-sends `host.theme` / `host.locale` when Termul's theme
 * or the browser language changes, without reloading the editor.
 */

import { useEffect } from 'react'
import { Edit2 } from '@/components/icons'
import { useMobileWebShell } from '@/hooks/use-mobile-web-shell'
import { basename } from '@/lib/path-basename'
import { COLOR_THEME_CHANGED_EVENT } from '@/lib/themes'
import { type CanvasPhase, useCanvasSession, useCanvasStore } from '@/stores/canvas-store'
import { CanvasShell } from './CanvasShell'

interface CanvasPanelProps {
  projectId: string
  docPath: string
  isVisible: boolean
}

function statusLabel(phase: CanvasPhase): string {
  switch (phase.kind) {
    case 'opening':
      return 'Starting canvas…'
    case 'loading':
      return 'Loading editor…'
    case 'ready':
      return 'Ready'
    case 'failed':
      return 'Canvas unavailable'
  }
}

function Notice({ text, testId }: { text: string; testId?: string }): React.JSX.Element {
  return (
    <div className="flex h-full w-full items-center justify-center bg-background p-4">
      <span className="text-center text-sm text-muted-foreground" data-testid={testId}>
        {text}
      </span>
    </div>
  )
}

export function CanvasPanel({
  projectId,
  docPath,
  isVisible
}: CanvasPanelProps): React.JSX.Element {
  const session = useCanvasSession(projectId)
  const isPhoneWidth = useMobileWebShell()

  useEffect(() => {
    const pushTheme = (): void => useCanvasStore.getState().pushTheme(projectId)
    const pushLocale = (): void => useCanvasStore.getState().pushLocale(projectId)
    window.addEventListener(COLOR_THEME_CHANGED_EVENT, pushTheme)
    window.addEventListener('languagechange', pushLocale)
    return () => {
      window.removeEventListener(COLOR_THEME_CHANGED_EVENT, pushTheme)
      window.removeEventListener('languagechange', pushLocale)
    }
  }, [projectId])

  if (isPhoneWidth) {
    return (
      <Notice
        text="The canvas is not available at this screen width."
        testId="canvas-unsupported-surface"
      />
    )
  }
  if (!session) return <Notice text="Canvas closed" />

  const name = basename(session.docPath || docPath)
  const { phase } = session
  const mount = phase.kind === 'loading' || phase.kind === 'ready' ? phase.mount : null

  return (
    <div
      className="flex h-full w-full flex-col bg-background"
      data-canvas-tab-state={isVisible ? 'visible' : 'hidden'}
      data-canvas-phase={phase.kind}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-card px-3">
        <Edit2 size={12} className="shrink-0 text-primary" aria-hidden="true" />
        <span className="min-w-0 truncate text-2xs font-medium text-foreground">{name}</span>
        {session.dirty && (
          <span
            role="img"
            className="h-2 w-2 shrink-0 rounded-full bg-primary-fill"
            aria-label="Unsaved changes"
            title="Unsaved changes"
          />
        )}
        <span className="shrink-0 text-3xs text-muted-foreground">{statusLabel(phase)}</span>
      </div>
      <div className="relative min-h-0 flex-1">
        {mount ? (
          <CanvasShell
            key={mount.mountSeq}
            projectId={projectId}
            mount={mount}
            title={`Canvas — ${name}`}
          />
        ) : phase.kind === 'failed' ? (
          <Notice text={phase.message} testId="canvas-mount-failed" />
        ) : (
          <Notice text="Starting canvas…" />
        )}
      </div>
    </div>
  )
}
