/**
 * Tauri IPC implementation of the canvas facade. Mirrors the
 * `#[tauri::command] canvas_open` / `canvas_close` handlers in
 * `src-tauri/src/commands/canvas.rs`, which already return `IpcResult<T>`.
 * The web twin is `web-canvas-api.ts`; the parity checklist pins both.
 */

import type { CanvasApi, CanvasOpenInfo } from '@shared/types/canvas.types'
import type { IpcResult } from '@shared/types/ipc.types'

import { invokeIpc } from './ipc/tauri'
import { isTauriContext } from './tauri-runtime'

const IPC_COMMANDS = {
  OPEN: 'canvas_open',
  CLOSE: 'canvas_close'
} as const

function notTauri(method: string): IpcResult<never> {
  return {
    success: false,
    error: `${method} requires the Tauri runtime`,
    code: 'INVOKE_ERROR'
  }
}

/** Build the Tauri IPC impl of [`CanvasApi`]; `canvas-api.ts` picks it when
 * `isTauriContext()` is true. */
export function createTauriCanvasApi(): CanvasApi {
  return {
    async open(docPath: string, projectId: string): Promise<IpcResult<CanvasOpenInfo>> {
      if (!isTauriContext()) return notTauri(IPC_COMMANDS.OPEN)
      return invokeIpc<CanvasOpenInfo>(IPC_COMMANDS.OPEN, { docPath, projectId })
    },

    async close(projectId: string): Promise<IpcResult<boolean>> {
      if (!isTauriContext()) return notTauri(IPC_COMMANDS.CLOSE)
      return invokeIpc<boolean>(IPC_COMMANDS.CLOSE, { projectId })
    }
  }
}
