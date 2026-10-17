/**
 * Canvas store — one figcraft mount per project (AD-7, AD-19).
 *
 * A session's `phase` is the mount state machine:
 * `opening` (facade open in flight) → `loading` (iframe navigating, bridge
 * attached, waiting for `event.ready`) → `ready` (`host.describe` matched the
 * pin, theme and locale sent) or `failed` (mount refused or editor crashed).
 * A new `mountSeq` is allocated per mount and never reused, so a remount
 * cannot accept the previous iframe's messages.
 *
 * Bridges live in a module-level map, not in reactive state: they are
 * imperative controllers that `CanvasShell` attaches and detaches with the
 * iframe. Logs never carry the `ct` token or document content.
 */

import type { CanvasErrorCode, CanvasOpenInfo } from '@shared/types/canvas.types'
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

export interface CanvasSession {
  projectId: string
  docPath: string
  phase: CanvasPhase
  dirty: boolean
  /** The engine's u64 content version, kept as text. */
  contentVersion: JsonText | null
}

export interface CanvasState {
  sessions: Record<string, CanvasSession>
  /** True when the project has a canvas session afterwards; false on a typed
   * failure (callers fall back); 'superseded' when a newer open or close
   * took over the project while this one was in flight. */
  openCanvas: (projectId: string, docPath: string) => Promise<boolean | 'superseded'>
  closeCanvas: (projectId: string) => Promise<void>
  attachBridge: (projectId: string, mountSeq: number, bridge: CanvasBridge) => void
  detachBridge: (projectId: string, bridge: CanvasBridge) => void
  handleBridgeEvent: (projectId: string, mountSeq: number, event: CanvasBridgeEvent) => void
  /** Forward one control-protocol call to the project's editor. */
  callCanvas: (projectId: string, method: string, params?: JsonText) => Promise<CanvasBridgeReply>
  pushTheme: (projectId: string) => void
  pushLocale: (projectId: string) => void
}

const bridges = new Map<string, { mountSeq: number; bridge: CanvasBridge }>()
const lastMountSeq = new Map<string, number>()
const lifecycleGenerations = new Map<string, number>()

function nextMountSeq(projectId: string): number {
  const next = (lastMountSeq.get(projectId) ?? 0) + 1
  lastMountSeq.set(projectId, next)
  return next
}

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

function sameTarget(a: CanvasOpenInfo, b: CanvasOpenInfo): boolean {
  return a.baseUrl === b.baseUrl && a.canvasId === b.canvasId && a.canvasToken === b.canvasToken
}

function currentBridge(projectId: string, mountSeq: number): CanvasBridge | null {
  const entry = bridges.get(projectId)
  return entry && entry.mountSeq === mountSeq ? entry.bridge : null
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

  const failMount = (projectId: string, code: CanvasErrorCode, message: string): void => {
    bridges.get(projectId)?.bridge.dispose()
    bridges.delete(projectId)
    patchSession(projectId, { phase: { kind: 'failed', code, message } })
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
    logCanvas(
      verdict === 'match' ? 'info' : 'warn',
      'mount',
      `canvas ready for project ${projectId} mountSeq=${mount.mountSeq} bundle=${tag} pin=${mount.info.pinTag}`
    )
    patchSession(projectId, { phase: { kind: 'ready', mount } })
    get().pushTheme(projectId)
    get().pushLocale(projectId)
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
      const generation = nextLifecycleGeneration(projectId)
      const existing = get().sessions[projectId]
      if (existing === undefined) {
        set((state) => ({
          sessions: {
            ...state.sessions,
            [projectId]: {
              projectId,
              docPath,
              phase: { kind: 'opening' },
              dirty: false,
              contentVersion: null
            }
          }
        }))
      }
      const result = await canvasApi.open(docPath, projectId)
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
        if (existing === undefined) {
          set((state) => {
            const { [projectId]: _dropped, ...rest } = state.sessions
            return { sessions: rest }
          })
        }
        return false
      }
      const liveMount = existing && existing.docPath === docPath ? mountOf(existing.phase) : null
      if (!liveMount || !sameTarget(liveMount.info, result.data)) {
        const mount: CanvasMount = { mountSeq: nextMountSeq(projectId), info: result.data }
        set((state) => ({
          sessions: {
            ...state.sessions,
            [projectId]: {
              projectId,
              docPath,
              phase: { kind: 'loading', mount },
              dirty: false,
              contentVersion: null
            }
          }
        }))
      }
      useWorkspaceStore.getState().addCanvasTab(projectId, docPath)
      return true
    },

    closeCanvas: async (projectId) => {
      if (!get().sessions[projectId]) return
      const generation = nextLifecycleGeneration(projectId)
      bridges.get(projectId)?.bridge.dispose()
      bridges.delete(projectId)
      set((state) => {
        const { [projectId]: _closed, ...rest } = state.sessions
        return { sessions: rest }
      })
      const result = await canvasApi.close(projectId)
      if (!result.success && !isStaleLifecycle(projectId, generation)) {
        logCanvas(
          'warn',
          'closeCanvas',
          `canvas close failed for project ${projectId}: ${result.code} ${result.error}`
        )
      }
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
      if (!currentBridge(projectId, mountSeq) || phaseMountSeq(projectId) !== mountSeq) return
      switch (event.method) {
        case 'event.ready': {
          const session = get().sessions[projectId]
          if (session?.phase.kind === 'loading') void completeMount(projectId, session.phase.mount)
          return
        }
        case 'event.dirtyChanged':
          patchSession(projectId, { dirty: event.dirty, contentVersion: event.contentVersion })
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
