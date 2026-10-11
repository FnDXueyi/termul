import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SSHWorkspaceState } from '@/layouts/workspace-layout/use-ssh-workspace'
import type { Project } from '@/types/project'
import { ExplorerColumn } from './ExplorerColumn'

const visible = vi.hoisted(() => ({ value: true }))

vi.mock('@/stores/file-explorer-store', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useFileExplorerVisible: () => visible.value
}))
vi.mock('@/components/file-explorer/FileExplorer', () => ({
  FileExplorer: ({ side }: { side?: string }) => (
    <div data-testid="file-explorer" data-side={side} />
  )
}))
vi.mock('@/components/ssh/SSHFileExplorer', () => ({
  SSHFileExplorer: () => <div data-testid="ssh-explorer" />
}))

const project = { id: 'p1', name: 'One', path: '/repo', color: 'blue' } as Project

function makeSsh(active: boolean): SSHWorkspaceState {
  return {
    activeSSHProfile: active ? { id: 's1', name: 'Box' } : null,
    sshConn: {
      connectionId: null,
      isConnected: false,
      sftpReady: false,
      entries: [],
      currentPath: '/',
      expandedDirs: new Set(),
      childEntries: {},
      loadingDirs: new Set(),
      isLoadingRoot: false,
      handleConnect: vi.fn(),
      handleBrowseFiles: vi.fn(),
      toggleDirectory: vi.fn(),
      loadDirectory: vi.fn()
    },
    handleSSHMkdir: vi.fn(),
    handleSSHCreateFile: vi.fn(),
    handleSSHDelete: vi.fn(),
    handleSSHRename: vi.fn()
  } as unknown as SSHWorkspaceState
}

const slot = <div data-testid="side-panel-slot" />

describe('ExplorerColumn', () => {
  beforeEach(() => {
    visible.value = true
  })

  it('renders the classic File Explorer without a sidePanel slot', async () => {
    render(<ExplorerColumn activeProject={project} ssh={makeSsh(false)} side="right" />)
    expect(await screen.findByTestId('file-explorer')).toHaveAttribute('data-side', 'right')
    expect(screen.queryByTestId('side-panel-slot')).toBeNull()
  })

  it('keeps the classic File Explorer and the SSH block together without a slot', async () => {
    render(<ExplorerColumn activeProject={project} ssh={makeSsh(true)} />)
    expect(await screen.findByTestId('file-explorer')).toBeInTheDocument()
    expect(await screen.findByTestId('ssh-explorer')).toBeInTheDocument()
  })

  it('renders the sidePanel slot in place of the File Explorer', async () => {
    render(
      <ExplorerColumn activeProject={project} ssh={makeSsh(false)} side="left" sidePanel={slot} />
    )
    expect(await screen.findByTestId('side-panel-slot')).toBeInTheDocument()
    expect(screen.queryByTestId('file-explorer')).toBeNull()
  })

  it('shows only the SSH block, at its own width, while an SSH profile is active', async () => {
    render(
      <ExplorerColumn activeProject={project} ssh={makeSsh(true)} side="left" sidePanel={slot} />
    )
    const ssh = await screen.findByTestId('ssh-explorer')
    expect(screen.queryByTestId('side-panel-slot')).toBeNull()
    expect(screen.queryByTestId('file-explorer')).toBeNull()
    expect(ssh.parentElement).toHaveClass('w-64')
  })

  it('renders nothing for the slot while the explorer is hidden', () => {
    visible.value = false
    render(
      <ExplorerColumn activeProject={project} ssh={makeSsh(false)} side="left" sidePanel={slot} />
    )
    expect(screen.queryByTestId('side-panel-slot')).toBeNull()
  })
})
