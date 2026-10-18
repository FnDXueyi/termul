import { toast } from 'sonner'

/**
 * Toast for a delete that already happened. It has no timer: it stays until
 * the user selects Undo, closes it, or the next undo toast replaces it.
 */
export function showUndoToast(message: string, onUndo: () => void): void {
  toast(message, {
    id: 'settings-undo',
    duration: Number.POSITIVE_INFINITY,
    action: { label: 'Undo', onClick: onUndo }
  })
}
