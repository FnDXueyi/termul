import { ProviderSettings } from '@/components/settings/provider/ProviderSettings'
import { SettingsSection } from '@/components/settings/SettingsLayout'

export function ProviderSection(): React.JSX.Element {
  return (
    <SettingsSection id="provider">
      <ProviderSettings />
    </SettingsSection>
  )
}
