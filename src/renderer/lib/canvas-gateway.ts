/**
 * Connects the canvas store to the host gateway (spine AD-21, AD-29): every
 * canvas this renderer has mounted (loading or ready) is reported to the host
 * as held by this connection, and the agent calls the host forwards run
 * through that mount's bridge. Calls made while the editor is still loading
 * wait in the bridge's queue; the host times them out, never this side.
 *
 * `host.save` is the one call answered here instead of in the editor: the
 * agent's `save_file` is Termul's save of the project's document (AD-18,
 * AD-23), reached through `onHostSave`.
 */

import { type AcpTransport, getAcpTransport } from '@/lib/acp-transport'
import {
  answerGatewayCall,
  asJsonText,
  bridgeFailure,
  type CanvasBridgeError,
  type CanvasBridgeReply
} from '@/lib/canvas-bridge'
import { type CanvasSession, useCanvasStore } from '@/stores/canvas-store'

/** The host save's outcome: the saved document's project-relative path. */
export type HostSaveResult = { ok: true; path: string } | { ok: false; error: CanvasBridgeError }

/** Save the project's mounted canvas document now. */
export type HostSave = (projectId: string) => Promise<HostSaveResult>

export interface CanvasGatewayOptions {
  onHostSave?: HostSave
}

const HOST_SAVE = 'host.save'

const unwiredHostSave: HostSave = async () => ({
  ok: false,
  error: { code: 'CANVAS_CLOSED', message: 'saving the canvas is not available yet' }
})

function mountedSeqOf(session: CanvasSession | undefined): number | null {
  const phase = session?.phase
  return phase?.kind === 'loading' || phase?.kind === 'ready' ? phase.mount.mountSeq : null
}

async function hostSaveReply(save: HostSave, projectId: string): Promise<CanvasBridgeReply> {
  try {
    const saved = await save(projectId)
    return saved.ok
      ? { ok: true, result: asJsonText(JSON.stringify({ path: saved.path })) }
      : { ok: false, error: saved.error }
  } catch (error) {
    return bridgeFailure('ENGINE_ERROR', error instanceof Error ? error.message : String(error))
  }
}

/** Start serving gateway calls from this renderer's canvases. Returns the stop. */
export function startCanvasGateway(
  transport: AcpTransport = getAcpTransport(),
  { onHostSave = unwiredHostSave }: CanvasGatewayOptions = {}
): () => void {
  const gateway = transport.canvas
  if (!gateway) return () => {}
  const store = useCanvasStore.getState

  gateway.setCallHandler((call) =>
    answerGatewayCall(call, mountedSeqOf(store().sessions[call.projectId]), (method, params) =>
      method === HOST_SAVE
        ? hostSaveReply(onHostSave, call.projectId)
        : store().callCanvas(call.projectId, method, params)
    )
  )

  const reported = new Set<string>()
  const sync = (sessions: Record<string, CanvasSession>): void => {
    for (const projectId of reported) {
      if (mountedSeqOf(sessions[projectId]) === null) {
        gateway.setMount(projectId, null)
        reported.delete(projectId)
      }
    }
    for (const [projectId, session] of Object.entries(sessions)) {
      const mountSeq = mountedSeqOf(session)
      if (mountSeq === null) continue
      gateway.setMount(projectId, mountSeq)
      reported.add(projectId)
    }
  }
  sync(store().sessions)
  const unsubscribe = useCanvasStore.subscribe((state) => sync(state.sessions))

  return () => {
    unsubscribe()
    gateway.setCallHandler(null)
    for (const projectId of reported) gateway.setMount(projectId, null)
    reported.clear()
  }
}
