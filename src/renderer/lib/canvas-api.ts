/**
 * Canvas API singleton: the Tauri IPC impl inside a Tauri webview, the
 * fetch-backed HTTP impl on the web client. Both return the same
 * `IpcResult<...>` shapes (the parity checklist pins this), so nothing above
 * this file branches on surface.
 *
 * Phone-width gating: `open` checks the live viewport predicate at call time
 * and answers a typed `UNSUPPORTED_SURFACE` failure (no canvas on the mobile
 * web shell).
 */

import type { CanvasApi, CanvasOpenInfo } from '@shared/types/canvas.types'
import type { IpcResult } from '@shared/types/ipc.types'

import { isMobileWebShellViewport } from '@/hooks/use-mobile-web-shell'
import { createTauriCanvasApi } from './tauri-canvas-api'
import { isTauriContext } from './tauri-runtime'
import { webCanvasApi } from './web-canvas-api'

const tauriCanvasApi = createTauriCanvasApi()

export const canvasApi: CanvasApi = {
  open(docPath: string, projectId: string): Promise<IpcResult<CanvasOpenInfo>> {
    if (isMobileWebShellViewport()) {
      return Promise.resolve({
        success: false,
        error: 'canvas open is unavailable on the mobile web shell',
        code: 'UNSUPPORTED_SURFACE'
      })
    }
    return isTauriContext()
      ? tauriCanvasApi.open(docPath, projectId)
      : webCanvasApi.open(docPath, projectId)
  },

  close(projectId: string): Promise<IpcResult<boolean>> {
    return isTauriContext() ? tauriCanvasApi.close(projectId) : webCanvasApi.close(projectId)
  }
}

export { createTauriCanvasApi } from './tauri-canvas-api'
export { webCanvasApi } from './web-canvas-api'
