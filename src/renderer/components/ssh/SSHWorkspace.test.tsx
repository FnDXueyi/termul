import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { useSSHConnection } from '@/hooks/use-ssh-connection'
import { SSHWorkspace } from './SSHWorkspace'

vi.mock('@/components/terminal/ConnectedTerminal', () => ({ ConnectedTerminal: () => null }))
vi.mock('./SSHFileEditor', () => ({ SSHFileEditor: () => null }))
vi.mock('@/stores/ssh-store', () => ({ useSSHEditorFile: () => null }))
vi.mock('@/lib/api', () => ({
  windowApi: {
    onMaximizeChange: () => () => {},
    minimize: vi.fn(),
    toggleMaximize: vi.fn().mockResolvedValue({ success: true, data: false }),
    close: vi.fn()
  }
}))

const profile = { id: 's1', name: 'prod' } as never
const conn = {
  isConnected: false,
  isConnectingStatus: false,
  isConnecting: false,
  localTerminalPtyId: null,
  connectionId: null,
  handleConnect: vi.fn(),
  handleDisconnect: vi.fn()
} as unknown as ReturnType<typeof useSSHConnection>

const header = (): HTMLElement =>
  screen.getByText('SSH: prod').closest('div.flex.items-center.border-b') as HTMLElement

describe('SSHWorkspace header', () => {
  it('is the classic 36px header without topRow', () => {
    const { container } = render(<SSHWorkspace profile={profile} conn={conn} />)
    expect(header()).toHaveClass('h-9')
    expect(header()).not.toHaveClass('h-10')
    expect(container.querySelector('[data-tauri-drag-region]')).toBeNull()
    expect(screen.queryByTestId('window-controls')).toBeNull()
  })

  it('is a 40px drag header with the controls flush right when it owns the top row', () => {
    const { container } = render(
      <SSHWorkspace
        profile={profile}
        conn={conn}
        topRow={{ dragRegion: true, windowControls: true }}
      />
    )
    const root = header()
    expect(root).toHaveClass('h-10')
    expect(root).not.toHaveClass('h-9')
    expect(root).toHaveAttribute('data-tauri-drag-region')
    // Root and the flex-1 spacer only; no button carries it.
    expect(container.querySelectorAll('[data-tauri-drag-region]')).toHaveLength(2)
    for (const button of screen.getAllByRole('button')) {
      expect(button).not.toHaveAttribute('data-tauri-drag-region')
    }
    const controls = screen.getByTestId('window-controls')
    expect(root.lastElementChild).toBe(controls)
    expect(screen.getAllByRole('button', { name: /window$/ })).toHaveLength(3)
  })

  it('renders no drag attribute and no controls on web or macOS', () => {
    const { container } = render(
      <SSHWorkspace
        profile={profile}
        conn={conn}
        topRow={{ dragRegion: false, windowControls: false }}
      />
    )
    expect(header()).toHaveClass('h-10')
    expect(container.querySelector('[data-tauri-drag-region]')).toBeNull()
    expect(screen.queryByTestId('window-controls')).toBeNull()
  })
})
