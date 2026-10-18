import type { BrowserActionClass, BrowserPreapproval } from '@shared/types/browser-policy.types'
import { useId, useState } from 'react'
import { Plus, X } from '@/components/icons'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from '@/components/ui/select'
import type { BrowserPolicyFailure } from '@/lib/browser-policy-api'
import { ACTION_LABELS, describeFailure } from './browser-policy-copy'

const ROW_CLASS =
  'flex min-w-0 items-center gap-2 rounded-md border border-border bg-secondary/30 px-3 py-1.5'

function ErrorLine({ message }: { message: string | null }): React.JSX.Element | null {
  if (!message) return null
  return (
    <p role="alert" className="text-xs text-destructive">
      {message}
    </p>
  )
}

interface OriginListEditorProps {
  label: string
  description: string
  placeholder: string
  entries: string[]
  disabled: boolean
  onAdd: (origin: string) => Promise<BrowserPolicyFailure | null>
  onRemove: (origin: string) => Promise<BrowserPolicyFailure | null>
}

/** A list of origins with a field to add one. */
export function OriginListEditor({
  label,
  description,
  placeholder,
  entries,
  disabled,
  onAdd,
  onRemove
}: OriginListEditorProps): React.JSX.Element {
  const inputId = useId()
  const [draft, setDraft] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = async (): Promise<void> => {
    const origin = draft.trim()
    if (!origin) return
    setBusy(true)
    const failure = await onAdd(origin)
    setBusy(false)
    setError(failure ? describeFailure(failure) : null)
    if (!failure) setDraft('')
  }

  const remove = async (origin: string): Promise<void> => {
    setBusy(true)
    const failure = await onRemove(origin)
    setBusy(false)
    setError(failure ? describeFailure(failure) : null)
  }

  return (
    <div className="space-y-2">
      <div>
        <Label htmlFor={inputId} className="text-sm text-foreground">
          {label}
        </Label>
        <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
      </div>
      {entries.length === 0 ? (
        <p className="text-xs text-muted-foreground">None yet.</p>
      ) : (
        <ul className="space-y-1.5" aria-label={`${label} list`}>
          {entries.map((origin) => (
            <li key={origin} className={ROW_CLASS}>
              <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">
                {origin}
              </span>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                disabled={disabled || busy}
                onClick={() => void remove(origin)}
                aria-label={`Remove ${origin}`}
              >
                <X size={14} />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex flex-col gap-2 sm:flex-row"
        onSubmit={(event) => {
          event.preventDefault()
          void submit()
        }}
      >
        <Input
          id={inputId}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={placeholder}
          disabled={disabled}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          className="h-9 min-w-0 flex-1 font-mono text-sm"
        />
        <Button
          type="submit"
          size="sm"
          variant="secondary"
          disabled={disabled || busy || !draft.trim()}
        >
          <Plus size={14} className="mr-1" />
          Add
        </Button>
      </form>
      <ErrorLine message={error} />
    </div>
  )
}

interface PreapprovalEditorProps {
  label: string
  description: string
  /** Action classes the scope may pre-approve. */
  actions: readonly BrowserActionClass[]
  entries: BrowserPreapproval[]
  disabled: boolean
  onAdd: (preapproval: BrowserPreapproval) => Promise<BrowserPolicyFailure | null>
  onRemove: (preapproval: BrowserPreapproval) => Promise<BrowserPolicyFailure | null>
}

/** Pre-approvals keyed by (action, origin), with a form to add one. */
export function PreapprovalEditor({
  label,
  description,
  actions,
  entries,
  disabled,
  onAdd,
  onRemove
}: PreapprovalEditorProps): React.JSX.Element {
  const originId = useId()
  const [action, setAction] = useState<BrowserActionClass>(actions[0])
  const [origin, setOrigin] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = async (): Promise<void> => {
    const trimmed = origin.trim()
    if (!trimmed) return
    setBusy(true)
    const failure = await onAdd({ action, origin: trimmed })
    setBusy(false)
    setError(failure ? describeFailure(failure) : null)
    if (!failure) setOrigin('')
  }

  const remove = async (entry: BrowserPreapproval): Promise<void> => {
    setBusy(true)
    const failure = await onRemove(entry)
    setBusy(false)
    setError(failure ? describeFailure(failure) : null)
  }

  return (
    <div className="space-y-2">
      <div>
        <Label htmlFor={originId} className="text-sm text-foreground">
          {label}
        </Label>
        <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
      </div>
      {entries.length === 0 ? (
        <p className="text-xs text-muted-foreground">None yet.</p>
      ) : (
        <ul className="space-y-1.5" aria-label={`${label} list`}>
          {entries.map((entry) => (
            <li key={`${entry.action}|${entry.origin}`} className={ROW_CLASS}>
              <span className="min-w-0 flex-1 text-xs text-foreground">
                <span>{ACTION_LABELS[entry.action]}</span>
                <span className="text-muted-foreground"> on </span>
                <span className="break-all font-mono">{entry.origin}</span>
              </span>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                disabled={disabled || busy}
                onClick={() => void remove(entry)}
                aria-label={`Remove ${ACTION_LABELS[entry.action]} on ${entry.origin}`}
              >
                <X size={14} />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex flex-col gap-2 sm:flex-row"
        onSubmit={(event) => {
          event.preventDefault()
          void submit()
        }}
      >
        <Select
          value={action}
          onValueChange={(value) => setAction(value as BrowserActionClass)}
          disabled={disabled}
        >
          <SelectTrigger className="h-9 sm:w-60" aria-label="Action to pre-approve">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {actions.map((candidate) => (
              <SelectItem key={candidate} value={candidate}>
                {ACTION_LABELS[candidate]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Input
          id={originId}
          value={origin}
          onChange={(event) => setOrigin(event.target.value)}
          placeholder="https://example.com"
          disabled={disabled}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          className="h-9 min-w-0 flex-1 font-mono text-sm"
        />
        <Button
          type="submit"
          size="sm"
          variant="secondary"
          disabled={disabled || busy || !origin.trim()}
        >
          <Plus size={14} className="mr-1" />
          Add
        </Button>
      </form>
      <ErrorLine message={error} />
    </div>
  )
}
