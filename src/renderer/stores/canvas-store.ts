/**
 * Canvas store — one figcraft mount per project (AD-7, AD-19).
 *
 * A session's `phase` is the mount state machine:
 * `opening` (facade open in flight) → `loading` (iframe navigating, bridge
 * attached; on `event.ready` the pin is checked and the document loaded) →
 * `ready` (document loaded, theme and locale sent), `failed` (mount
 * refused, load failed or editor crashed) or `unloaded` (no mount: the host
 * evicted it to stay under `canvas.max_mounted` (AD-26), or an open was
 * refused because the canvas that had to make room could not be saved). The host allocates `mountSeq` on
 * every open and never reuses it, so a remount cannot accept the previous
 * iframe's messages.
 *
 * Persistence is the host's (AD-23): this store holds only the projection
 * `{dirty, contentVersion, savedVersion, saving, saveError}`. It forwards
 * `event.dirtyChanged` to the facade, which answers once the host's debounce
 * settles; when the host says a save is due, or the user saves, it runs
 * `host.snapshot` → facade write → `host.markSaved`.
 *
 * The host ranks eviction by activity, so a visible `CanvasPanel` reports
 * itself through `setCanvasVisible`; a report the host answers
 * `CANVAS_CLOSED` means the mount is gone and the session becomes
 * `unloaded`.
 *
 * A mount outlives its tab (AD-19): closing the tab only hides the iframe
 * (`CanvasMountLayer` keeps it offscreen), so agent calls still reach it and
 * reopening the tab shows the same mount. A session ends only through
 * `unmountCanvas`, which asks the host to run the AD-31 sequence (quiesce,
 * save, verify, unmount) and drops the session once the host unmounted.
 *
 * The host leases each project's canvas to one window (AD-30). When another
 * window opens it, the host saves and unmounts this window's mount and the
 * session moves to `elsewhere`; opening it again here takes it back the same
 * way.
 *
 * Bridges live in a module-level map, not in reactive state: they are
 * imperative controllers that `CanvasShell` attaches and detaches with the
 * iframe. Logs never carry the `ct` token or document content.
 */

import type {
  CanvasCloseOutcome,
  CanvasErrorCode,
  CanvasLeaseLoss,
  CanvasOpenInfo,
  CanvasPersistenceView
} from '@shared/types/canvas.types'
import type { IpcResult } from '@shared/types/ipc.types'
import { toast } from 'sonner'
import { create } from 'zustand'
import { canvasApi } from '@/lib/canvas-api'
import {
  asJsonText,
  bridgeFailure,
  type CanvasBridge,
  type CanvasBridgeEvent,
  type CanvasBridgeReply,
  type JsonText
} from '@/lib/canvas-bridge'
import { readCanvasThemeTokens } from '@/lib/canvas-theme'
import { logFrontendError } from '@/lib/log-api'
import { useAppSettingsStore } from '@/stores/app-settings-store'
import { useWorkspaceStore } from '@/stores/workspace-store'

export interface CanvasMount {
  mountSeq: number
  info: CanvasOpenInfo
}

export type CanvasPhase =
  | { kind: 'opening' }
  | { kind: 'loading'; mount: CanvasMount }
  | { kind: 'ready'; mount: CanvasMount }
  | { kind: 'failed'; code: CanvasErrorCode; message: string }
  /** Another window took the canvas's lease; this one's mount was saved and
   * unmounted. Opening the canvas again takes it back. */
  | { kind: 'elsewhere' }
  | { kind: 'unloaded'; reason: UnloadReason; message: string | null }

/** `evicted`: unloaded to save memory. `blocked`: an open was refused
 * because the canvas that had to make room could not be saved. */
export type UnloadReason = 'evicted' | 'blocked'

export interface CanvasSession {
  projectId: string
  docPath: string
  phase: CanvasPhase
  dirty: boolean
  /** The engine's u64 content version, kept as decimal text. */
  contentVersion: string | null
  /** The version last written to disk; `null` until the file exists. */
  savedVersion: string | null
  saving: boolean
  saveError: string | null
  /** The host is running the unmount sequence for this mount. */
  closing: boolean
}

