import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { writeDebounced } = vi.hoisted(() => ({
  writeDebounced: vi.fn()
}))

vi.mock('@/lib/api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api')>()
  return { ...actual, persistenceApi: { ...actual.persistenceApi, writeDebounced } }
})
vi.mock('@/lib/git-api', () => ({ gitApi: { getLog: vi.fn().mockResolvedValue([]) } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import { useGitHistoryStore } from '@/stores/git-history-store'
import { resetShellLayoutStoreForTests, useShellLayoutStore } from '@/stores/shell-layout-store'
import { GraphSection } from './GraphSection'

const CWD = '/work'

function layout() {
  return useShellLayoutStore.getState()
}

function renderSection() {
  return render(<GraphSection cwd={CWD} isVisible />)
}

function handle(): HTMLElement {
  return screen.getByRole('separator', { name: 'Resize graph' })
}

function pointer(type: string, clientY: number): void {
  act(() => {
    document.dispatchEvent(new MouseEvent(type, { clientY, bubbles: true }))
  })
}

describe('GraphSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    writeDebounced.mockResolvedValue({ success: true, data: undefined })
    resetShellLayoutStoreForTests()
    useShellLayoutStore.setState({ status: 'ready' })
    useGitHistoryStore.setState({ commits: { [CWD]: [] }, loading: {}, error: {} })
  })

  it('opens at the default 240px with a labelled horizontal separator', () => {
    renderSection()

    const separator = handle()
    expect(separator).toHaveAttribute('aria-orientation', 'horizontal')
    expect(separator).toHaveAttribute('aria-valuenow', '240')
    expect(separator).toHaveAttribute('aria-valuemin', '120')
    expect(separator).toHaveAttribute('aria-valuemax', '600')
    const section = screen.getByRole('region', { name: 'Git graph' })
    expect(section.style.height).toBe('240px')
    expect(section.className).toContain('border-t')
    expect(section.className).toContain('max-h-[60%]')
    expect(screen.getByText('No commit history')).toBeInTheDocument()
  })

  it('resizes by dragging the top edge: up is taller, and the store keeps the height', () => {
    renderSection()

    fireEvent.pointerDown(handle(), { button: 0, clientY: 400 })
    pointer('pointermove', 300)
    expect(layout().graphHeight).toBe(340)
    pointer('pointermove', 500)
    expect(layout().graphHeight).toBe(140)
    pointer('pointermove', 0)
    expect(layout().graphHeight).toBe(600)
    pointer('pointermove', 900)
    expect(layout().graphHeight).toBe(120)
    pointer('pointerup', 900)

    // Released: further moves no longer resize.
    pointer('pointermove', 100)
    expect(layout().graphHeight).toBe(120)
    expect(writeDebounced).toHaveBeenCalled()
  })

  it('resizes by keyboard in steps of 16 and clamps with Home and End', () => {
    renderSection()

    fireEvent.keyDown(handle(), { key: 'ArrowDown' })
    expect(layout().graphHeight).toBe(224)
    fireEvent.keyDown(handle(), { key: 'ArrowUp' })
    fireEvent.keyDown(handle(), { key: 'ArrowUp' })
    expect(layout().graphHeight).toBe(256)
    fireEvent.keyDown(handle(), { key: 'Home' })
    expect(layout().graphHeight).toBe(120)
    expect(handle()).toHaveAttribute('aria-valuenow', '120')
    fireEvent.keyDown(handle(), { key: 'ArrowDown' })
    expect(layout().graphHeight).toBe(120)
    fireEvent.keyDown(handle(), { key: 'End' })
    expect(layout().graphHeight).toBe(600)
    fireEvent.keyDown(handle(), { key: 'ArrowUp' })
    expect(layout().graphHeight).toBe(600)
  })

  it('shows a stored height below the minimum as 120', () => {
    act(() => layout().setGraphHeight(50))
    renderSection()

    expect(layout().graphHeight).toBe(120)
    expect(screen.getByRole('region', { name: 'Git graph' }).style.height).toBe('120px')
  })

  it('collapses from the Git History header to the header alone and reopens at the same height', () => {
    act(() => layout().setGraphHeight(320))
    renderSection()
    writeDebounced.mockClear()

    const title = screen.getByRole('button', { name: 'Git History' })
    expect(title).toHaveAttribute('aria-expanded', 'true')

    fireEvent.click(title)

    expect(layout().graphOpen).toBe(false)
    expect(screen.getByRole('button', { name: 'Git History' })).toHaveAttribute(
      'aria-expanded',
      'false'
    )
    expect(screen.queryByRole('separator')).toBeNull()
    expect(screen.queryByLabelText('Filter commits')).toBeNull()
    expect(screen.queryByText('No commit history')).toBeNull()
    expect(screen.getByRole('region', { name: 'Git graph' }).style.height).toBe('')
    // The open flag is in memory only.
    expect(writeDebounced).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Git History' }))

    expect(layout().graphOpen).toBe(true)
    expect(handle()).toHaveAttribute('aria-valuenow', '320')
    expect(screen.getByRole('region', { name: 'Git graph' }).style.height).toBe('320px')
    expect(screen.getByLabelText('Filter commits')).toBeInTheDocument()
  })

  it('never writes graphOpen into the persisted record', () => {
    renderSection()
    fireEvent.click(screen.getByRole('button', { name: 'Git History' }))
    act(() => layout().setGraphHeight(300))

    const snapshot = writeDebounced.mock.calls.at(-1)?.[1] as Record<string, unknown>
    expect(snapshot.graphHeight).toBe(300)
    expect(snapshot).not.toHaveProperty('graphOpen')
  })
})
