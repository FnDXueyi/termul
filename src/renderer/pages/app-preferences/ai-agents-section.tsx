import { useEffect, useState } from 'react'
import { Bot } from '@/components/icons'
import { AcpAgentsSettings } from '@/components/settings/AcpAgentsSettings'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { PANEL_FIELD_CLASS } from '@/components/ui/panel-styles'
import type { AgentIdleWindowStatus } from '@/lib/acp-api'
import { acpApi } from '@/lib/api'
import { logFrontendError } from '@/lib/log-api'
import { isTauriContext } from '@/lib/tauri-runtime'
import { cn } from '@/lib/utils'
import {
  ACP_SESSION_NEW_TIMEOUT_OPTIONS,
  ACP_SESSION_REOPEN_TIMEOUT_OPTIONS,
  ACP_TURN_IDLE_TIMEOUT_OPTIONS,
  ACP_TURN_TIMEOUT_OPTIONS,
  AGENT_IDLE_WINDOW_OPTIONS,
  formatAgentIdleWindow
} from '@/types/settings'

interface AiAgentsSectionProps {
  acpTurnTimeoutSecs: number | null
  acpTurnIdleTimeoutSecs: number | null
  acpSessionNewTimeoutSecs: number | null
  acpSessionReopenTimeoutSecs: number | null
  agentIdleWindowSecs: number | null
  handleAcpTurnTimeoutChange: (value: number | null) => void
  handleAcpTurnIdleTimeoutChange: (value: number | null) => void
  handleAcpSessionNewTimeoutChange: (value: number | null) => void
  handleAcpSessionReopenTimeoutChange: (value: number | null) => void
  handleAgentIdleWindowChange: (value: number | null) => void
}

interface TimeoutChoice {
  value: number | null
  label: string
}

function TimeoutSelect({
  id,
  label,
  value,
  options,
  help,
  onChange
}: {
  id: string
  label: string
  value: number | null
  options: TimeoutChoice[]
  help: string
  onChange: (value: number | null) => void
}): React.JSX.Element {
  return (
    <div>
      <label htmlFor={id} className="block text-sm font-medium text-secondary-foreground mb-2">
        {label}
      </label>
      <select
        id={id}
        value={value === null ? 'null' : String(value)}
        onChange={(e) => onChange(e.target.value === 'null' ? null : parseInt(e.target.value, 10))}
        className={cn(PANEL_FIELD_CLASS, 'w-full px-3 py-2 text-sm')}
      >
        {options.map((option) => (
          <option
            key={option.value === null ? 'null' : String(option.value)}
            value={option.value === null ? 'null' : String(option.value)}
          >
            {option.label}
          </option>
        ))}
      </select>
      <p className="text-xs text-muted-foreground mt-1">{help}</p>
    </div>
  )
}

