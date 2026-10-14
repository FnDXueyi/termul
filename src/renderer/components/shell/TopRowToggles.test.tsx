import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import * as appSettingsHooks from '@/hooks/use-app-settings'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { RailToggleButton, withShortcut } from './TopRowToggles'

const { mockToastError } = vi.hoisted(() => ({ mockToastError: vi.fn() }))
vi.mock('sonner', () => ({ toast: { error: mockToastError } }))

const updatePanelVisibility = vi.fn((panel: string, visible: boolean) => {
  if (panel === 'sidebarVisible') useShellLayoutStore.getState().setRailExpanded(visible)
  return Promise.resolve()
})

function renderToggle() {
  render(
    <TooltipProvider delayDuration={0}>
      <RailToggleButton />
    </TooltipProvider>
  )
  return screen.getByRole('button', { name: 'Expand or collapse projects' })
}

beforeEach(() => {
  vi.clearAllMocks()
  resetShellLayoutStoreForTests()
  useShellLayoutStore.setState({ status: 'unavailable' })
  vi.spyOn(appSettingsHooks, 'useUpdatePanelVisibility').mockReturnValue(updatePanelVisibility)
})

describe('withShortcut', () => {
  it('appends a bound shortcut and leaves the label alone without one', () => {
    expect(withShortcut('Hide side panel', 'Ctrl+B')).toBe('Hide side panel (Ctrl+B)')
    expect(withShortcut('Hide side panel', undefined)).toBe('Hide side panel')
  })
})

describe('RailToggleButton', () => {
  it('keeps a stable name and follows the Rail state', async () => {
    const toggle = renderToggle()
    expect(toggle).toHaveAttribute('aria-pressed', 'true')

    fireEvent.click(toggle)
    expect(updatePanelVisibility).toHaveBeenCalledWith('sidebarVisible', false)
    await waitFor(() => expect(toggle).toHaveAttribute('aria-pressed', 'false'))
  })

  it('uses the 36px rail button look, with no keycap while expanded', () => {
    const toggle = renderToggle()
    expect(toggle).toHaveClass('size-9', 'rounded-lg')
    expect(toggle).not.toHaveClass('keycap')
    expect(toggle.querySelector('svg')).toHaveAttribute('width', '16')
  })

  it('swaps the glyph: collapse when expanded, expand when collapsed', async () => {
    const toggle = renderToggle()
    expect(toggle.querySelector('[data-termul-icon="PanelLeftCollapse"]')).not.toBeNull()

    fireEvent.click(toggle)
    await waitFor(() =>
      expect(toggle.querySelector('[data-termul-icon="PanelLeftExpand"]')).not.toBeNull()
    )
  })

  it('names the next action and its shortcut in the tooltip', async () => {
    const toggle = renderToggle()
    fireEvent.focus(toggle)
    expect((await screen.findAllByText(/^Collapse projects \(.*B\)$/)).length).toBeGreaterThan(0)
  })

  it('toasts when saving fails', async () => {
    updatePanelVisibility.mockRejectedValueOnce(new Error('disk full'))
    fireEvent.click(renderToggle())
    await waitFor(() => expect(mockToastError).toHaveBeenCalledWith('disk full'))
  })
})
