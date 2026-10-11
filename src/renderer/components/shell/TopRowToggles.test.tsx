import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as appSettingsHooks from '@/hooks/use-app-settings'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { RailToggleButton, SidePanelToggleButton } from './TopRowToggles'

const { tauriRef, mockToastError } = vi.hoisted(() => ({
  tauriRef: { current: true },
  mockToastError: vi.fn()
}))

vi.mock('sonner', () => ({ toast: { error: mockToastError } }))
vi.mock('@/lib/tauri-runtime', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-runtime')>('@/lib/tauri-runtime')
  return { ...actual, isTauriContext: () => tauriRef.current }
})

const updatePanelVisibility = vi.fn((panel: string, visible: boolean) => {
  if (panel === 'sidebarVisible') useShellLayoutStore.getState().setRailExpanded(visible)
  if (panel === 'fileExplorerVisible') useShellLayoutStore.setState({ sidePanelVisible: visible })
  return Promise.resolve()
})

beforeEach(() => {
  vi.clearAllMocks()
  tauriRef.current = true
  resetShellLayoutStoreForTests()
  useShellLayoutStore.setState({ status: 'unavailable' })
  vi.spyOn(appSettingsHooks, 'useUpdatePanelVisibility').mockReturnValue(updatePanelVisibility)
})

describe('RailToggleButton', () => {
  it('is named and follows the Rail state', async () => {
    render(<RailToggleButton />)
    const toggle = screen.getByRole('button', { name: 'Expand or collapse projects' })
    expect(toggle).toHaveAttribute('title', 'Expand or collapse projects')
    expect(toggle).toHaveAttribute('aria-pressed', 'true')

    fireEvent.click(toggle)
    expect(updatePanelVisibility).toHaveBeenCalledWith('sidebarVisible', false)
    await waitFor(() => expect(toggle).toHaveAttribute('aria-pressed', 'false'))
  })

  it('toasts when saving fails', async () => {
    updatePanelVisibility.mockRejectedValueOnce(new Error('disk full'))
    render(<RailToggleButton />)
    fireEvent.click(screen.getByRole('button', { name: 'Expand or collapse projects' }))
    await waitFor(() => expect(mockToastError).toHaveBeenCalledWith('disk full'))
  })
})

describe('SidePanelToggleButton', () => {
  it('is named and follows the Side panel state', async () => {
    render(<SidePanelToggleButton />)
    const toggle = screen.getByRole('button', { name: 'Show or hide side panel' })
    expect(toggle).toHaveAttribute('title', 'Show or hide side panel')
    expect(toggle).toHaveAttribute('aria-pressed', 'true')

    fireEvent.click(toggle)
    expect(updatePanelVisibility).toHaveBeenCalledWith('fileExplorerVisible', false)
    await waitFor(() => expect(toggle).toHaveAttribute('aria-pressed', 'false'))

    fireEvent.click(toggle)
    expect(updatePanelVisibility).toHaveBeenLastCalledWith('fileExplorerVisible', true)
  })

  it('uses the generic message when the error is not an Error', async () => {
    updatePanelVisibility.mockRejectedValueOnce('nope')
    render(<SidePanelToggleButton />)
    fireEvent.click(screen.getByRole('button', { name: 'Show or hide side panel' }))
    await waitFor(() =>
      expect(mockToastError).toHaveBeenCalledWith('Failed to update side panel visibility')
    )
  })
})

describe('look', () => {
  it('uses the full-height titlebar look with a 16px glyph on desktop', () => {
    render(<SidePanelToggleButton />)
    const toggle = screen.getByRole('button', { name: 'Show or hide side panel' })
    expect(toggle).toHaveClass('h-full', 'px-3')
    expect(toggle.querySelector('svg')).toHaveAttribute('width', '16')
    expect(toggle).not.toHaveAttribute('data-tauri-drag-region')
  })

  it('uses the 28px panel icon button with a 14px glyph on web', () => {
    tauriRef.current = false
    render(<RailToggleButton />)
    const toggle = screen.getByRole('button', { name: 'Expand or collapse projects' })
    expect(toggle).toHaveClass('size-7', 'rounded-md')
    expect(toggle.querySelector('svg')).toHaveAttribute('width', '14')
  })
})
