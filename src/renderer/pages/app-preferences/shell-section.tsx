import type { DetectedShells } from '@shared/types/ipc.types'
import { SettingsCard, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SettingsRow, settingsDescriptionId } from '@/components/settings/SettingsRow'
import { SettingsSelect } from '@/components/settings/SettingsSelect'
import { rowId } from './categories'

interface ShellSectionProps {
  defaultShell: string
  availableShells: DetectedShells | null
  handleDefaultShellChange: (value: string) => void
}

/** Radix Select reserves the empty string; the system default needs its own value. */
const SYSTEM_DEFAULT_SHELL = '__system_default__'

/** Stored value may be a name or a path; show the matching detected shell's path. */
function displayValue(defaultShell: string, availableShells: DetectedShells | null): string {
  if (!defaultShell) return SYSTEM_DEFAULT_SHELL
  if (defaultShell.includes('\\') || defaultShell.includes('/')) return defaultShell
  const match = availableShells?.available.find((shell) => {
    if (shell.name === defaultShell) return true
    return shell.path.split(/[\\/]/).pop() === defaultShell
  })
  return match?.path ?? defaultShell
}

export function ShellSection({
  defaultShell,
  availableShells,
  handleDefaultShellChange
}: ShellSectionProps): React.JSX.Element {
  const value = displayValue(defaultShell, availableShells)
  const options = [
    { value: SYSTEM_DEFAULT_SHELL, label: 'System default' },
    ...(availableShells?.available ?? []).map((shell) => ({
      value: shell.path,
      label: shell.displayName
    }))
  ]
  // Keep a stored shell that detection did not find, so the menu shows it.
  if (!options.some((option) => option.value === value)) {
    options.push({ value, label: value })
  }
  return (
    <SettingsSection id="shell">
      <SettingsGroup title="Default shell">
        <SettingsCard>
          <SettingsRow
            id={rowId('default-shell')}
            htmlFor="default-shell"
            label="Shell"
            description="A project can choose its own shell in Project settings."
          >
            <SettingsSelect
              id="default-shell"
              aria-describedby={settingsDescriptionId('default-shell')}
              options={options}
              value={value}
              onValueChange={(next) =>
                handleDefaultShellChange(next === SYSTEM_DEFAULT_SHELL ? '' : next)
              }
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsGroup>
    </SettingsSection>
  )
}
