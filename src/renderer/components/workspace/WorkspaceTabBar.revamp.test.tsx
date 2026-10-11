import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  captureShellLayoutAtLoad,
  resetShellLayoutActiveAtLoadForTests
} from '@/lib/shell-layout-mode'
import { WorkspaceTabBar } from './WorkspaceTabBar'

const { tauriRef, getShells } = vi.hoisted(() => ({
  tauriRef: { current: true as boolean },
  getShells: vi.fn()
}))

vi.mock('@/lib/tauri-runtime', () => ({ isTauriContext: () => tauriRef.current }))
vi.mock('@/lib/api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/api')>('@/lib/api')
  return { ...actual, shellApi: { getAvailableShells: getShells } }
})

function renderBar(props: Partial<React.ComponentProps<typeof WorkspaceTabBar>> = {}) {
  return render(
    <WorkspaceTabBar
      paneId="pane-a"
      tabs={[]}
      activeTabId={null}
      onAddTerminal={vi.fn()}
      onAddBrowserTab={vi.fn()}
      {...props}
    />
  )
}

describe('WorkspaceTabBar shell flag', () => {
  beforeEach(() => {
    tauriRef.current = true
    getShells.mockResolvedValue({
      success: true,
      data: { available: [{ name: 'bash', displayName: 'Bash', path: '/bin/bash' }] }
    })
  })

  afterEach(() => {
    cleanup()
    resetShellLayoutActiveAtLoadForTests()
  })

  it('classic keeps the terminal menu and browser buttons and no New tab trigger', () => {
    captureShellLayoutAtLoad('classic')
    renderBar()
    expect(screen.getByRole('button', { name: 'Open terminal menu' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'New browser tab' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'New tab' })).toBeNull()
  })

  it('revamp renders exactly one New tab trigger and no separate terminal or browser button', () => {
    captureShellLayoutAtLoad('revamp')
    renderBar()
    expect(screen.getAllByRole('button', { name: 'New tab' })).toHaveLength(1)
    expect(screen.queryByRole('button', { name: 'Open terminal menu' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'New browser tab' })).toBeNull()
  })

  it('revamp menu adds a terminal through onAddTerminal', async () => {
    captureShellLayoutAtLoad('revamp')
    const onAddTerminal = vi.fn()
    renderBar({ onAddTerminal })
    fireEvent.pointerDown(screen.getByRole('button', { name: 'New tab' }), {
      button: 0,
      ctrlKey: false
    })
    fireEvent.click(await screen.findByRole('menuitem', { name: /Bash/ }))
    expect(onAddTerminal).toHaveBeenCalledWith(expect.objectContaining({ name: 'bash' }))
    await waitFor(() => expect(screen.queryByRole('menu')).toBeNull())
  })
})
