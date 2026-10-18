import { MoreHorizontal } from '@/components/icons'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger
} from '@/components/ui/dropdown-menu'

interface RowMenuProps {
  /** Accessible name, e.g. "Actions for filesystem". */
  label: string
  /** `DropdownMenuItem`s. */
  children: React.ReactNode
  disabled?: boolean
}

/** The … menu at the end of a list row. Items are composed by the caller. */
export function RowMenu({ label, children, disabled }: RowMenuProps): React.JSX.Element {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label={label}
          disabled={disabled}
          className="size-7 text-muted-foreground data-[state=open]:bg-foreground/[0.06] data-[state=open]:text-foreground"
        >
          <MoreHorizontal size={14} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-44">
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
