import { act, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAcpStore } from '@/stores/acp-store'
import { useGitStatusStore } from '@/stores/git-status-store'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import type { Project } from '@/types/project'
import { SidePanelColumn } from './SidePanelColumn'

vi.mock('@/components/file-explorer/FileExplorer', () => ({
  FileExplorer: ({ embedded }: { embedded?: boolean }) => (
    <div data-testid="file-explorer" data-embedded={String(Boolean(embedded))} />
  )
}))

vi.mock('./ChangesPanel', () => ({
  ChangesPanel: ({ cwd }: { cwd: string }) => <div data-testid="changes-panel" data-cwd={cwd} />
}))

const mocks = vi.hoisted(() => ({ showAgentLauncher: vi.fn() }))

const makeProject = (overrides: Partial<Project> = {}): Project =>
  ({
    id: 'p1',
    name: 'One',
    path: '/repo',
    color: 'blue',
    gitBranch: 'main',
    isGitRepo: true,
    worktrees: [
      {
        id: 'w1',
        name: 'feature',
        branch: 'feat/x',
        path: '/repo/.termul/worktrees/feature',
        createdAt: '2026-01-01T00:00:00.000Z'
      }
    ],
    ...overrides
  }) as Project

function LocationProbe(): React.JSX.Element {
  return <div data-testid="location">{useLocation().pathname}</div>
}

function renderPanel(project: Project, initialPath = '/') {
  const view = (p: Project) => (
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route path="*" element={<SidePanelColumn activeProject={p} />} />
      </Routes>
      <LocationProbe />
    </MemoryRouter>
  )
  const result = render(view(project))
  return { ...result, rerenderProject: (p: Project) => result.rerender(view(p)) }
}

beforeEach(() => {
  mocks.showAgentLauncher.mockReset()
  resetShellLayoutStoreForTests()
  useShellLayoutStore.setState({ status: 'ready' })
  useAcpStore.setState({ sessionIndex: [], sessions: {}, agentConfigs: [] })
  useGitStatusStore.setState({ statuses: {} })
  useWorkspaceStore.setState({
    activePaneId: 'pane-1',
    showAgentLauncher: mocks.showAgentLauncher
  })
})

describe('SidePanelColumn', () => {
  it('renders the header, the three tabs and the Chats list by default', () => {
    renderPanel(makeProject())
    expect(screen.getByText('Workspace')).toBeInTheDocument()
    expect(screen.getAllByRole('tab').map((t) => t.textContent)).toEqual([
      'Chats',
      'Explorer',
      'Changes'
    ])
    expect(screen.getByRole('tab', { name: 'Chats' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByLabelText('Search chats')).toBeInTheDocument()
  })

  it('shows the picked worktree branch in the header', () => {
    useShellLayoutStore.setState({
      worktreePickByProject: { p1: '/repo/.termul/worktrees/feature' }
    })
    renderPanel(makeProject())
    expect(screen.getByText('feat/x')).toBeInTheDocument()
    expect(screen.queryByText('Workspace')).toBeNull()
  })

  it('opens the launcher from New chat', () => {
    renderPanel(makeProject())
    fireEvent.click(screen.getByRole('button', { name: 'New chat' }))
    expect(mocks.showAgentLauncher).toHaveBeenCalledWith('pane-1')
  })

  it('navigates to the workspace route before opening the launcher', async () => {
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      cb(0)
      return 0
    })
    renderPanel(makeProject(), '/settings')
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'New chat' }))
    })
    expect(screen.getByTestId('location')).toHaveTextContent('/')
    expect(mocks.showAgentLauncher).toHaveBeenCalledWith('pane-1')
    vi.restoreAllMocks()
  })

  it('remembers the last tab per project', async () => {
    const p1 = makeProject()
    const p2 = makeProject({ id: 'p2', name: 'Two', path: '/two', worktrees: [] })
    const { rerenderProject } = renderPanel(p1)

    fireEvent.click(screen.getByRole('tab', { name: 'Explorer' }))
    expect(await screen.findByTestId('file-explorer')).toHaveAttribute('data-embedded', 'true')
    expect(useShellLayoutStore.getState().lastColumnTabByProject).toEqual({ p1: 'explorer' })

    rerenderProject(p2)
    expect(screen.getByRole('tab', { name: 'Chats' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByTestId('file-explorer')).toBeNull()

    rerenderProject(p1)
    expect(screen.getByRole('tab', { name: 'Explorer' })).toHaveAttribute('aria-selected', 'true')
  })

  it('renders the Changes panel for the picked working copy', () => {
    useShellLayoutStore.setState({
      worktreePickByProject: { p1: '/repo/.termul/worktrees/feature' }
    })
    useGitStatusStore.setState({
      statuses: {
        '/repo/.termul/worktrees/feature': [
          { path: 'a.ts' },
          { path: 'b.ts' },
          { path: 'a.ts' }
        ] as never
      }
    })
    renderPanel(makeProject())
    expect(screen.getByTestId('column-tab-changes-badge')).toHaveTextContent('2')

    fireEvent.click(screen.getByRole('tab', { name: /Changes/ }))
    expect(screen.getByTestId('changes-panel')).toHaveAttribute(
      'data-cwd',
      '/repo/.termul/worktrees/feature'
    )
  })

  it('uses the project path for Changes when nothing is picked', () => {
    renderPanel(makeProject())
    fireEvent.click(screen.getByRole('tab', { name: /Changes/ }))
    expect(screen.getByTestId('changes-panel')).toHaveAttribute('data-cwd', '/repo')
  })

  it('sizes the column from the store and exposes a resize separator', () => {
    useShellLayoutStore.setState({ sidePanelWidth: 300 })
    const { container } = renderPanel(makeProject())
    expect((container.firstElementChild as HTMLElement).style.width).toBe('300px')
    const separator = screen.getByRole('separator', { name: 'Resize side panel' })
    expect(separator).toHaveAttribute('aria-valuenow', '300')
    fireEvent.keyDown(separator, { key: 'ArrowRight' })
    expect(useShellLayoutStore.getState().sidePanelWidth).toBe(316)
  })
})
