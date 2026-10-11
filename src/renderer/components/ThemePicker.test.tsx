import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  captureShellLayoutAtLoad,
  resetShellLayoutActiveAtLoadForTests
} from '@/lib/shell-layout-mode'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { useThemePickerStore } from '@/stores/theme-picker-store'
import { ThemePicker } from './ThemePicker'

function openPicker(): HTMLElement {
  act(() => useThemePickerStore.getState().open('termul'))
  return screen.getByRole('dialog', { name: 'Color theme picker' })
}

describe('ThemePicker anchor', () => {
  beforeEach(() => {
    // The picker scrolls its highlighted row into view; jsdom has no layout.
    Element.prototype.scrollIntoView = vi.fn()
    resetShellLayoutActiveAtLoadForTests()
    resetShellLayoutStoreForTests()
    useShellLayoutStore.setState({ status: 'unavailable' })
    useThemePickerStore.getState().close()
  })

  afterEach(() => {
    act(() => useThemePickerStore.getState().close())
    resetShellLayoutActiveAtLoadForTests()
  })

  it('keeps the classic left-14 anchor under the classic shell', () => {
    captureShellLayoutAtLoad('classic')
    render(<ThemePicker />)
    const dialog = openPicker()
    expect(dialog).toHaveClass('left-14')
    expect(dialog.style.left).toBe('')
  })

  it('keeps the classic anchor before the layout mode is known', () => {
    render(<ThemePicker />)
    expect(openPicker()).toHaveClass('left-14')
  })

  it('opens beside the expanded Rail: rail width + 8px gutter', () => {
    captureShellLayoutAtLoad('revamp')
    useShellLayoutStore.setState({ railExpanded: true, railWidth: 240 })
    render(<ThemePicker />)
    const dialog = openPicker()
    expect(dialog).not.toHaveClass('left-14')
    expect(dialog.style.left).toBe('248px')
  })

  it('opens beside the collapsed Rail: 48px + 8px gutter', () => {
    captureShellLayoutAtLoad('revamp')
    useShellLayoutStore.setState({ railExpanded: false, railWidth: 240 })
    render(<ThemePicker />)
    expect(openPicker().style.left).toBe('56px')
  })

  it('follows a Rail resize while open', () => {
    captureShellLayoutAtLoad('revamp')
    render(<ThemePicker />)
    const dialog = openPicker()
    expect(dialog.style.left).toBe('208px')

    act(() => useShellLayoutStore.getState().setRailWidth(300))
    expect(dialog.style.left).toBe('308px')
  })
})
