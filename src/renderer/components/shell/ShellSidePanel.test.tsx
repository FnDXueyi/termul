import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/layouts/workspace-layout/ExplorerColumn', () => ({
  ExplorerColumn: () => <div data-testid="explorer-column" />
}))
vi.mock('@/components/shell/ChangesPanel', () => ({
  ChangesPanel: ({ cwd, isVisible }: { cwd: string; isVisible: boolean }) => (
    <div data-testid="changes-panel" data-cwd={cwd} data-visible={String(isVisible)} />
  )
}))

import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import type { Project, Worktree } from '@/types/project'
import { ShellSidePanel } from './ShellSidePanel'

function project(overrides: Partial<Project> = {}): Project {
  return {
    id: 'p1',
    name: 'Proj',
    color: 'blue',
    path: 'C:/work/proj',
    worktrees: [],
    ...overrides
  } as Project
}

function ssh(activeSSHProfile: unknown = null): SSHWorkspaceState {
  return { activeSSHProfile } as unknown as SSHWorkspaceState
}

describe('ShellSidePanel Changes column', () => {
  beforeEach(() => {
    resetShellLayoutStoreForTests()
    useShellLayoutStore.setState({ status: 'ready', sidePanelVisible: true })
  })

  it('renders the Changes panel after the explorer at the stored side panel width', () => {
    useShellLayoutStore.setState({ sidePanelWidth: 300 })
    render(<ShellSidePanel activeProject={project()} ssh={ssh()} />)

    const explorer = screen.getByTestId('explorer-column')
    const panel = screen.getByTestId('changes-panel')
    expect(explorer.compareDocumentPosition(panel) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(panel).toHaveAttribute('data-cwd', 'C:/work/proj')
    expect(panel).toHaveAttribute('data-visible', 'true')
    const column = panel.parentElement as HTMLElement
    expect(column.style.width).toBe('300px')
    expect(column.className).toContain('bg-background')
    expect(column.className).toContain('rounded-xl')
  })

  it('shows the picked worktree instead of the project folder', () => {
    const worktree: Worktree = {
      id: 'w1',
      name: 'feat',
      branch: 'feat',
      path: 'C:/work/proj/.termul/worktrees/feat',
      createdAt: '2026-01-01T00:00:00.000Z'
    }
    useShellLayoutStore.getState().setWorktreePick('p1', worktree.path)
    render(<ShellSidePanel activeProject={project({ worktrees: [worktree] })} ssh={ssh()} />)

    expect(screen.getByTestId('changes-panel')).toHaveAttribute('data-cwd', worktree.path)
  })

  it('is hidden without a project folder', () => {
    render(<ShellSidePanel activeProject={project({ path: '' })} ssh={ssh()} />)
    expect(screen.queryByTestId('changes-panel')).toBeNull()
    expect(screen.getByTestId('explorer-column')).toBeInTheDocument()
  })

  it('is hidden without a project', () => {
    render(<ShellSidePanel activeProject={undefined} ssh={ssh()} />)
    expect(screen.queryByTestId('changes-panel')).toBeNull()
  })

  it('is hidden while an SSH profile is active', () => {
    render(<ShellSidePanel activeProject={project()} ssh={ssh({ id: 's1', name: 'box' })} />)
    expect(screen.queryByTestId('changes-panel')).toBeNull()
  })

  it('is hidden while the explorer is hidden', () => {
    useShellLayoutStore.setState({ sidePanelVisible: false })
    render(<ShellSidePanel activeProject={project()} ssh={ssh()} />)
    expect(screen.queryByTestId('changes-panel')).toBeNull()
  })
})
