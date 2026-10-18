/**
 * Warm-pool session ids that have not been promoted to a saved chat.
 * Lives in its own module so helpers can drop an id without importing
 * `shared-state` (that file already imports helpers).
 */
export const ephemeralSessionIds = new Set<string>()

/**
 * Agents whose kill is in flight. Their sessions go with the process, so a
 * warm session on one is dropped without a separate dispose.
 */
export const stoppingAgentIds = new Set<string>()
