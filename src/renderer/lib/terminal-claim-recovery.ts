import {
  COMMAND_RESULT_CODES,
  type IpcResult,
  type PreservedTerminalEntry,
  type RotatedClaim
} from '@shared/types/ipc.types'

/**
 * A rotate whose first run applied but whose reply (the new claim) is gone
 * answers OUTCOME_UNKNOWN, and the host never kept the new credential.
 * `list_preserved` re-issues a claim for each of the project's terminals, so
 * the terminal's entry carries a working one. Without the project the terminal
 * was spawned or listed for, the failure stands.
 */
export async function recoverLostRotation(
  rotated: IpcResult<RotatedClaim>,
  terminalId: string,
  projectId: string | undefined,
  listPreserved: (projectId: string) => Promise<IpcResult<PreservedTerminalEntry[]>>
): Promise<IpcResult<RotatedClaim>> {
  if (rotated.success || rotated.code !== COMMAND_RESULT_CODES.OUTCOME_UNKNOWN || !projectId) {
    return rotated
  }
  const listed = await listPreserved(projectId)
  const entry = listed.success ? listed.data.find((item) => item.id === terminalId) : undefined
  return entry?.claim ? { success: true, data: { claim: entry.claim } } : rotated
}