export function AiAgentsSection({
  acpTurnTimeoutSecs,
  acpTurnIdleTimeoutSecs,
  acpSessionNewTimeoutSecs,
  acpSessionReopenTimeoutSecs,
  agentIdleWindowSecs,
  handleAcpTurnTimeoutChange,
  handleAcpTurnIdleTimeoutChange,
  handleAcpSessionNewTimeoutChange,
  handleAcpSessionReopenTimeoutChange,
  handleAgentIdleWindowChange
}: AiAgentsSectionProps): React.JSX.Element {
  const isDesktop = isTauriContext()
  const [liveWindow, setLiveWindow] = useState<AgentIdleWindowStatus | null>(null)
  const [liveWindowFailed, setLiveWindowFailed] = useState(false)

  useEffect(() => {
    // `agentIdleWindowSecs` is the refetch key. The host answer can differ
    // from the saved value when TERMUL_AGENT_IDLE_REAP_SECS is set.
    let cancelled = false
    void agentIdleWindowSecs
    void acpApi
      .getAgentIdleWindow()
      .then((status) => {
        if (cancelled) return
        setLiveWindow(status)
        setLiveWindowFailed(false)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        setLiveWindow(null)
        setLiveWindowFailed(true)
        void logFrontendError({
          level: 'warn',
          source: 'AiAgentsSection.idleWindow',
          message: `Failed to read the agent idle window: ${error instanceof Error ? error.message : String(error)}`
        })
      })
    return () => {
      cancelled = true
    }
  }, [agentIdleWindowSecs])

  const envLocksWindow = liveWindow?.source === 'env'
  const operatorLocksWindow = liveWindow?.source === 'operator'

  return (
    <SettingsSection id="ai-agents">
      <div className="flex flex-col items-start gap-6 border-b border-border pb-6 md:flex-row">
        <div className="w-full pt-1 md:w-1/3">
          <div className="flex items-center gap-2">
            <Bot size={18} className="text-primary" />
            <h2 className="text-lg font-medium text-foreground">AI Agents</h2>
          </div>
          <p className="text-sm text-muted-foreground mt-1">
            View ACP agent availability and warm/auth status. Agent Chat supports these agents
            automatically.
          </p>
        </div>
        <div className="w-full space-y-4 md:w-2/3">
          <h3 className="text-sm font-medium text-foreground">When you leave a chat</h3>
          <TimeoutSelect
            id="agent-idle-window"
            label="Keep idle chats"
            value={agentIdleWindowSecs}
            options={AGENT_IDLE_WINDOW_OPTIONS}
            onChange={handleAgentIdleWindowChange}
            help="Choose how long an idle agent chat stays open. An open tab does not keep the chat alive. A running answer, a pending approval, or a running agent terminal does. The change applies within one minute. A server flag or TERMUL_AGENT_IDLE_REAP_SECS overrides this menu."
          />
          {envLocksWindow && liveWindow ? (
            <p className="text-xs text-muted-foreground">
              TERMUL_AGENT_IDLE_REAP_SECS is set to {formatAgentIdleWindow(liveWindow.secs)}. This
              menu is saved. It applies after you remove that variable and restart Termul.
            </p>
          ) : null}
          {operatorLocksWindow && liveWindow ? (
            <p className="text-xs text-muted-foreground">
              The server was started with --agent-idle-window-secs (
              {formatAgentIdleWindow(liveWindow.secs)}). This menu is saved. It applies after that
              flag is removed and the server restarts.
            </p>
          ) : null}
          {isDesktop ? (
            <>
              <h3 className="text-sm font-medium text-foreground">While an agent answers</h3>
              <TimeoutSelect
                id="acp-turn-idle-timeout"
                label="Stop a silent answer after"
                value={acpTurnIdleTimeoutSecs}
                options={ACP_TURN_IDLE_TIMEOUT_OPTIONS}
                onChange={handleAcpTurnIdleTimeoutChange}
                help="Termul cancels an answer that sends no update for this long. The default has no limit. TERMUL_ACP_TURN_IDLE_TIMEOUT_SECS overrides this menu."
              />
              <TimeoutSelect
                id="acp-turn-timeout"
                label="Stop a long answer after"
                value={acpTurnTimeoutSecs}
                options={ACP_TURN_TIMEOUT_OPTIONS}
                onChange={handleAcpTurnTimeoutChange}
                help="Termul stops an answer that is still sending output when it reaches this time. The default has no limit. TERMUL_ACP_TURN_TIMEOUT_SECS overrides this menu."
              />
              <h3 className="text-sm font-medium text-foreground">When a chat starts</h3>
              <TimeoutSelect
                id="acp-session-new-timeout"
                label="Wait for a new agent"
                value={acpSessionNewTimeoutSecs}
                options={ACP_SESSION_NEW_TIMEOUT_OPTIONS}
                onChange={handleAcpSessionNewTimeoutChange}
                help="How long Termul waits for a new agent to become ready. The default is 60 seconds. TERMUL_ACP_SESSION_NEW_TIMEOUT_SECS overrides this menu."
              />
              <TimeoutSelect
                id="acp-session-reopen-timeout"
                label="Wait to reopen a chat"
                value={acpSessionReopenTimeoutSecs}
                options={ACP_SESSION_REOPEN_TIMEOUT_OPTIONS}
                onChange={handleAcpSessionReopenTimeoutChange}
                help="How long Termul waits to reopen a saved chat. A long history can need more time. The default is 60 seconds. TERMUL_ACP_SESSION_REOPEN_TIMEOUT_SECS overrides this menu."
              />
            </>
          ) : (
            <div className="bg-secondary/30 border border-border rounded-md px-4 py-3">
              {liveWindowFailed ? (
                <p className="mb-3 text-xs text-muted-foreground">
                  Termul could not read the active idle window. Your menu choice is still saved.
                </p>
              ) : null}
              <div className="text-sm font-medium text-foreground">While an agent answers</div>
              <ul className="mt-1 space-y-1 text-xs text-muted-foreground">
                <li>
                  Silent answer:{' '}
                  <span className="font-mono text-foreground">
                    {acpTurnIdleTimeoutSecs === null
                      ? 'server default (no limit)'
                      : `${acpTurnIdleTimeoutSecs}s`}
                  </span>
                </li>
                <li>
                  Long answer:{' '}
                  <span className="font-mono text-foreground">
                    {acpTurnTimeoutSecs === null
                      ? 'server default (no limit)'
                      : `${acpTurnTimeoutSecs}s`}
                  </span>
                </li>
              </ul>
              <div className="mt-3 text-sm font-medium text-foreground">When a chat starts</div>
              <ul className="mt-1 space-y-1 text-xs text-muted-foreground">
                <li>
                  New agent:{' '}
                  <span className="font-mono text-foreground">
                    {acpSessionNewTimeoutSecs === null
                      ? 'server default (60 seconds)'
                      : `${acpSessionNewTimeoutSecs}s`}
                  </span>
                </li>
                <li>
                  Reopen a chat:{' '}
                  <span className="font-mono text-foreground">
                    {acpSessionReopenTimeoutSecs === null
                      ? 'server default (60 seconds)'
                      : `${acpSessionReopenTimeoutSecs}s`}
                  </span>
                </li>
              </ul>
              <p className="mt-2 text-xs text-muted-foreground">
                Managed by the server — timeouts for agents spawned by the standalone server come
                from its env vars (TERMUL_ACP_*_TIMEOUT_SECS), not from the browser client.
              </p>
            </div>
          )}
          <AcpAgentsSettings />
        </div>
      </div>
    </SettingsSection>
  )
}
