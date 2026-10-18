import { Globe } from '@/components/icons'
import { BrowserPolicySettings } from '@/components/settings/BrowserPolicySettings'
import { SettingsSection } from '@/components/settings/SettingsLayout'

export function BrowserSection(): React.JSX.Element {
  return (
    <SettingsSection id="browser">
      <div className="flex flex-col gap-6 border-b border-border pb-6 md:flex-row md:items-start">
        <div className="w-full pt-1 md:w-1/3">
          <div className="flex items-center gap-2">
            <Globe size={18} className="text-primary" />
            <h2 className="text-lg font-medium text-foreground">Browser</h2>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            Choose which sites agents may browse without asking, and how they sign in.
          </p>
        </div>
        <div className="w-full md:w-2/3">
          <BrowserPolicySettings />
        </div>
      </div>
    </SettingsSection>
  )
}
