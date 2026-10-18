import { SettingsCard, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SettingsRow, settingsDescriptionId } from '@/components/settings/SettingsRow'
import { Switch } from '@/components/ui/switch'
import { rowId } from './categories'

interface NotificationsSectionProps {
  notifyOnTerminalIdle: boolean
  notifyOnAgentChatTurnFinished: boolean
  notifyOnAgentChatNeedsYou: boolean
  handleNotifyOnTerminalIdleToggle: (enabled: boolean) => void
  handleNotifyOnAgentChatTurnFinishedToggle: (enabled: boolean) => void
  handleNotifyOnAgentChatNeedsYouToggle: (enabled: boolean) => void
}

function SwitchRow({
  id,
  label,
  description,
  checked,
  onToggle
}: {
  id: string
  label: string
  description: string
  checked: boolean
  onToggle: (enabled: boolean) => void
}): React.JSX.Element {
  return (
    <SettingsRow
      id={rowId(id)}
      htmlFor={id}
      label={label}
      description={description}
      layout="inline"
    >
      <Switch
        id={id}
        aria-describedby={settingsDescriptionId(id)}
        checked={checked}
        onCheckedChange={onToggle}
      />
    </SettingsRow>
  )
}

export function NotificationsSection({
  notifyOnTerminalIdle,
  notifyOnAgentChatTurnFinished,
  notifyOnAgentChatNeedsYou,
  handleNotifyOnTerminalIdleToggle,
  handleNotifyOnAgentChatTurnFinishedToggle,
  handleNotifyOnAgentChatNeedsYouToggle
}: NotificationsSectionProps): React.JSX.Element {
  return (
    <SettingsSection id="notifications">
      <SettingsGroup title="Terminal">
        <SettingsCard>
          <SwitchRow
            id="notify-terminal-idle"
            label="Notify when a terminal agent finishes"
            description="A long-running terminal tab goes quiet."
            checked={notifyOnTerminalIdle}
            onToggle={handleNotifyOnTerminalIdleToggle}
          />
        </SettingsCard>
      </SettingsGroup>
      <SettingsGroup title="Agent chat">
        <SettingsCard>
          <SwitchRow
            id="notify-turn-finished"
            label="Notify when an agent chat turn finishes"
            description="A chat turn ends and nothing is queued."
            checked={notifyOnAgentChatTurnFinished}
            onToggle={handleNotifyOnAgentChatTurnFinishedToggle}
          />
          <SwitchRow
            id="notify-needs-you"
            label="Notify when an agent chat needs you"
            description="An agent waits for your approval or an answer."
            checked={notifyOnAgentChatNeedsYou}
            onToggle={handleNotifyOnAgentChatNeedsYouToggle}
          />
        </SettingsCard>
      </SettingsGroup>
    </SettingsSection>
  )
}