export interface CanvasState {
  sessions: Record<string, CanvasSession>
  /** True when the project has a canvas session afterwards; false on a typed
   * failure (callers fall back); 'superseded' when a newer open or close
   * took over the project while this one was in flight. */
  openCanvas: (projectId: string, docPath: string) => Promise<boolean | 'superseded'>
  /** Unmount through the host's AD-31 sequence; the session ends only when
   * the host unmounted. `discard` is the user's choice to drop unsaved
   * changes after a failed save. */
  unmountCanvas: (projectId: string, discard?: boolean) => Promise<IpcResult<CanvasCloseOutcome>>
  /** This window no longer holds the project's mount: another window took
   * it (`elsewhere`), or the host dropped it after the connection was gone
   * longer than the lease grace (`expired`). */
  leaseLost: (projectId: string, mountSeq: number, loss: CanvasLeaseLoss) => void
  /** The host no longer holds this project's mount: drop the editor and show
   * the unloaded state. */
  markUnloaded: (
    projectId: string,
    reason: UnloadReason,
    message?: string,
    docPath?: string
  ) => void
  /** Tell the host the user started or stopped looking at the canvas. */
  setCanvasVisible: (projectId: string, mountSeq: number, visible: boolean) => Promise<void>
  attachBridge: (projectId: string, mountSeq: number, bridge: CanvasBridge) => void
  detachBridge: (projectId: string, bridge: CanvasBridge) => void
  handleBridgeEvent: (projectId: string, mountSeq: number, event: CanvasBridgeEvent) => void
  /** Forward one control-protocol call to the project's editor. */
  callCanvas: (projectId: string, method: string, params?: JsonText) => Promise<CanvasBridgeReply>
  /** User Save: snapshot and write now, regardless of the debounce. True
   * when the file on disk holds this snapshot or a newer one. */
  saveCanvas: (projectId: string) => Promise<boolean>
  pushTheme: (projectId: string) => void
  pushLocale: (projectId: string) => void
}

const bridges = new Map<string, { mountSeq: number; bridge: CanvasBridge }>()
const lifecycleGenerations = new Map<string, number>()
const savesInFlight = new Map<string, number>()

const CLEAN_PROJECTION = {
  dirty: false,
  contentVersion: null,
  savedVersion: null,
  saving: false,
  saveError: null,
  closing: false
} as const satisfies Partial<CanvasSession>

function nextLifecycleGeneration(projectId: string): number {
  const next = (lifecycleGenerations.get(projectId) ?? 0) + 1
  lifecycleGenerations.set(projectId, next)
  return next
}

function isStaleLifecycle(projectId: string, generation: number): boolean {
  return lifecycleGenerations.get(projectId) !== generation
}

function mountOf(phase: CanvasPhase): CanvasMount | null {
  return phase.kind === 'loading' || phase.kind === 'ready' ? phase.mount : null
}

function currentBridge(projectId: string, mountSeq: number): CanvasBridge | null {
  const entry = bridges.get(projectId)
  return entry && entry.mountSeq === mountSeq ? entry.bridge : null
}

function projectionOf(view: CanvasPersistenceView): Partial<CanvasSession> {
  return {
    dirty: view.dirty,
    contentVersion: view.contentVersion,
    savedVersion: view.savedVersion,
    saveError: view.saveError
  }
}

export type PinVerdict = 'match' | 'mismatch-logged' | 'mismatch-refused'

/** AD-22: a bundle whose `host.describe` tag differs from the pin is refused
 * in release builds and only logged in dev, where the bundle is a local
 * build tagged `dev`. */
export function pinVerdict(
  describedTag: string | null,
  pinTag: string,
  release: boolean
): PinVerdict {
  if (describedTag === pinTag) return 'match'
  return release ? 'mismatch-refused' : 'mismatch-logged'
}

function describedTag(reply: CanvasBridgeReply): string | null {
  if (!reply.ok) return null
  try {
    const tag = (JSON.parse(reply.result) as { tag?: unknown } | null)?.tag
    return typeof tag === 'string' ? tag : null
  } catch {
    return null
  }
}

