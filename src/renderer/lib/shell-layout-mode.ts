/**
 * Opt-in shell layout flag (shell revamp, CAP-18).
 *
 * `AppSettings.shellLayout` is optional and absent from `DEFAULT_APP_SETTINGS`,
 * so an unset flag resolves to `SHELL_LAYOUT_DEFAULT`. The mode that was
 * active when the app loaded is captured once (first wins); the Preferences
 * toggle compares the saved setting with it to show "Reload to apply".
 *
 * Nothing reads the resolved mode to change a layout yet: the frame that
 * consumes it lands in a later story, and the phone shell ignores it.
 */

import { useAppSettingsStore } from '@/stores/app-settings-store'
import type { ShellLayoutMode } from '@/types/settings'

/** Code-constant default; never stored in `DEFAULT_APP_SETTINGS`. */
export const SHELL_LAYOUT_DEFAULT: ShellLayoutMode = 'classic'

/** Maps a stored (possibly absent or unknown) value to a valid mode. */
export function resolveShellLayout(setting: unknown): ShellLayoutMode {
  return setting === 'revamp' || setting === 'classic' ? setting : SHELL_LAYOUT_DEFAULT
}

let activeAtLoad: ShellLayoutMode | null = null

/** Records the mode active at load. Only the first call takes effect. */
export function captureShellLayoutAtLoad(setting: unknown): ShellLayoutMode {
  if (activeAtLoad === null) activeAtLoad = resolveShellLayout(setting)
  return activeAtLoad
}

/** The mode captured at load, or `null` before settings have loaded. */
export function getShellLayoutActiveAtLoad(): ShellLayoutMode | null {
  return activeAtLoad
}

/** @internal Test-only: forget the captured mode. */
export function resetShellLayoutActiveAtLoadForTests(): void {
  activeAtLoad = null
}

/** True while the saved setting differs from the mode active at load. */
export function useShellLayoutReloadPending(): boolean {
  const saved = useAppSettingsStore((state) => state.settings.shellLayout)
  const isLoaded = useAppSettingsStore((state) => state.isLoaded)
  if (!isLoaded || activeAtLoad === null) return false
  return resolveShellLayout(saved) !== activeAtLoad
}
