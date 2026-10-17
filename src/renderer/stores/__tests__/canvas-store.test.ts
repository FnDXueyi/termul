/**
 * Canvas store: the open/close lifecycle against a mocked facade and the
 * mount state machine against a real bridge talking to a fake editor
 * window, with the real workspace store for the singleton tab.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { open, close, toastError, logFrontendError } = vi.hoisted(() => ({
  open: vi.fn(),
  close: vi.fn(),
  toastError: vi.fn(),
  logFrontendError: vi.fn()
}))

vi.mock('@/lib/canvas-api', () => ({ canvasApi: { open, close } }))
vi.mock('@/lib/log-api', () => ({ logFrontendError }))
vi.mock('sonner', () => ({
  toast: { error: toastError, info: vi.fn(), success: vi.fn(), warning: vi.fn() }
}))

const THEME = {
  mode: 'dark',
  background: '#08090a',
  card: '#0f1011',
  foreground: '#e5e5e6',
  mutedForeground: '#8a8f98',
  border: '#23252a',
  primary: '#3b82f6',
  primaryFill: '#3b82f6',
  destructive: '#ef4444'
}
const readCanvasThemeTokens = vi.hoisted(() => vi.fn())
vi.mock('@/lib/canvas-theme', () => ({ readCanvasThemeTokens }))

import type { CanvasOpenInfo } from '@shared/types/canvas.types'
import type { IpcResult } from '@shared/types/ipc.types'
import { asJsonText, createCanvasBridge } from '@/lib/canvas-bridge'
import type { LeafNode } from '@/types/workspace.types'
import { type CanvasMount, pinVerdict, useCanvasStore } from '../canvas-store'
import { useWorkspaceStore } from '../workspace-store'

const PROJECT = 'proj-1'
const DOC_A = 'C:/proj/design.figcraft'
const DOC_B = 'C:/proj/poster.figcraft'
const CANVAS_ORIGIN = 'http://127.0.0.1:5199'
const PIN = 'termul-v1'

const INFO: CanvasOpenInfo = {
  canvasId: 'cv0123456789abcdef',
  baseUrl: CANVAS_ORIGIN,
  canvasToken: 'token-a',
  pinTag: PIN,
  docKey: DOC_A
}

function openInfo(overrides: Partial<CanvasOpenInfo> = {}): IpcResult<CanvasOpenInfo> {
  return { success: true, data: { ...INFO, ...overrides } }
}

function session() {
  return useCanvasStore.getState().sessions[PROJECT]
}

function currentMount(): CanvasMount {
  const phase = session()?.phase
  if (phase?.kind !== 'loading' && phase?.kind !== 'ready')
    throw new Error(`no mount: ${phase?.kind}`)
  return phase.mount
}

interface Request {
  id: number
  method: string
  params: string
  mountSeq: number
}

/** What `CanvasShell` does for a mount, against a fake editor window. */
function mountEditor() {
  const { mountSeq } = currentMount()
  const posted: Request[] = []
  const contentWindow = {
    postMessage: (data: Request) => posted.push(data)
  } as unknown as Window
  const send = (data: unknown): void => {
    const event = new MessageEvent('message', { data })
    Object.defineProperty(event, 'origin', { value: CANVAS_ORIGIN })
    Object.defineProperty(event, 'source', { value: contentWindow })
    window.dispatchEvent(event)
  }
  const bridge = createCanvasBridge({
    iframe: { contentWindow } as unknown as HTMLIFrameElement,
    canvasOrigin: CANVAS_ORIGIN,
    mountSeq,
    onEvent: (event) => useCanvasStore.getState().handleBridgeEvent(PROJECT, mountSeq, event)
  })
  useCanvasStore.getState().attachBridge(PROJECT, mountSeq, bridge)
  const reply = (method: string, result: string): void => {
    const request = posted.find((r) => r.method === method)
    if (!request) throw new Error(`${method} was not sent`)
    send({ id: request.id, ok: true, result, mountSeq })
  }
  return {
    bridge,
    posted,
    send,
    reply,
    ready: () => send({ method: 'event.ready', params: '{}', mountSeq }),
    sent: (method: string) => posted.filter((r) => r.method === method)
  }
}

async function flush(): Promise<void> {
  for (let i = 0; i < 5; i += 1) await Promise.resolve()
}

function canvasTabs() {
  const root = useWorkspaceStore.getState().root
  return root.type === 'leaf' ? root.tabs.filter((t) => t.type === 'canvas') : []
}

