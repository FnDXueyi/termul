import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { useUpdateAppSetting } from '@/hooks/use-app-settings'
import { resolveShellLayout, useShellLayoutReloadPending } from '@/lib/shell-layout-mode'
import { useAppSettingsStore } from '@/stores/app-settings-store'

/**
 * Opt-in switch for the revamped shell layout. The only writer of
 * `AppSettings.shellLayout`. The flag is read at load, so a change shows
 * "Reload to apply" until the app restarts with it.
 */
export function ShellLayoutToggle(): React.JSX.Element {
  const shellLayout = useAppSettingsStore((state) => state.settings.shellLayout)
  const updateSetting = useUpdateAppSetting<'shellLayout'>()
  const reloadPending = useShellLayoutReloadPending()

  return (
    <div className="flex min-h-11 items-center justify-between gap-4 rounded-md border border-border bg-secondary/30 px-4 py-3">
      <div className="flex-1">
        <Label htmlFor="shell-layout-revamp">Revamped shell layout</Label>
        <p id="shell-layout-revamp-help" className="mt-0.5 text-xs text-muted-foreground">
          Opt in to the redesigned workspace shell as it rolls out. Takes effect after a reload.
        </p>
        {reloadPending ? (
          <p className="mt-1 text-xs font-medium text-warning" role="status">
            Reload to apply
          </p>
        ) : null}
      </div>
      <Switch
        id="shell-layout-revamp"
        aria-describedby="shell-layout-revamp-help"
        checked={resolveShellLayout(shellLayout) === 'revamp'}
        onCheckedChange={(checked) => {
          void updateSetting('shellLayout', checked ? 'revamp' : 'classic')
        }}
      />
    </div>
  )
}
