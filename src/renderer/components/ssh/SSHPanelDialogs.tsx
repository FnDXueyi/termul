import type { SSHProfile } from '@shared/types/ssh.types'
import { useState } from 'react'
import { toast } from 'sonner'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { useSSHActions, useSSHConnections } from '@/stores/ssh-store'
import { SSHProfileForm } from './SSHProfileForm'

/** State and handlers behind the SSH profile form and the delete confirmation. */
export interface SSHPanelDialogsController {
  /** True while the profile form or the delete confirmation is open. */
  isOpen: boolean
  /** Opens the profile form; `null` = new profile. */
  openForm: (profile: SSHProfile | null) => void
  /** Asks to delete `profile` (opens the confirmation). */
  requestDelete: (profile: SSHProfile) => void
  form: { profile: SSHProfile | null } | null
  closeForm: () => void
  saved: () => void
  deleteConfirm: SSHProfile | null
  cancelDelete: () => void
  confirmDelete: () => void
}

/**
 * Lifts the profile form and delete-confirm handling out of `SSHPanel` so a
 * host that cannot keep them inside its own subtree (the Rail popover closes
 * first, then opens the dialog) can own them.
 */
export function useSSHPanelDialogs(activeProfileId?: string | null): SSHPanelDialogsController {
  const connections = useSSHConnections()
  const { loadProfiles, disconnect, deleteProfile, selectProfile } = useSSHActions()
  // Profile form: closed when null; `profile` null = new profile.
  const [form, setForm] = useState<{ profile: SSHProfile | null } | null>(null)
  // Delete is irreversible — gate it behind a confirmation dialog (mirrors
  // ProjectChatList) instead of deleting on the first menu click.
  const [deleteConfirm, setDeleteConfirm] = useState<SSHProfile | null>(null)
  const [deletingId, setDeletingId] = useState<string | null>(null)

  const handleDeleteProfile = async (profile: SSHProfile): Promise<void> => {
    // In-flight guard: a second confirm while the first delete is still
    // running is a no-op.
    if (deletingId !== null) return
    setDeletingId(profile.id)
    try {
      // Tear down any live connection before removing the profile so no orphan
      // connection state survives the delete. A failed disconnect aborts the
      // delete — proceeding would orphan live connection state.
      const connection = connections.find((c) => c.profileId === profile.id)
      if (connection) {
        const disconnected = await disconnect(connection.id)
        if (!disconnected) {
          toast.error(`Could not disconnect from “${profile.name}” — profile not deleted`)
          return
        }
      }
      const success = await deleteProfile(profile.id)
      if (!success) {
        toast.error(`Failed to delete SSH profile “${profile.name}”`)
        return
      }
      // Clear the active selection only after the delete succeeded so a
      // failed delete never strands the user on an unselected live profile.
      if (activeProfileId === profile.id) {
        selectProfile(null)
      }
    } catch (error) {
      toast.error(
        `Failed to delete SSH profile “${profile.name}”: ${error instanceof Error ? error.message : String(error)}`
      )
    } finally {
      setDeletingId(null)
    }
  }

  return {
    isOpen: form !== null || deleteConfirm !== null,
    openForm: (profile) => setForm({ profile }),
    requestDelete: setDeleteConfirm,
    form,
    closeForm: () => setForm(null),
    saved: () => {
      setForm(null)
      loadProfiles()
    },
    deleteConfirm,
    cancelDelete: () => setDeleteConfirm(null),
    confirmDelete: () => {
      if (deleteConfirm) {
        const profile = deleteConfirm
        setDeleteConfirm(null)
        void handleDeleteProfile(profile)
      }
    }
  }
}

/** The SSH profile form modal and the delete confirmation for a controller. */
export function SSHPanelDialogs({
  controller
}: {
  controller: SSHPanelDialogsController
}): React.JSX.Element {
  const { form, deleteConfirm } = controller
  return (
    <>
      {/* Profile Form Modal */}
      {form && (
        <SSHProfileForm
          profile={form.profile}
          onClose={controller.closeForm}
          onSaved={controller.saved}
        />
      )}

      <ConfirmDialog
        isOpen={deleteConfirm !== null}
        title="Delete SSH profile"
        message={
          deleteConfirm ? `Delete “${deleteConfirm.name}”? This action cannot be undone.` : ''
        }
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        onConfirm={controller.confirmDelete}
        onCancel={controller.cancelDelete}
      />
    </>
  )
}
