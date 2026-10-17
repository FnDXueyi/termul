/**
 * Connects the canvas store to the host gateway (spine AD-21, AD-29): every
 * canvas this renderer has mounted (loading or ready) is reported to the host
 * as held by this connection, and the agent calls the host forwards run
 * through that mount's bridge. Calls made while the editor is still loading
 * wait in the bridge's queue; the host times them out, never this side.
 */

import { type AcpTransport, getAcpTransport } from '@/lib/acp-transport'
import { answerGatewayCall } from '@/lib/canvas-bridge'
import { type CanvasSession, useCanvasStore } from '@/stores/canvas-store'

function mountedSeqOf(session: CanvasSession | undefined): number | null {
  const phase = session?.phase
  return phase?.kind === 'loading' || phase?.kind === 'ready' ? phase.mount.mountSeq : null
}

/** Start serving gateway calls from this renderer's canvases. Returns the stop. */
export function startCanvasGateway(transport: AcpTransport = getAcpTransport()): () => void {
  const gateway = transport.canvas
  if (!gateway) return () => {}
  const store = useCanvasStore.getState

  gateway.setCallHandler((call) =>
    answerGatewayCall(call, mountedSeqOf(store().sessions[call.projectId]), (method, params) =>
      store().callCanvas(call.projectId, method, params)
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
