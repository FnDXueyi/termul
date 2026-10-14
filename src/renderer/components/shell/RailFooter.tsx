import { useEffect, useRef } from 'react'
import { Palette, SlidersHorizontal } from '@/components/icons'
import { RailRow } from '@/components/shell/RailRow'
import { RailButton, railButtonClass } from '@/components/shell/rail-button'
import { RailToggleButton } from '@/components/shell/TopRowToggles'
import { useShortcutLabel } from '@/components/shell/use-shortcut-label'
import { TitleBarShortcutsPopover } from '@/components/TitleBarShortcutsPopover'
import { useSettingsModalStore, useSettingsModalView } from '@/stores/settings-modal-store'

interface RailFooterProps {
  isShortcutsOpen: boolean
  onShortcutsOpenChange: (open: boolean) => void
  isThemePickerOpen: boolean
  onToggleThemePicker?: () => void
}

/**
 * Expanded Rail footer, one row: the Rail toggle, the Settings row, then the
 * 28px Theme picker and Shortcuts buttons. Closing the picker or the shortcuts
 * popover returns focus to its button when focus fell to the page body.
 */
export function RailFooter({
  isShortcutsOpen,
  onShortcutsOpenChange,
  isThemePickerOpen,
  onToggleThemePicker
}: RailFooterProps): React.JSX.Element {
  const settingsView = useSettingsModalView()
  const settingsShortcut = useShortcutLabel('settings')
  const buttonsRef = useRef<HTMLDivElement>(null)
  const wasThemePickerOpen = useRef(false)

  useEffect(() => {
    const wasOpen = wasThemePickerOpen.current
    wasThemePickerOpen.current = isThemePickerOpen
    if (!wasOpen || isThemePickerOpen) return
    // The picker unmounts with the store flag; wait one frame so the browser
    // has moved focus to the body before deciding whether to restore it.
    const frame = requestAnimationFrame(() => {
      const active = document.activeElement
      if (active && active !== document.body) return
      buttonsRef.current
        ?.querySelector<HTMLButtonElement>('button[aria-label="Color themes"]')
        ?.focus()
    })
    return () => cancelAnimationFrame(frame)
  }, [isThemePickerOpen])

  return (
    // One row: the Rail toggle (bottom-left, x center 24 like the collapsed
    // Rail), Settings in the free width, then the two 28px icon buttons.
    <div className="flex shrink-0 items-center gap-0.5 px-1.5 pb-2 pt-1">
      <RailToggleButton />
      <div className="min-w-0 flex-1">
        <RailRow
          label="Settings"
          hint={settingsShortcut}
          icon={<SlidersHorizontal size={16} />}
          pressed={settingsView === 'app'}
          onClick={() => useSettingsModalStore.getState().openApp()}
        />
      </div>

      <div ref={buttonsRef} className="flex shrink-0 items-center gap-0.5">
        <RailButton
          label="Color themes"
          size="sm"
          aria-label="Color themes"
          pressed={onToggleThemePicker ? isThemePickerOpen : undefined}
          disabled={!onToggleThemePicker}
          onClick={() => onToggleThemePicker?.()}
        >
          <Palette size={16} />
        </RailButton>

        <TitleBarShortcutsPopover
          buttonClassName={railButtonClass(isShortcutsOpen, 'sm')}
          open={isShortcutsOpen}
          onOpenChange={onShortcutsOpenChange}
          restoreFocusOnClose
        />
      </div>
    </div>
  )
}
