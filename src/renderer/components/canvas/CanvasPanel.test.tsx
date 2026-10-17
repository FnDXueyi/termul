/**
 * CanvasPanel against the real canvas store: what each mount phase renders,
 * the iframe URL, the phone-width gate, theme/locale re-sends, and the
 * unsaved dot, Save button and save error.
 */

import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { phoneWidth } = vi.hoisted(() => ({ phoneWidth: { value: false } }))

vi.mock('@/hooks/use-mobile-web-shell', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/hooks/use-mobile-web-shell')>()
  return { ...actual, useMobileWebShell: () => phoneWidth.value }
})
vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))

import type { CanvasOpenInfo } from '@shared/types/canvas.types'
import { COLOR_THEME_CHANGED_EVENT } from '@/lib/themes'
import { type CanvasPhase, type CanvasSession, useCanvasStore } from '@/stores/canvas-store'
import { CanvasPanel } from './CanvasPanel'

const PROJECT = 'proj-1'
const DOC = 'C:/proj/design.figcraft'
const INFO: CanvasOpenInfo = {
  canvasId: 'cv0123456789abcdef',
  baseUrl: 'http://127.0.0.1:5199',
  canvasToken: 'cafe',
  pinTag: 'termul-v1',
  docKey: DOC,
  mountSeq: 4
}

function setPhase(phase: CanvasPhase, projection: Partial<CanvasSession> = {}): void {
  useCanvasStore.setState({
    sessions: {
      [PROJECT]: {
        projectId: PROJECT,
        docPath: DOC,
        phase,
        dirty: false,
        contentVersion: null,
        savedVersion: null,
        saving: false,
        saveError: null,
        ...projection
      }
    }
  })
}

function renderPanel() {
  return render(<CanvasPanel projectId={PROJECT} docPath={DOC} isVisible />)
}

const original = useCanvasStore.getState()

beforeEach(() => {
  phoneWidth.value = false
  useCanvasStore.setState(original, true)
})

afterEach(() => {
  useCanvasStore.setState(original, true)
})

describe('CanvasPanel', () => {
  it('mounts the editor at the kiosk URL for the current mount', () => {
    setPhase({ kind: 'loading', mount: { mountSeq: 4, info: INFO } })
    renderPanel()

    const iframe = screen.getByTitle('Canvas — design.figcraft') as HTMLIFrameElement
    const origin = encodeURIComponent(window.location.origin)
    expect(iframe.src).toBe(
      `http://127.0.0.1:5199/canvas/cv0123456789abcdef/?embed=kiosk&ct=cafe&ho=${origin}&ms=4`
    )
    expect(iframe.dataset.canvasMountSeq).toBe('4')
    expect(screen.getByText('Loading editor…')).toBeTruthy()
  })

  it('unmounting the iframe closes its bridge', async () => {
    setPhase({ kind: 'loading', mount: { mountSeq: 4, info: INFO } })
    const view = renderPanel()
    const pending = useCanvasStore.getState().callCanvas(PROJECT, 'document.inspect')

    view.unmount()
    await expect(pending).resolves.toMatchObject({ ok: false, error: { code: 'CANVAS_CLOSED' } })
  })

  it('shows why a mount failed instead of an iframe', () => {
    setPhase({ kind: 'failed', code: 'BUNDLE_PIN_MISMATCH', message: 'Bundle mismatch.' })
    renderPanel()

    expect(screen.getByTestId('canvas-mount-failed').textContent).toBe('Bundle mismatch.')
    expect(screen.queryByTitle(/Canvas —/)).toBeNull()
  })

  it('is gated off on phone-width viewports', () => {
    phoneWidth.value = true
    setPhase({ kind: 'loading', mount: { mountSeq: 4, info: INFO } })
    renderPanel()

    expect(screen.getByTestId('canvas-unsupported-surface')).toBeTruthy()
    expect(screen.queryByTitle(/Canvas —/)).toBeNull()
  })

  it('re-sends theme and locale when they change, without remounting', () => {
    const pushTheme = vi.fn()
    const pushLocale = vi.fn()
    setPhase({ kind: 'ready', mount: { mountSeq: 4, info: INFO } })
    useCanvasStore.setState({ pushTheme, pushLocale })
    renderPanel()
    const iframe = screen.getByTitle('Canvas — design.figcraft')

    act(() => {
      window.dispatchEvent(new CustomEvent(COLOR_THEME_CHANGED_EVENT, { detail: {} }))
      window.dispatchEvent(new Event('languagechange'))
    })

    expect(pushTheme).toHaveBeenCalledWith(PROJECT)
    expect(pushLocale).toHaveBeenCalledWith(PROJECT)
    expect(screen.getByTitle('Canvas — design.figcraft')).toBe(iframe)
  })

  it('shows the unsaved dot while dirty and saves on the Save button', () => {
    const saveCanvas = vi.fn().mockResolvedValue(true)
    setPhase({ kind: 'ready', mount: { mountSeq: 4, info: INFO } }, { dirty: true })
    useCanvasStore.setState({ saveCanvas })
    renderPanel()

    expect(screen.getByLabelText('Unsaved changes')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Save canvas' }))
    expect(saveCanvas).toHaveBeenCalledWith(PROJECT)
  })

  it('clears the dot when saved, disables Save while loading or saving, shows a save error', () => {
    setPhase({ kind: 'loading', mount: { mountSeq: 4, info: INFO } })
    const view = renderPanel()
    expect(screen.queryByLabelText('Unsaved changes')).toBeNull()
    expect(screen.getByRole('button', { name: 'Save canvas' }).hasAttribute('disabled')).toBe(true)
    view.unmount()

    setPhase(
      { kind: 'ready', mount: { mountSeq: 4, info: INFO } },
      { saving: true, saveError: 'could not write the canvas document: disk full' }
    )
    renderPanel()
    expect(screen.getByRole('button', { name: 'Save canvas' }).hasAttribute('disabled')).toBe(true)
    expect(screen.getByTestId('canvas-save-error').getAttribute('title')).toContain('disk full')
  })
})
