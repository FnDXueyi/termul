import {
  APP_PREAPPROVABLE_ACTIONS,
  PROJECT_PREAPPROVABLE_ACTIONS
} from '@shared/types/browser-policy.types'
import { useState } from 'react'
import { useShallow } from 'zustand/shallow'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { useAppBrowserPolicy, useProjectBrowserPolicy } from '@/hooks/use-browser-policy'
import { useProjectStore } from '@/stores/project-store'
import { OriginListEditor, PreapprovalEditor } from './BrowserPolicyEditors'
import { describeFailure } from './browser-policy-copy'
import { SettingsSwitchRow } from './SettingsSwitchRow'

function Loading(): React.JSX.Element {
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground" role="status">
      <Spinner size={14} decorative />
      Loading browser rules…
    </div>
  )
}

function ReadOnlyNotice(): React.JSX.Element {
  return (
    <p className="rounded-md border border-border bg-secondary/30 px-3 py-2 text-xs text-muted-foreground">
      These rules were saved by a newer version of Termul. You can read them but not change them.
    </p>
  )
}

function Failure({ message }: { message: string }): React.JSX.Element {
  return (
    <p role="alert" className="text-xs text-destructive">
      {message}
    </p>
  )
}

function SwitchFailure({ message }: { message: string | null }): React.JSX.Element | null {
  return message ? <Failure message={message} /> : null
}

function ProjectPanel({ projectId }: { projectId: string }): React.JSX.Element {
  const { state, edit } = useProjectBrowserPolicy(projectId)
  const [switchError, setSwitchError] = useState<string | null>(null)

  if (state.status === 'loading') return <Loading />
  if (state.status === 'error') return <Failure message={describeFailure(state.failure)} />
  const { policy } = state
  const locked = policy.readOnly

  return (
    <div className="space-y-5">
      {locked && <ReadOnlyNotice />}
      <div className="space-y-1.5">
        <SettingsSwitchRow
          label="Browse logged out in this project"
          description="Agent tabs start logged out and everything they save is deleted when the chat ends. Off: agents use the shared agent profile."
          checked={policy.profileMode === 'ephemeral'}
          onToggle={(ephemeral) => {
            void edit({ op: 'set_ephemeral', ephemeral }).then((failure) =>
              setSwitchError(failure ? describeFailure(failure) : null)
            )
          }}
        />
        <SwitchFailure message={switchError} />
      </div>
      <OriginListEditor
        label="Allowed sites"
        description="Agents open these without asking. Use https://*.example.com for every subdomain."
        placeholder="https://example.com"
        entries={policy.allowList}
        disabled={locked}
        onAdd={(origin) => edit({ op: 'add_origin', origin })}
        onRemove={(origin) => edit({ op: 'remove_origin', origin })}
      />
      <PreapprovalEditor
        label="Pre-approved actions"
        description="Agents skip the question for these when they browse logged out in this project. The shared agent profile uses the all-projects list below."
        actions={PROJECT_PREAPPROVABLE_ACTIONS}
        entries={policy.preapprovals}
        disabled={locked}
        onAdd={({ action, origin }) => edit({ op: 'add_preapproval', action, origin })}
        onRemove={({ action, origin }) => edit({ op: 'remove_preapproval', action, origin })}
      />
    </div>
  )
}

function clearMessage(sessions: number, projects: number): string {
  if (sessions === 0) {
    return 'The agent signs out of every site. No agent browser tabs are open right now.'
  }
  const chats = `${sessions} ${sessions === 1 ? 'chat' : 'chats'}`
  const scope = `${projects} ${projects === 1 ? 'project' : 'projects'}`
  return `The agent signs out of every site and ${chats} in ${scope} lose their agent browser tabs.`
}