function resetStores(): void {
  useCanvasStore.setState({ sessions: {} })
  useWorkspaceStore.setState(() => {
    const root: LeafNode = { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null }
    return { root, activePaneId: 'pane-root', fullscreenPaneId: null, agentLauncherPaneId: null }
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  readCanvasThemeTokens.mockReturnValue(THEME)
  close.mockResolvedValue({ success: true, data: true })
  resetStores()
})

afterEach(async () => {
  await useCanvasStore.getState().closeCanvas(PROJECT)
  vi.unstubAllEnvs()
  resetStores()
})

describe('open and close', () => {
  it('a successful open starts loading a new mount and focuses the singleton tab', async () => {
    open.mockResolvedValueOnce(openInfo())
    await expect(useCanvasStore.getState().openCanvas(PROJECT, DOC_A)).resolves.toBe(true)

    expect(open).toHaveBeenCalledWith(DOC_A, PROJECT)
    expect(session()?.phase.kind).toBe('loading')
    expect(currentMount().info).toEqual(INFO)
    expect(canvasTabs()).toEqual([
      expect.objectContaining({ id: `canvas-${PROJECT}`, projectId: PROJECT, docPath: DOC_A })
    ])
  })

  it('a repeat open of the same target keeps the mount; a new target remounts', async () => {
    open.mockResolvedValue(openInfo())
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
    const first = currentMount()
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
    expect(currentMount()).toBe(first)

    open.mockResolvedValueOnce(openInfo({ baseUrl: 'http://127.0.0.1:6200' }))
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
    expect(currentMount().mountSeq).toBeGreaterThan(first.mountSeq)

    open.mockResolvedValueOnce(openInfo({ docKey: DOC_B }))
    const beforeRebind = currentMount().mountSeq
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_B)
    expect(session()?.docPath).toBe(DOC_B)
    expect(currentMount().mountSeq).toBeGreaterThan(beforeRebind)
    expect(close).not.toHaveBeenCalled()
    expect(canvasTabs()).toHaveLength(1)
  })

  it('a failed first open leaves no session and no tab', async () => {
    open.mockResolvedValueOnce({
      success: false,
      error: 'canvas open is unavailable on the mobile web shell',
      code: 'UNSUPPORTED_SURFACE'
    })
    await expect(useCanvasStore.getState().openCanvas(PROJECT, DOC_A)).resolves.toBe(false)

    expect(session()).toBeUndefined()
    expect(canvasTabs()).toHaveLength(0)
    expect(toastError).toHaveBeenCalled()
    expect(logFrontendError).toHaveBeenCalledWith(
      expect.objectContaining({ message: expect.stringContaining('UNSUPPORTED_SURFACE') })
    )
  })

  it('a close that lands while the open is in flight wins', async () => {
    let resolveOpen: (value: IpcResult<CanvasOpenInfo>) => void = () => {}
    open.mockReturnValueOnce(new Promise((resolve) => (resolveOpen = resolve)))
    const opening = useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
    await useCanvasStore.getState().closeCanvas(PROJECT)
    resolveOpen(openInfo())

    await expect(opening).resolves.toBe('superseded')
    expect(session()).toBeUndefined()
    expect(canvasTabs()).toHaveLength(0)
  })

  it('close unmounts: pending calls resolve CANVAS_CLOSED and the token is revoked', async () => {
    open.mockResolvedValueOnce(openInfo())
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
    mountEditor()
    const pending = useCanvasStore.getState().callCanvas(PROJECT, 'document.inspect')

    await useCanvasStore.getState().closeCanvas(PROJECT)

    await expect(pending).resolves.toMatchObject({ ok: false, error: { code: 'CANVAS_CLOSED' } })
    expect(close).toHaveBeenCalledWith(PROJECT)
    expect(session()).toBeUndefined()
    await expect(
      useCanvasStore.getState().callCanvas(PROJECT, 'document.inspect')
    ).resolves.toMatchObject({ ok: false, error: { code: 'CANVAS_CLOSED' } })
  })
})

