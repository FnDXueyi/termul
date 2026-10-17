/**
 * Canvas mode (figcraft embed) — shared runtime-neutral contracts.
 *
 * `CanvasOpenInfo` mirrors the camelCase serde shape of the Rust
 * `canvas::static_server::CanvasOpenInfo`, returned by both the desktop
 * `canvas_open` command and the web `POST /canvas/open` route. The renderer
 * builds the iframe URL from it; the Rust side never hands out a full URL so
 * the host origin and mount sequence stay renderer-owned.
 */

import type { IpcResult } from './ipc.types'

export interface CanvasOpenInfo {
  /** Project-derived id the `/canvas/<id>/` bundle routes on. */
  canvasId: string
  /** Loopback origin on desktop (`http://127.0.0.1:<port>`); `''` on the web
   * client, where the bundle is same-origin. */
  baseUrl: string
  /** The `ct` session token for the bundle routes. Never logged. */
  canvasToken: string
  /** The figcraft tag this build pins; `host.describe` must report it. */
  pinTag: string
  /** Canonicalized absolute doc path. */
  docKey: string
}

/** Typed canvas facade and mount failure codes. */
export const CanvasErrorCodes = {
  PATH_VALIDATION_FAILED: 'PATH_VALIDATION_FAILED',
  TOKEN_GENERATION_FAILED: 'TOKEN_GENERATION_FAILED',
  BUNDLE_LISTENER_DOWN: 'BUNDLE_LISTENER_DOWN',
  BUNDLE_PIN_MISMATCH: 'BUNDLE_PIN_MISMATCH',
  EDITOR_CRASHED: 'EDITOR_CRASHED',
  UNSUPPORTED_SURFACE: 'UNSUPPORTED_SURFACE',
  NETWORK_ERROR: 'NETWORK_ERROR',
  INVOKE_ERROR: 'INVOKE_ERROR'
} as const

export type CanvasErrorCode = (typeof CanvasErrorCodes)[keyof typeof CanvasErrorCodes]

/**
 * Typed errors of the editor control protocol (spine AD-25, integration
 * contracts "Typed errors"). Where figcraft's `Backend` only carries a
 * string, the string is the JSON encoding of `{code, message}`.
 */
export const CANVAS_BRIDGE_ERROR_CODES = [
  'CANVAS_CLOSED',
  'CANVAS_CLOSING',
  'CANVAS_HIDDEN',
  'CANVAS_OPEN_ELSEWHERE',
  'CANVAS_TIMEOUT',
  'UNSUPPORTED_IN_KIOSK',
  'UNKNOWN_METHOD',
  'ENGINE_ERROR'
] as const

export type CanvasBridgeErrorCode = (typeof CANVAS_BRIDGE_ERROR_CODES)[number]

/**
 * Transport-neutral canvas facade: desktop resolves to the `canvas_open` /
 * `canvas_close` Tauri commands, web to `POST /canvas/open|close`. `open` is
 * gated off on phone-width viewports with a typed `UNSUPPORTED_SURFACE`.
 */
export interface CanvasApi {
  open: (docPath: string, projectId: string) => Promise<IpcResult<CanvasOpenInfo>>
  close: (projectId: string) => Promise<IpcResult<boolean>>
}
