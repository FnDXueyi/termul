import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Worktree } from '@/types/project'
import { LauncherContextStrip } from './context-strip'
import type { LaunchWorkspace } from './use-launch-workspace'

const occlusion = vi.hoisted(() => vi.fn())
vi.mock('@/hooks/use-native-webview-occlusion', () => ({
  useNativeWebviewOcclusion: (open: boolean) => occlusion(open)
}))

// Radix Select portals do not render under jsdom: each Select becomes a button
// that reports `onOpenChange`, enough to observe the occlusion tracking.
vi.mock('@/components/ui/select', () => ({
  Select: ({
    children,
    onOpenChange
  }: {
    children: React.ReactNode
    onOpenChange?: (open: boolean) => void
  }) => (
    <div>
      <button type="button" onClick={() => onOpenChange?.(true)}>
        open
      </button>
      <button type="button" onClick={() => onOpenChange?.(false)}>
        close
      </button>
      {children}
    </div>
  ),
  SelectTrigger: () => null,
  SelectValue: () => null,
  SelectContent: () => null,
  SelectItem: () => null
}))

const worktree: Worktree = {
  id: 'a',
  name: 'a',
  branch: 'feat/a',
  path: '/p/.termul/worktrees/a',
  createdAt: '2026-01-01T00:00:00.000Z'
}

function workspace(over: Partial<LaunchWorkspace> = {}): LaunchWorkspace {
  return {
    isolationMode: 'current',
    setIsolationMode: vi.fn(),
    existingWorktreeId: null,
    setExistingWorktreeId: vi.fn(),
    pickIsProjectFolder: true,
    resetWorkspace: vi.fn(),
    ...over
  }
}

function strip(over: Partial<LaunchWorkspace> = {}): React.JSX.Element {
  return (
    <LauncherContextStrip
      workspace={workspace(over)}
      worktrees={[worktree]}
      baseBranch={null}
      setBaseBranch={vi.fn()}
      baseBranchInfo={null}
      baseOptions={[]}
      interactive
      gitBranch="main"
      isGitRepo
    />
  )
}

describe('LauncherContextStrip menus', () => {
  beforeEach(() => occlusion.mockClear())
  afterEach(cleanup)

  it('is not an occluder until one of its selects opens, and releases on close', () => {
    render(strip())
    expect(occlusion).toHaveBeenLastCalledWith(false)
    fireEvent.click(screen.getAllByText('open')[0])
    expect(occlusion).toHaveBeenLastCalledWith(true)
    fireEvent.click(screen.getAllByText('close')[0])
    expect(occlusion).toHaveBeenLastCalledWith(false)
  })

  it('tracks the worktree select in Existing mode', () => {
    render(strip({ isolationMode: 'existing', existingWorktreeId: 'a' }))
    // Selects: the Workspace chip, then the Worktree picker.
    fireEvent.click(screen.getAllByText('open')[1])
    expect(occlusion).toHaveBeenLastCalledWith(true)
  })

  it('shows the read-only branch beside the Current checkout chip', () => {
    render(strip())
    expect(screen.getByText('main')).toBeInTheDocument()
    cleanup()
    render(
      <LauncherContextStrip
        workspace={workspace()}
        worktrees={[]}
        baseBranch={null}
        setBaseBranch={vi.fn()}
        baseBranchInfo={null}
        baseOptions={[]}
        interactive
        gitBranch={null}
        isGitRepo
      />
    )
    expect(screen.getByText('Detached HEAD')).toBeInTheDocument()
  })
})
