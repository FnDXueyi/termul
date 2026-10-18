/**
 * The renderer leg of the canvas gateway (spine AD-21, AD-29) on `/ws`, the
 * same on the desktop Channel and a browser socket.
 *
 * It tells the host which project canvases this renderer holds
 * (`canvas_mount` / `canvas_unmount`) and re-reports them on every
 * (re)connect, because the host keys a mount by connection. Each
 * `canvas_call` frame goes to the registered handler and its answer goes back
 * as `canvas_reply`. It never times a call out: the host owns the timeout, and
 * a reply the host no longer waits for is simply ignored there.
 *
 * A mount this renderer no longer holds the lease of (spine AD-30) goes to the
 * lease-lost handler: the host says so with a `canvas_lease_lost` frame when
 * another window takes the canvas or the mount cap evicts it, or refuses the
 * re-report after a reconnect that came too late.
 */

import type { CanvasLeaseLoss } from '@shared/types/canvas.types'
import type {
  WsCanvasCallFrame,
  WsCanvasLeaseLostFrame,
  WsCanvasMountReply,
  WsCanvasReplyPayload,
  WsRequestType
} from '@shared/types/web-protocol.types'
import { logFrontendError } from '@/lib/log-api'

type CanvasRequestType = Extract<WsRequestType, 'canvas_mount' | 'canvas_unmount' | 'canvas_reply'>

/** What the client needs from its transport. */
export interface CanvasGatewayTransport {
  request(type: CanvasRequestType, payload: unknown): Promise<unknown>
  /** Whether the socket is open and authenticated. */
  isLive(): boolean
}

/** Runs one forwarded call in the mounted editor. Must not reject. */
export type CanvasCallHandler = (call: WsCanvasCallFrame) => Promise<WsCanvasReplyPayload>

/** This renderer no longer holds `projectId`'s canvas at `mountSeq`. */
export type CanvasLeaseLostHandler = (
  projectId: string,
  mountSeq: number,
  loss: CanvasLeaseLoss
) => void

function closedReply(call: WsCanvasCallFrame, message: string): WsCanvasReplyPayload {
  return {
    callId: call.callId,
    mountSeq: call.mountSeq,
    ok: false,
    error: { code: 'CANVAS_CLOSED', message }
  }
}

export class CanvasGatewayClient {
  /** projectId → the mountSeq this renderer holds. */
  private readonly mounts = new Map<string, number>()
  private handler: CanvasCallHandler | null = null
  private leaseLost: CanvasLeaseLostHandler | null = null

  constructor(private readonly transport: CanvasGatewayTransport) {}

  setCallHandler(handler: CanvasCallHandler | null): void {
    this.handler = handler
  }

  setLeaseLostHandler(handler: CanvasLeaseLostHandler | null): void {
    this.leaseLost = handler
  }

  /** This renderer holds `projectId`'s canvas at `mountSeq`, or no longer (`null`). */
  setMount(projectId: string, mountSeq: number | null): void {
    const held = this.mounts.get(projectId)
    if (mountSeq === null) {
      if (held === undefined) return
      this.mounts.delete(projectId)
      void this.send('canvas_unmount', { projectId, mountSeq: held })
      return
    }
    if (held === mountSeq) return
    this.mounts.set(projectId, mountSeq)
    this.report(projectId, mountSeq)
  }

  /** A new connection holds nothing until told again. */
  onConnected(): void {
    for (const [projectId, mountSeq] of this.mounts) this.report(projectId, mountSeq)
  }

  handleLeaseLost(frame: WsCanvasLeaseLostFrame): void {
    this.lose(frame.projectId, frame.mountSeq, frame.reason === 'evicted' ? 'evicted' : 'elsewhere')
  }

  private report(projectId: string, mountSeq: number): void {
    void this.send('canvas_mount', { projectId, mountSeq }).then((reply) => {
      const refused = reply as Partial<WsCanvasMountReply> | undefined
      if (refused?.attached !== false) return
      this.lose(projectId, mountSeq, refused.heldElsewhere ? 'elsewhere' : 'expired')
    })
  }

  private lose(projectId: string, mountSeq: number, loss: CanvasLeaseLoss): void {
    if (this.mounts.get(projectId) !== mountSeq) return
    this.mounts.delete(projectId)
    this.leaseLost?.(projectId, mountSeq, loss)
  }

  handleFrame(call: WsCanvasCallFrame): void {
    const handler = this.handler
    const answer = handler
      ? handler(call).catch((error: unknown) => ({
          callId: call.callId,
          mountSeq: call.mountSeq,
          ok: false as const,
          error: { code: 'ENGINE_ERROR', message: String(error) }
        }))
      : Promise.resolve(closedReply(call, 'no canvas is mounted in this window'))
    void answer.then((reply) => this.send('canvas_reply', reply))
  }

  /** The host's reply, or `undefined` when nothing was sent or it failed. */
  private send(type: CanvasRequestType, payload: unknown): Promise<unknown> {
    if (!this.transport.isLive()) return Promise.resolve(undefined)
    return this.transport.request(type, payload).catch((error: unknown) => {
      void logFrontendError({
        level: 'warn',
        source: `canvas-gateway.${type}`,
        message: `${type} failed: ${String(error)}`
      })
      return undefined
    })
  }
}