describe('mount', () => {
  beforeEach(async () => {
    open.mockResolvedValueOnce(openInfo())
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
  })

  it('on event.ready checks host.describe, then sends host.theme and host.locale', async () => {
    const editor = mountEditor()
    editor.ready()
    expect(editor.posted.map((r) => r.method)).toEqual(['host.describe'])
    expect(session()?.phase.kind).toBe('loading')

    editor.reply('host.describe', JSON.stringify({ tag: PIN, engineVersion: '0.6.0' }))
    await flush()

    expect(session()?.phase.kind).toBe('ready')
    expect(editor.sent('host.theme').map((r) => JSON.parse(r.params))).toEqual([{ tokens: THEME }])
    expect(editor.sent('host.locale').map((r) => JSON.parse(r.params))).toEqual([
      { bcp47: navigator.language }
    ])
  })

  it('a theme change re-sends host.theme to the same mount, without a remount', async () => {
    const editor = mountEditor()
    editor.ready()
    editor.reply('host.describe', JSON.stringify({ tag: PIN, engineVersion: '0.6.0' }))
    await flush()
    const { mountSeq } = currentMount()

    readCanvasThemeTokens.mockReturnValue({ ...THEME, mode: 'light', background: '#ffffff' })
    useCanvasStore.getState().pushTheme(PROJECT)
    useCanvasStore.getState().pushLocale(PROJECT)

    expect(editor.sent('host.theme').map((r) => JSON.parse(r.params).tokens.mode)).toEqual([
      'dark',
      'light'
    ])
    expect(editor.sent('host.locale')).toHaveLength(2)
    expect(currentMount().mountSeq).toBe(mountSeq)
  })

  it('a pin mismatch is only logged in dev builds', async () => {
    vi.stubEnv('PROD', false)
    const editor = mountEditor()
    editor.ready()
    editor.reply('host.describe', JSON.stringify({ tag: 'dev', engineVersion: '0.6.0' }))
    await flush()

    expect(session()?.phase.kind).toBe('ready')
    expect(logFrontendError).toHaveBeenCalledWith(
      expect.objectContaining({ level: 'warn', message: expect.stringContaining('bundle=dev') })
    )
  })

  it('a pin mismatch refuses the mount in release builds', async () => {
    vi.stubEnv('PROD', true)
    const editor = mountEditor()
    editor.ready()
    editor.reply('host.describe', JSON.stringify({ tag: 'dev', engineVersion: '0.6.0' }))
    await flush()

    expect(session()?.phase).toMatchObject({ kind: 'failed', code: 'BUNDLE_PIN_MISMATCH' })
    expect(editor.sent('host.theme')).toHaveLength(0)
    await expect(editor.bridge.call('document.inspect')).resolves.toMatchObject({
      ok: false,
      error: { code: 'CANVAS_CLOSED' }
    })
  })

  it('forwards agent calls as opaque text and tracks dirty state from events', async () => {
    const editor = mountEditor()
    const params = asJsonText('{"command":"object.rename","params":{"id":18446744073709551615}}')
    const call = useCanvasStore.getState().callCanvas(PROJECT, 'engine.execute', params)
    editor.ready()

    const request = editor.sent('engine.execute')[0]
    expect(request?.params).toBe(params)
    editor.reply('engine.execute', '{"id":18446744073709551615}')
    await expect(call).resolves.toEqual({ ok: true, result: '{"id":18446744073709551615}' })

    editor.send({
      method: 'event.dirtyChanged',
      params: '{"dirty":true,"contentVersion":9007199254740993}',
      mountSeq: currentMount().mountSeq
    })
    expect(session()).toMatchObject({ dirty: true, contentVersion: '9007199254740993' })
  })

  it('ignores events addressed to a previous mount', async () => {
    const stale = mountEditor()
    open.mockResolvedValueOnce(openInfo({ canvasToken: 'token-b' }))
    await useCanvasStore.getState().openCanvas(PROJECT, DOC_A)
    const staleSeq = currentMount().mountSeq - 1

    useCanvasStore.getState().handleBridgeEvent(PROJECT, staleSeq, { method: 'event.ready' })
    useCanvasStore
      .getState()
      .handleBridgeEvent(PROJECT, staleSeq, { method: 'event.crashed', reason: 'old' })

    expect(session()?.phase.kind).toBe('loading')
    expect(stale.posted).toHaveLength(0)
  })

  it('an editor crash fails the mount', () => {
    const editor = mountEditor()
    editor.ready()
    editor.send({
      method: 'event.crashed',
      params: '{"reason":"panic"}',
      mountSeq: currentMount().mountSeq
    })
    expect(session()?.phase).toMatchObject({ kind: 'failed', code: 'EDITOR_CRASHED' })
  })
})

describe('pinVerdict', () => {
  it('matches only the exact pinned tag and refuses only in release', () => {
    expect(pinVerdict('termul-v1', 'termul-v1', true)).toBe('match')
    expect(pinVerdict('dev', 'termul-v1', false)).toBe('mismatch-logged')
    expect(pinVerdict('dev', 'termul-v1', true)).toBe('mismatch-refused')
    expect(pinVerdict(null, 'termul-v1', true)).toBe('mismatch-refused')
  })
})
