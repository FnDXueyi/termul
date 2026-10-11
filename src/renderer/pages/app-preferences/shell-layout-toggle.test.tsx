import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  captureShellLayoutAtLoad,
  resetShellLayoutActiveAtLoadForTests
} from '@/lib/shell-layout-mode'
import { useAppSettingsStore } from '@/stores/app-settings-store'
import { APP_SETTINGS_KEY, DEFAULT_APP_SETTINGS } from '@/types/settings'
import { ShellLayoutToggle } from './shell-layout-toggle'

const { mockWriteDebounced } = vi.hoisted(() => ({ mockWriteDebounced: vi.fn() }))

vi.mock('@/lib/api', () => ({
  acpApi: {},
  terminalApi: {},
  persistenceApi: { writeDebounced: mockWriteDebounced }
}))

function loadSettings(shellLayout?: 'classic' | 'revamp'): void {
  const settings = shellLayout ? { ...DEFAULT_APP_SETTINGS, shellLayout } : DEFAULT_APP_SETTINGS
  captureShellLayoutAtLoad(settings.shellLayout)
  useAppSettingsStore.getState().setSettings({ ...settings })
}

function getSwitch(): HTMLElement {
  return screen.getByRole('switch', { name: 'Revamped shell layout' })
}

describe('ShellLayoutToggle', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockWriteDebounced.mockResolvedValue({ success: true, data: undefined })
    resetShellLayoutActiveAtLoadForTests()
    useAppSettingsStore.setState({ settings: { ...DEFAULT_APP_SETTINGS }, isLoaded: false })
  })

  it('is off when the flag is unset and shows no reload hint', () => {
    loadSettings()
    render(<ShellLayoutToggle />)
    expect(getSwitch()).toHaveAttribute('aria-checked', 'false')
    expect(screen.queryByText('Reload to apply')).not.toBeInTheDocument()
  })

  it('persists revamp when flipped on and shows "Reload to apply"', () => {
    loadSettings()
    render(<ShellLayoutToggle />)

    fireEvent.click(getSwitch())

    expect(useAppSettingsStore.getState().settings.shellLayout).toBe('revamp')
    expect(mockWriteDebounced).toHaveBeenCalledWith(
      APP_SETTINGS_KEY,
      expect.objectContaining({ shellLayout: 'revamp' })
    )
    expect(getSwitch()).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByText('Reload to apply')).toBeInTheDocument()
  })

  it('drops the hint when flipped back to the mode that is active', () => {
    loadSettings()
    render(<ShellLayoutToggle />)
    fireEvent.click(getSwitch())
    fireEvent.click(getSwitch())

    expect(useAppSettingsStore.getState().settings.shellLayout).toBe('classic')
    expect(screen.queryByText('Reload to apply')).not.toBeInTheDocument()
  })

  it('reads as on without a hint after a reload that loaded revamp', () => {
    loadSettings('revamp')
    render(<ShellLayoutToggle />)
    expect(getSwitch()).toHaveAttribute('aria-checked', 'true')
    expect(screen.queryByText('Reload to apply')).not.toBeInTheDocument()

    act(() => useAppSettingsStore.getState().updateSetting('shellLayout', 'classic'))
    expect(screen.getByText('Reload to apply')).toBeInTheDocument()
  })

  it('does not touch the legacy panel settings', () => {
    loadSettings()
    render(<ShellLayoutToggle />)
    fireEvent.click(getSwitch())

    const { settings } = useAppSettingsStore.getState()
    expect(settings.sidebarVisible).toBe(DEFAULT_APP_SETTINGS.sidebarVisible)
    expect(settings.fileExplorerVisible).toBe(DEFAULT_APP_SETTINGS.fileExplorerVisible)
    expect(settings.sshPanelVisible).toBe(DEFAULT_APP_SETTINGS.sshPanelVisible)
  })
})
