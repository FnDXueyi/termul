/**
 * Fetch-based HTTP impl of the canvas facade over the routes in
 * `src-tauri/src/web/canvas_api.rs` (same-origin, behind the bearer
 * `web_auth_gate`; `postJson` adds the web auth header):
 * - `POST /canvas/open` body `{ docPath, projectId }` → `CanvasOpenInfo`
 *   with an empty `baseUrl` (the bundle is same-origin);
 * - `POST /canvas/close` body `{ projectId }` — revokes the `ct` token.
 *
 * The bundle routes set their own HttpOnly per-canvas cookie on the first
 * `ct` request, so this adapter never touches `document.cookie`. Transport
 * failures map to `NETWORK_ERROR`; server failures keep their typed code.
 */

import type { CanvasApi, CanvasOpenInfo } from '@shared/types/canvas.types'
import type { IpcResult } from '@shared/types/ipc.types'

import { postJson } from './ipc/http'

export const webCanvasApi: CanvasApi = {
  open(docPath: string, projectId: string): Promise<IpcResult<CanvasOpenInfo>> {
    return postJson<CanvasOpenInfo>('/canvas/open', { docPath, projectId })
  },

  close(projectId: string): Promise<IpcResult<boolean>> {
    return postJson<boolean>('/canvas/close', { projectId })
  }
}