function logCanvas(level: 'info' | 'warn', source: string, message: string): void {
  void logFrontendError({ level, source: `canvas-store.${source}`, message })
}

export const useCanvasStore = create<CanvasState>((set, get) => {
  const patchSession = (projectId: string, patch: Partial<CanvasSession>): void => {
    set((state) => {
      const existing = state.sessions[projectId]
      if (!existing) return {}
      return { sessions: { ...state.sessions, [projectId]: { ...existing, ...patch } } }
    })
  }

  const phaseMountSeq = (projectId: string): number | null => {
    const session = get().sessions[projectId]
    return session ? (mountOf(session.phase)?.mountSeq ?? null) : null
  }

  const isLiveMount = (projectId: string, mountSeq: number): boolean =>
    currentBridge(projectId, mountSeq) !== null && phaseMountSeq(projectId) === mountSeq

  /** Ends this window's hold on the project's mount: in-flight work for it
   * goes stale and its bridge is disposed. */
  const releaseMount = (projectId: string): void => {
    nextLifecycleGeneration(projectId)
    bridges.get(projectId)?.bridge.dispose()
    bridges.delete(projectId)
    savesInFlight.delete(projectId)
  }

  const dropSession = (projectId: string): void => {
    releaseMount(projectId)
    set((state) => {
      const { [projectId]: _closed, ...rest } = state.sessions
      return { sessions: rest }
    })
  }

  const failMount = (projectId: string, code: CanvasErrorCode, message: string): void => {
    bridges.get(projectId)?.bridge.dispose()
    bridges.delete(projectId)
    patchSession(projectId, { phase: { kind: 'failed', code, message } })
  }

  /** AD-31: the document is loaded before anything can be dirty or saved. A
   * missing file is a new document; its first save creates it. */
  const loadDocument = async (
    projectId: string,
    mount: CanvasMount,
    bridge: CanvasBridge
  ): Promise<CanvasPersistenceView | null> => {
    const fail = (message: string): null => {
      logCanvas('warn', 'load', `canvas document load failed for project ${projectId}: ${message}`)
      failMount(projectId, 'DOCUMENT_LOAD_FAILED', `Could not load the canvas: ${message}`)
      return null
    }
    const stale = (): boolean => currentBridge(projectId, mount.mountSeq) !== bridge
    const read = await canvasApi.readDocument(projectId, mount.mountSeq)
    if (stale()) return null
    if (!read.success) return fail(`${read.code} ${read.error}`)
    let ack: string | null = null
    if (read.data.bytes !== null) {
      const opened = await bridge.call(
        'host.openDocument',
        asJsonText(JSON.stringify({ bytes: read.data.bytes }))
      )
      if (stale()) return null
      if (!opened.ok) return fail(`${opened.error.code} ${opened.error.message}`)
      ack = opened.result
    }
    const loaded = await canvasApi.documentLoaded(projectId, mount.mountSeq, ack)
    if (stale()) return null
    if (!loaded.success) return fail(`${loaded.code} ${loaded.error}`)
    return loaded.data
  }

  const completeMount = async (projectId: string, mount: CanvasMount): Promise<void> => {
    const bridge = currentBridge(projectId, mount.mountSeq)
    if (!bridge) return
    const tag = describedTag(await bridge.call('host.describe'))
    if (currentBridge(projectId, mount.mountSeq) !== bridge) return
    const verdict = pinVerdict(tag, mount.info.pinTag, import.meta.env.PROD)
    if (verdict === 'mismatch-refused') {
      logCanvas(
        'warn',
        'mount',
        `refused the canvas mount for project ${projectId}: bundle tag ${tag} does not match the pinned ${mount.info.pinTag}`
      )
      failMount(
        projectId,
        'BUNDLE_PIN_MISMATCH',
        `The canvas bundle (${tag ?? 'unknown'}) does not match this build (${mount.info.pinTag}).`
      )
      return
    }
    const view = await loadDocument(projectId, mount, bridge)
    if (view === null) return
    logCanvas(
      verdict === 'match' ? 'info' : 'warn',
      'mount',
      `canvas ready for project ${projectId} mountSeq=${mount.mountSeq} bundle=${tag} pin=${mount.info.pinTag} saved=${view.savedVersion ?? 'new'}`
    )
    patchSession(projectId, { phase: { kind: 'ready', mount }, ...projectionOf(view) })
    get().pushTheme(projectId)
    get().pushLocale(projectId)
  }

  const setSaving = (projectId: string, delta: 1 | -1): void => {
    const count = (savesInFlight.get(projectId) ?? 0) + delta
    if (count > 0) savesInFlight.set(projectId, count)
    else savesInFlight.delete(projectId)
    patchSession(projectId, { saving: count > 0 })
  }

  /** `host.snapshot` → host write → `host.markSaved`. The host serializes
   * writes and drops a snapshot older than what it already saved. */
  const runSave = async (projectId: string, mountSeq: number): Promise<boolean> => {
    const bridge = currentBridge(projectId, mountSeq)
    if (!bridge) return false
    const failed = (message: string): false => {
      logCanvas('warn', 'save', `canvas save failed for project ${projectId}: ${message}`)
      if (isLiveMount(projectId, mountSeq)) patchSession(projectId, { saveError: message })
      return false
    }
    setSaving(projectId, 1)
    try {
      const snapshot = await bridge.call('host.snapshot')
      if (!snapshot.ok) return failed(`${snapshot.error.code} ${snapshot.error.message}`)
      const written = await canvasApi.writeSnapshot(projectId, mountSeq, snapshot.result)
      if (!isLiveMount(projectId, mountSeq)) return false
      if (!written.success) return failed(`${written.code} ${written.error}`)
      patchSession(projectId, projectionOf(written.data.view))
      const { markSaved } = written.data
      if (markSaved === null) {
        const { saveError } = written.data.view
        return saveError ? failed(saveError) : true
      }
      const marked = await bridge.call(
        'host.markSaved',
        asJsonText(`{"contentVersion":${markSaved}}`)
      )
      if (!marked.ok) {
        logCanvas(
          'warn',
          'save',
          `host.markSaved failed for project ${projectId}: ${marked.error.code}`
        )
      }
      return true
    } finally {
      setSaving(projectId, -1)
    }
  }

  /** The host owns the debounce: this resolves once it settles, and a later
   * notification supersedes an earlier one. */
  const forwardDirty = async (
    projectId: string,
    mountSeq: number,
    dirty: boolean,
    contentVersion: string
  ): Promise<void> => {
    const result = await canvasApi.dirtyChanged(projectId, mountSeq, dirty, contentVersion)
    if (!isLiveMount(projectId, mountSeq)) return
    if (!result.success) {
      logCanvas(
        'warn',
        'dirtyChanged',
        `canvas dirty notification failed for project ${projectId}: ${result.code}`
      )
      return
    }
    const outcome = result.data
    if (outcome.kind === 'superseded') return
    patchSession(projectId, projectionOf(outcome.view))
    if (outcome.kind === 'saveDue') await runSave(projectId, mountSeq)
  }

  const sendHostCall = (projectId: string, method: string, params: () => JsonText): void => {
    const session = get().sessions[projectId]
    if (session?.phase.kind !== 'ready') return
    const bridge = currentBridge(projectId, session.phase.mount.mountSeq)
    if (!bridge) return
    let body: JsonText
    try {
      body = params()
    } catch (error) {
      logCanvas('warn', method, `could not build ${method}: ${String(error)}`)
      return
    }
    void bridge.call(method, body).then((reply) => {
      if (!reply.ok) {
        logCanvas('warn', method, `${method} failed for project ${projectId}: ${reply.error.code}`)
      }
    })
  }

  return {
    sessions: {},

    openCanvas: async (projectId, docPath) => {
      if (!projectId || !docPath) return false
      const existing = get().sessions[projectId]
      if (existing && existing.docPath === docPath && mountOf(existing.phase)) {
        useWorkspaceStore.getState().addCanvasTab(projectId, docPath)
        return true
      }
      if (existing && mountOf(existing.phase)) {
        const closed = await get().unmountCanvas(projectId)
        if (!closed.success) {
          toast.error(`Could not close the open canvas: ${closed.error}`)
          return false
        }
        return get().openCanvas(projectId, docPath)
      }
      const generation = nextLifecycleGeneration(projectId)
      const resuming = existing?.phase.kind === 'unloaded'
      if (existing === undefined) {
        set((state) => ({
          sessions: {
            ...state.sessions,
            [projectId]: { projectId, docPath, phase: { kind: 'opening' }, ...CLEAN_PROJECTION }
          }
        }))
      } else if (resuming) {
        patchSession(projectId, { docPath, phase: { kind: 'opening' } })
      }
      const { canvasAutosaveMs, canvasLeaseGraceMs, canvasMaxMounted } =
        useAppSettingsStore.getState().settings
      const result = await canvasApi.open(docPath, projectId, {
        autosaveMs: canvasAutosaveMs,
        leaseGraceMs: canvasLeaseGraceMs,
        maxMounted: canvasMaxMounted
      })
      if (result.success) {
        for (const eviction of result.data.evicted) {
          logCanvas(
            'info',
            'openCanvas',
            `canvas for project ${eviction.projectId} was unloaded: ${eviction.reason}`
          )
          get().markUnloaded(eviction.projectId, 'evicted')
        }
      }
      if (isStaleLifecycle(projectId, generation)) {
        logCanvas('info', 'openCanvas', `dropped a superseded canvas open for project ${projectId}`)
        return 'superseded'
      }
      if (!result.success) {
        logCanvas(
          'warn',
          'openCanvas',
          `canvas open failed for project ${projectId}: ${result.code} ${result.error}`
        )
        toast.error(`Could not open the canvas: ${result.error}`)
        if (result.code === 'EVICTION_BLOCKED') {
          get().markUnloaded(projectId, 'blocked', result.error, docPath)
          useWorkspaceStore.getState().addCanvasTab(projectId, docPath)
        } else if (existing === undefined) {
          set((state) => {
            const { [projectId]: _dropped, ...rest } = state.sessions
            return { sessions: rest }
          })
        } else if (resuming) {
          patchSession(projectId, { phase: existing.phase })
        }
        return false
      }
      const mount: CanvasMount = { mountSeq: result.data.mountSeq, info: result.data }
      set((state) => ({
        sessions: {
          ...state.sessions,
          [projectId]: {
            projectId,
            docPath,
            phase: { kind: 'loading', mount },
            ...CLEAN_PROJECTION
          }
        }
      }))
      useWorkspaceStore.getState().addCanvasTab(projectId, docPath)
      return true
    },

    unmountCanvas: async (projectId, discard = false) => {
      const session = get().sessions[projectId]
      if (!session || session.phase.kind === 'elsewhere') {
        // The other window holds the host's mount; closing here must not close it.
        if (session) dropSession(projectId)
        return {
          success: true,
          data: { unmounted: false, savedVersion: null, discarded: false }
        }
      }
      patchSession(projectId, { closing: true })
      const result = await canvasApi.close(projectId, discard)
      if (!result.success) {
        logCanvas(
          'warn',
          'unmountCanvas',
          `canvas unmount refused for project ${projectId}: ${result.code} ${result.error}`
        )
        patchSession(projectId, { closing: false })
        return result
      }
      dropSession(projectId)
      logCanvas(
        result.data.discarded ? 'warn' : 'info',
        'unmountCanvas',
        `canvas unmounted for project ${projectId} saved=${result.data.savedVersion ?? 'none'} discarded=${result.data.discarded}`
      )
      return result
    },

    leaseLost: (projectId, mountSeq, loss) => {
      if (phaseMountSeq(projectId) !== mountSeq) return
      releaseMount(projectId)
      const phase: CanvasPhase =
        loss === 'elsewhere'
          ? { kind: 'elsewhere' }
          : {
              kind: 'failed',
              code: 'CANVAS_CLOSED',
              message:
                'The canvas closed after this window was disconnected for too long. Open it again to continue from its last save.'
            }
      patchSession(projectId, { phase, ...CLEAN_PROJECTION })
      logCanvas(
        loss === 'elsewhere' ? 'info' : 'warn',
        'leaseLost',
        `canvas for project ${projectId} mountSeq=${mountSeq} lost its lease: ${loss}`
      )
    },

    markUnloaded: (projectId, reason, message, docPath) => {
      const existing = get().sessions[projectId]
      if (!existing && docPath === undefined) return
      releaseMount(projectId)
      const unloaded: CanvasSession = {
        projectId,
        docPath: docPath ?? existing?.docPath ?? '',
        phase: { kind: 'unloaded', reason, message: message ?? null },
        ...CLEAN_PROJECTION
      }
      set((state) => ({ sessions: { ...state.sessions, [projectId]: unloaded } }))
    },

    setCanvasVisible: async (projectId, mountSeq, visible) => {
      const result = await canvasApi.setVisible(projectId, mountSeq, visible)
      if (result.success || !visible || result.code !== 'CANVAS_CLOSED') return
      if (phaseMountSeq(projectId) !== mountSeq) return
      logCanvas(
        'info',
        'setCanvasVisible',
        `the host no longer holds the canvas of project ${projectId}`
      )
      get().markUnloaded(projectId, 'evicted')
    },

    attachBridge: (projectId, mountSeq, bridge) => {
      const previous = bridges.get(projectId)
      if (previous && previous.bridge !== bridge) previous.bridge.dispose()
      bridges.set(projectId, { mountSeq, bridge })
    },

    detachBridge: (projectId, bridge) => {
      bridge.dispose()
      if (bridges.get(projectId)?.bridge === bridge) bridges.delete(projectId)
    },

    handleBridgeEvent: (projectId, mountSeq, event) => {
      if (!isLiveMount(projectId, mountSeq)) return
      const session = get().sessions[projectId]
      switch (event.method) {
        case 'event.ready':
          if (session?.phase.kind === 'loading') void completeMount(projectId, session.phase.mount)
          return
        case 'event.dirtyChanged':
          // AD-31: dirty state starts once the document is loaded.
          if (session?.phase.kind !== 'ready') return
          patchSession(projectId, { dirty: event.dirty, contentVersion: event.contentVersion })
          void forwardDirty(projectId, mountSeq, event.dirty, event.contentVersion)
          return
        case 'event.crashed':
          logCanvas('warn', 'crashed', `canvas editor crashed for project ${projectId}`)
          failMount(projectId, 'EDITOR_CRASHED', 'The canvas editor stopped working.')
          return
        default: {
          const unhandled: never = event
          return unhandled
        }
      }
    },

    callCanvas: (projectId, method, params) => {
      const entry = bridges.get(projectId)
      if (!entry || phaseMountSeq(projectId) !== entry.mountSeq) {
        return Promise.resolve(bridgeFailure('CANVAS_CLOSED', 'no canvas is mounted'))
      }
      return entry.bridge.call(method, params)
    },

    saveCanvas: async (projectId) => {
      const phase = get().sessions[projectId]?.phase
      if (phase?.kind !== 'ready') return false
      const saved = await runSave(projectId, phase.mount.mountSeq)
      if (!saved && isLiveMount(projectId, phase.mount.mountSeq)) {
        toast.error('Could not save the canvas.')
      }
      return saved
    },

    pushTheme: (projectId) => {
      sendHostCall(projectId, 'host.theme', () =>
        asJsonText(JSON.stringify({ tokens: readCanvasThemeTokens() }))
      )
    },

    pushLocale: (projectId) => {
      sendHostCall(projectId, 'host.locale', () =>
        asJsonText(JSON.stringify({ bcp47: navigator.language || 'en-US' }))
      )
    }
  }
})

if (import.meta.env.DEV && typeof window !== 'undefined') {
  // Dev builds only: lets Playwright/CDP drive a mounted editor through the
  // real bridge until the MCP gateway (E3) is the caller.
  Object.assign(window, {
    __termulCanvasCall: (projectId: string, method: string, params?: string) =>
      useCanvasStore
        .getState()
        .callCanvas(projectId, method, params === undefined ? undefined : asJsonText(params))
  })
}

export function useCanvasSession(projectId: string | null): CanvasSession | null {
  return useCanvasStore((state) => (projectId ? (state.sessions[projectId] ?? null) : null))
}
