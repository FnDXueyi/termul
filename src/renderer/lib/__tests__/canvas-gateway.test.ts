/**
 * The canvas store's mounts drive the gateway's mount reports, and forwarded
 * calls run through the mounted bridge.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/lib/log-api', () => ({ logFrontendError: vi.fn() }))
vi.mock('@/lib/canvas-api', () => ({ canvasApi: { open: vi.fn(), close: vi.fn() } }))

import type { CanvasOpenInfo } from '@shared/types/canvas.types'
import type { AcpTransport } from '@/lib/acp-transport'
import { CanvasGatewayClient } from '@/lib/acp-transport/canvas-gateway-client'
import { asJsonText, type CanvasBridge } from '@/lib/canvas-bridge'
import { type CanvasPhase, useCanvasStore } from '@/stores/canvas-store'
import { type CanvasGatewayOptions, startCanvasGateway } from '../canvas-gateway'

const INFO: CanvasOpenInfo = {
  canvasId: 'cv1',
  baseUrl: 'http://127.0.0.1:5199',
  canvasToken: 'token',
  pinTag: 'dev',
  docKey: 'C:/p/design.figcraft'
}

function setPhase(projectId: string, phase: CanvasPhase | null): void {
  useCanvasStore.setState((state) => {
    const { [projectId]: _dropped, ...rest } = state.sessions
    return {
      sessions:
        phase === null
          ? rest
          : {
              ...rest,
              [projectId]: {
                projectId,
                docPath: INFO.docKey,
                phase,
                dirty: false,
                contentVersion: null
              }
            }
    }
  })
}

function harness(options?: CanvasGatewayOptions) {
  const sent: Array<[string, unknown]> = []
  const canvas = new CanvasGatewayClient({
    request: (type, payload) => {
      sent.push([type, payload])
      return Promise.resolve({})
    },
    isLive: () => true
  })
  const stop = startCanvasGateway({ canvas } as unknown as AcpTransport, options)
  return { canvas, sent, stop }
}

let stopGateway: (() => void) | null = null

afterEach(() => {
  stopGateway?.()
  stopGateway = null
  useCanvasStore.setState({ sessions: {} })
})

describe('startCanvasGateway', () => {
  it('reports loading and ready mounts, remounts, and closes', () => {
    const { sent, stop } = harness()
    stopGateway = stop
    setPhase('proj-1', { kind: 'opening' })
    setPhase('proj-1', { kind: 'loading', mount: { mountSeq: 1, info: INFO } })
    setPhase('proj-1', { kind: 'ready', mount: { mountSeq: 1, info: INFO } })
    setPhase('proj-1', { kind: 'loading', mount: { mountSeq: 2, info: INFO } })
    setPhase('proj-1', null)
    expect(sent).toEqual([
      ['canvas_mount', { projectId: 'proj-1', mountSeq: 1 }],
      ['canvas_mount', { projectId: 'proj-1', mountSeq: 2 }],
      ['canvas_unmount', { projectId: 'proj-1', mountSeq: 2 }]
    ])
  })

  it('runs a forwarded call through the mounted bridge and replies with its result text', async () => {
    const { canvas, sent, stop } = harness()
    stopGateway = stop
    setPhase('proj-1', { kind: 'ready', mount: { mountSeq: 5, info: INFO } })
    const bridge: CanvasBridge = {
      call: vi.fn(async () => ({
        ok: true as const,
        result: asJsonText('{"id":18446744073709551615}')
      })),
      dispose: vi.fn()
    }
    useCanvasStore.getState().attachBridge('proj-1', 5, bridge)

    canvas.handleFrame({
      type: 'canvas_call',
      callId: 8,
      projectId: 'proj-1',
      mountSeq: 5,
      method: 'node.get',
      params: '{"id":18446744073709551615}'
    })
    await vi.waitFor(() => expect(sent.at(-1)?.[0]).toBe('canvas_reply'))
    expect(bridge.call).toHaveBeenCalledWith('node.get', '{"id":18446744073709551615}')
    expect(sent.at(-1)).toEqual([
      'canvas_reply',
      { callId: 8, mountSeq: 5, ok: true, result: '{"id":18446744073709551615}' }
    ])
    useCanvasStore.getState().detachBridge('proj-1', bridge)
  })

  it('answers CANVAS_CLOSED for a project this renderer has not mounted', async () => {
    const { canvas, sent, stop } = harness()
    stopGateway = stop
    canvas.handleFrame({
      type: 'canvas_call',
      callId: 2,
      projectId: 'proj-9',
      mountSeq: 1,
      method: 'document.inspect',
      params: '{}'
    })
    await vi.waitFor(() => expect(sent).toHaveLength(1))
    expect(sent[0]).toEqual([
      'canvas_reply',
      expect.objectContaining({
        callId: 2,
        ok: false,
        error: expect.objectContaining({ code: 'CANVAS_CLOSED' })
      })
    ])
  })

  it('answers host.save with the host save, never the editor', async () => {
    const onHostSave = vi.fn(async () => ({ ok: true as const, path: 'design/app.figcraft' }))
    const { canvas, sent, stop } = harness({ onHostSave })
    stopGateway = stop
    setPhase('proj-1', { kind: 'ready', mount: { mountSeq: 4, info: INFO } })
    const bridge: CanvasBridge = { call: vi.fn(), dispose: vi.fn() }
    useCanvasStore.getState().attachBridge('proj-1', 4, bridge)

    canvas.handleFrame({
      type: 'canvas_call',
      callId: 3,
      projectId: 'proj-1',
      mountSeq: 4,
      method: 'host.save',
      params: '{}'
    })
    await vi.waitFor(() => expect(sent.at(-1)?.[0]).toBe('canvas_reply'))
    expect(onHostSave).toHaveBeenCalledWith('proj-1')
    expect(bridge.call).not.toHaveBeenCalled()
    expect(sent.at(-1)).toEqual([
      'canvas_reply',
      { callId: 3, mountSeq: 4, ok: true, result: '{"path":"design/app.figcraft"}' }
    ])
    useCanvasStore.getState().detachBridge('proj-1', bridge)
  })

  it('answers host.save with CANVAS_CLOSED until a host save is wired', async () => {
    const { canvas, sent, stop } = harness()
    stopGateway = stop
    setPhase('proj-1', { kind: 'ready', mount: { mountSeq: 4, info: INFO } })

    canvas.handleFrame({
      type: 'canvas_call',
      callId: 5,
      projectId: 'proj-1',
      mountSeq: 4,
      method: 'host.save',
      params: '{}'
    })
    await vi.waitFor(() => expect(sent.at(-1)?.[0]).toBe('canvas_reply'))
    expect(sent.at(-1)).toEqual([
      'canvas_reply',
      expect.objectContaining({
        callId: 5,
        ok: false,
        error: expect.objectContaining({ code: 'CANVAS_CLOSED' })
      })
    ])
  })
})
