import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { useAppSettingsStore } from '@/stores/app-settings-store'
import { DEFAULT_APP_SETTINGS } from '@/types/settings'
import {
  captureShellLayoutAtLoad,
  getShellLayoutActiveAtLoad,
  resetShellLayoutActiveAtLoadForTests,
  resolveShellLayout,
  SHELL_LAYOUT_DEFAULT,
  useShellLayoutForRender,
  useShellLayoutReloadPending
} from './shell-layout-mode'

describe('shell-layout-mode', () => {
  beforeEach(() => {
    resetShellLayoutActiveAtLoadForTests()
    useAppSettingsStore.setState({ settings: { ...DEFAULT_APP_SETTINGS }, isLoaded: false })
  })

  it('defaults to revamp and is not part of DEFAULT_APP_SETTINGS', () => {
    expect(SHELL_LAYOUT_DEFAULT).toBe('revamp')
    expect('shellLayout' in DEFAULT_APP_SETTINGS).toBe(false)
  })

  it('resolves unset and unknown values to the default', () => {
    expect(resolveShellLayout(undefined)).toBe('revamp')
    expect(resolveShellLayout('nonsense')).toBe('revamp')
    expect(resolveShellLayout('classic')).toBe('classic')
    expect(resolveShellLayout('revamp')).toBe('revamp')
  })

  it('captures the mode active at load once (first wins)', () => {
    expect(getShellLayoutActiveAtLoad()).toBeNull()
    expect(captureShellLayoutAtLoad(undefined)).toBe('revamp')
    expect(captureShellLayoutAtLoad('classic')).toBe('revamp')
    expect(getShellLayoutActiveAtLoad()).toBe('revamp')
  })

  describe('useShellLayoutReloadPending', () => {
    it('is false before settings load', () => {
      const { result } = renderHook(() => useShellLayoutReloadPending())
      expect(result.current).toBe(false)
    })

    it('turns on when the saved flag differs from the captured one and off when it matches', () => {
      captureShellLayoutAtLoad(undefined)
      useAppSettingsStore.getState().setSettings({ ...DEFAULT_APP_SETTINGS })
      const { result } = renderHook(() => useShellLayoutReloadPending())
      expect(result.current).toBe(false)

      act(() => useAppSettingsStore.getState().updateSetting('shellLayout', 'classic'))
      expect(result.current).toBe(true)

      act(() => useAppSettingsStore.getState().updateSetting('shellLayout', 'revamp'))
      expect(result.current).toBe(false)
    })

    it('reads as settled after a reload that loaded classic', () => {
      captureShellLayoutAtLoad('classic')
      useAppSettingsStore
        .getState()
        .setSettings({ ...DEFAULT_APP_SETTINGS, shellLayout: 'classic' })
      const { result } = renderHook(() => useShellLayoutReloadPending())
      expect(result.current).toBe(false)
    })
  })

  describe('useShellLayoutForRender', () => {
    it('is null while settings are pending and does not capture', () => {
      const { result } = renderHook(() => useShellLayoutForRender())
      expect(result.current).toBeNull()
      expect(getShellLayoutActiveAtLoad()).toBeNull()
    })

    it('resolves to the mode loaded settings carry', () => {
      useAppSettingsStore
        .getState()
        .setSettings({ ...DEFAULT_APP_SETTINGS, shellLayout: 'classic' })
      const { result } = renderHook(() => useShellLayoutForRender())
      expect(result.current).toBe('classic')
    })

    it('resolves to revamp when the flag is absent', () => {
      useAppSettingsStore.getState().setSettings({ ...DEFAULT_APP_SETTINGS })
      const { result } = renderHook(() => useShellLayoutForRender())
      expect(result.current).toBe('revamp')
    })

    it('ignores setting changes after load', () => {
      useAppSettingsStore.getState().setSettings({ ...DEFAULT_APP_SETTINGS })
      const { result } = renderHook(() => useShellLayoutForRender())
      expect(result.current).toBe('revamp')

      act(() => useAppSettingsStore.getState().updateSetting('shellLayout', 'classic'))
      expect(result.current).toBe('revamp')
    })
  })
})
