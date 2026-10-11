import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { FileExplorerToggleButton } from './TitlebarPanelToggles'

const { mockUpdatePanelVisibility, mockToastError } = vi.hoisted(() => ({
  mockUpdatePanelVisibility: vi.fn(() => Promise.resolve()),
  mockToastError: vi.fn()
}))

vi.mock('sonner', () => ({
  toast: {
    error: mockToastError
  }
}))

vi.mock('@/hooks/use-app-settings', () => ({
  useUpdatePanelVisibility: () => mockUpdatePanelVisibility
}))

describe('TitlebarPanelToggles', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetShellLayoutStoreForTests()
    useShellLayoutStore.setState({ status: 'ready', sidePanelVisible: true })
  })

  it('toggles the side panel via persistence-aware updater on click', async () => {
    render(<FileExplorerToggleButton />)

    fireEvent.click(screen.getByRole('button', { name: 'Hide side panel' }))

    await waitFor(() => {
      expect(mockUpdatePanelVisibility).toHaveBeenCalledWith('fileExplorerVisible', false)
    })
  })

  it('shows error toast when side panel persistence update fails', async () => {
    mockUpdatePanelVisibility.mockRejectedValueOnce(new Error('persist failed'))

    render(<FileExplorerToggleButton />)

    fireEvent.click(screen.getByRole('button', { name: 'Hide side panel' }))

    await waitFor(() => {
      expect(mockToastError).toHaveBeenCalledWith('persist failed')
    })
  })

  it('reflects hidden state in label and pressed state when not visible', () => {
    useShellLayoutStore.setState({ sidePanelVisible: false })

    render(<FileExplorerToggleButton />)

    const explorerButton = screen.getByRole('button', { name: 'Show side panel' })

    expect(explorerButton).toHaveAttribute('aria-pressed', 'false')
  })
})
