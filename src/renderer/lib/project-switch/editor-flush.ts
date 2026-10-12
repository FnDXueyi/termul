/**
 * Seam between the project-switch coordinator and the editor's pending
 * autosaves: `leave()` awaits this before persisting the leaving project, so
 * no debounced save is still in flight when its files are cleared.
 *
 * Intentionally empty until the editor-drafts work lands `flushPendingAutoSaves`
 * in `@/lib/editor-auto-save`; its owner replaces the body with a call to it
 * (one line), and nothing else about the coordinator changes.
 */
export async function flushEditorWork(): Promise<void> {}
