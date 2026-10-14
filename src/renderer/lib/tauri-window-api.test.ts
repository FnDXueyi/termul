import { beforeEach, describe, expect, it, vi } from 'vitest'

const { hide, onCloseRequested, platformState, logFrontendError } = vi.hoisted(() => ({
  hide: vi.fn(async () => undefined),
  logFrontendError: vi.fn(async (_payload: unknown) => undefined),
  onCloseRequested: vi.fn(),
  platformState: { isWindows: false }
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    hide,
    onCloseRequested,
    minimize: vi.fn(),
    isMaximized: vi.fn(async () => false),
    maximize: vi.fn(),
    unmaximize: vi.fn(),
    close: vi.fn(),
    destroy: vi.fn(),
    onResized: vi.fn(async () => vi.fn())
  }),
  LogicalPosition: class LogicalPosition {},
  LogicalSize: class LogicalSize {}
}))

vi.mock('./log-api', () => ({
  logFrontendError
}))

vi.mock('./platform', () => ({
  get isWindows() {
    return platformState.isWindows
  }
}))

describe('createTauriWindowApi close behavior', () => {
  beforeEach(() => {
    hide.mockClear()
    logFrontendError.mockClear()
    onCloseRequested.mockReset()
    onCloseRequested.mockResolvedValue(vi.fn())
    platformState.isWindows = false
    ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
  })

  it('hides and prevents close on non-Windows without entering the quit callback', async () => {
    const { createTauriWindowApi } = await import('./tauri-window-api')
    const callback = vi.fn(async () => false)
    createTauriWindowApi().onCloseRequested(callback)

    const closeHandler = onCloseRequested.mock.calls[0]?.[0] as
      | ((event: { preventDefault: () => void }) => Promise<void>)
      | undefined
    const preventDefault = vi.fn()
    await closeHandler?.({ preventDefault })

    expect(preventDefault).toHaveBeenCalledOnce()
    expect(hide).toHaveBeenCalledOnce()
    expect(callback).not.toHaveBeenCalled()
  })

  it('logs hide failures while keeping the non-Windows close prevented', async () => {
    const error = new Error('hide failed')
    hide.mockRejectedValueOnce(error)
    const { createTauriWindowApi } = await import('./tauri-window-api')
    const callback = vi.fn(async () => false)
    createTauriWindowApi().onCloseRequested(callback)

    const closeHandler = onCloseRequested.mock.calls[0]?.[0] as
      | ((event: { preventDefault: () => void }) => Promise<void>)
      | undefined
    const preventDefault = vi.fn()
    await closeHandler?.({ preventDefault })

    expect(preventDefault).toHaveBeenCalledOnce()
    expect(logFrontendError).toHaveBeenCalledWith({
      source: 'tauri-window-api',
      message: 'Failed to hide window to tray: hide failed',
      stack: error.stack
    })
    expect(callback).not.toHaveBeenCalled()
  })

  it('uses the renderer quit callback on Windows', async () => {
    platformState.isWindows = true
    const { createTauriWindowApi } = await import('./tauri-window-api')
    const callback = vi.fn(async () => false)
    createTauriWindowApi().onCloseRequested(callback)

    const closeHandler = onCloseRequested.mock.calls[0]?.[0] as
      | ((event: { preventDefault: () => void }) => Promise<void>)
      | undefined
    const preventDefault = vi.fn()
    await closeHandler?.({ preventDefault })

    expect(callback).toHaveBeenCalledOnce()
    expect(preventDefault).toHaveBeenCalledOnce()
    expect(hide).not.toHaveBeenCalled()
  })

  it('reports an app-close owner only while a close listener is registered', async () => {
    vi.resetModules()
    const { createTauriWindowApi, hasAppCloseOwner } = await import('./tauri-window-api')
    expect(hasAppCloseOwner()).toBe(false)

    const cleanup = createTauriWindowApi().onCloseRequested(vi.fn(async () => false))
    await vi.waitFor(() => expect(hasAppCloseOwner()).toBe(true))

    cleanup()
    expect(hasAppCloseOwner()).toBe(false)
  })

  it('does not count a close listener disposed before it registered', async () => {
    vi.resetModules()
    const { createTauriWindowApi, hasAppCloseOwner } = await import('./tauri-window-api')

    createTauriWindowApi().onCloseRequested(vi.fn(async () => false))()
    await Promise.resolve()
    await Promise.resolve()

    expect(hasAppCloseOwner()).toBe(false)
  })
})