function AppPanel(): React.JSX.Element {
  const { state, edit } = useAppBrowserPolicy()
  const [switchError, setSwitchError] = useState<string | null>(null)
  const [confirming, setConfirming] = useState(false)
  const [clearing, setClearing] = useState(false)
  const [clearError, setClearError] = useState<string | null>(null)

  if (state.status === 'loading') return <Loading />
  if (state.status === 'error') return <Failure message={describeFailure(state.failure)} />
  const { policy } = state
  const locked = policy.readOnly

  const clear = async (): Promise<void> => {
    setClearing(true)
    const failure = await edit({ op: 'clear_global_profile' })
    setClearing(false)
    setConfirming(false)
    setClearError(failure ? describeFailure(failure) : null)
  }

  return (
    <div className="space-y-5">
      {locked && <ReadOnlyNotice />}
      <div className="space-y-1.5">
        <SettingsSwitchRow
          label="Shared agent profile"
          description="One agent profile for every project keeps logins between chats. Off: agents always browse logged out."
          checked={policy.globalProfileEnabled}
          onToggle={(enabled) => {
            void edit({ op: 'set_global_profile', enabled }).then((failure) =>
              setSwitchError(failure ? describeFailure(failure) : null)
            )
          }}
        />
        <SwitchFailure message={switchError} />
      </div>
      <PreapprovalEditor
        label="Pre-approved actions in every project"
        description="Agents skip the question for these on the shared agent profile. Password and payment forms always ask."
        actions={APP_PREAPPROVABLE_ACTIONS}
        entries={policy.preapprovals}
        disabled={locked}
        onAdd={({ action, origin }) => edit({ op: 'add_preapproval', action, origin })}
        onRemove={({ action, origin }) => edit({ op: 'remove_preapproval', action, origin })}
      />
      <div className="space-y-2">
        <div>
          <div className="text-sm text-foreground">Clear agent profile</div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            Closes agent browser tabs that use the shared profile and deletes its logins and site
            data.
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="destructive"
          disabled={locked}
          onClick={() => setConfirming(true)}
        >
          Clear agent profile
        </Button>
        <SwitchFailure message={clearError} />
      </div>
      <ConfirmDialog
        isOpen={confirming}
        title="Clear agent profile"
        message={clearMessage(policy.clearImpact.sessions, policy.clearImpact.projects)}
        confirmLabel="Clear profile"
        variant="danger"
        isLoading={clearing}
        onConfirm={() => void clear()}
        onCancel={() => setConfirming(false)}
      />
    </div>
  )
}

/** Settings for what agents may do in the browser: per project, and for the whole app. */
export function BrowserPolicySettings(): React.JSX.Element {
  const projects = useProjectStore(
    useShallow((state) => state.projects.filter((p) => !p.isArchived))
  )
  const activeProjectId = useProjectStore((state) => state.activeProjectId)
  const [picked, setPicked] = useState('')
  const projectId = projects.some((p) => p.id === picked)
    ? picked
    : projects.some((p) => p.id === activeProjectId)
      ? activeProjectId
      : (projects[0]?.id ?? '')

  return (
    <div className="space-y-8">
      <section aria-labelledby="browser-policy-project" className="space-y-4">
        <div className="space-y-2">
          <h3 id="browser-policy-project" className="text-sm font-medium text-foreground">
            This project
          </h3>
          {projects.length > 1 && (
            <div className="space-y-1.5">
              <Label htmlFor="browser-policy-project-select" className="text-xs">
                Project
              </Label>
              <Select value={projectId} onValueChange={setPicked}>
                <SelectTrigger id="browser-policy-project-select" className="h-9">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {projects.map((project) => (
                    <SelectItem key={project.id} value={project.id}>
                      {project.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
        </div>
        {projectId ? (
          <ProjectPanel key={projectId} projectId={projectId} />
        ) : (
          <p className="text-xs text-muted-foreground">Add a project to set its browser rules.</p>
        )}
      </section>
      <section aria-labelledby="browser-policy-app" className="space-y-4">
        <h3 id="browser-policy-app" className="text-sm font-medium text-foreground">
          All projects
        </h3>
        <AppPanel />
      </section>
    </div>
  )
}
