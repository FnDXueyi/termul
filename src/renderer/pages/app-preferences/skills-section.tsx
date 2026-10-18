import { useCallback } from 'react'
import { SettingsCard, SettingsGroup } from '@/components/settings/SettingsGroup'
import { SettingsSection } from '@/components/settings/SettingsLayout'
import { SettingsRow } from '@/components/settings/SettingsRow'
import { Button } from '@/components/ui/button'
import { useAgentSkills } from '@/hooks/use-agent-skills'
import { logFrontendError } from '@/lib/log-api'
import type { AgentSkillSummary } from '@/lib/skills-api'
import { useEditorStore } from '@/stores/editor-store'
import { useActiveProject } from '@/stores/project-store'
import { useSettingsModalStore } from '@/stores/settings-modal-store'
import { useWorkspaceStore } from '@/stores/workspace-store'

function skillsIn(skills: AgentSkillSummary[], scope: 'global' | 'project'): AgentSkillSummary[] {
  return skills.filter((skill) => skill.scope === scope)
}

function SkillRows({ skills }: { skills: AgentSkillSummary[] }): React.JSX.Element {
  const openSkill = useCallback(async (skill: AgentSkillSummary) => {
    try {
      await useEditorStore.getState().openFile(skill.path)
      if (!useEditorStore.getState().openFiles.has(skill.path)) {
        throw new Error('editor tab limit reached')
      }
      useWorkspaceStore.getState().addEditorTab(skill.path)
      useSettingsModalStore.getState().close()
    } catch (err) {
      void logFrontendError({
        level: 'warn',
        source: 'SkillsSection',
        message: `Failed to open skill '${skill.name}': ${err instanceof Error ? err.message : String(err)}`
      })
    }
  }, [])

  if (skills.length === 0) {
    return (
      <div className="rounded-lg border border-border bg-card px-4 py-3 text-xs text-muted-foreground">
        No skills in this folder.
      </div>
    )
  }

  return (
    <SettingsCard>
      {skills.map((skill) => (
        <SettingsRow
          key={`${skill.scope}:${skill.name}`}
          label={skill.name}
          description={skill.description || undefined}
          layout="inline"
        >
          <Button
            type="button"
            size="xs"
            variant="outline"
            onClick={() => void openSkill(skill)}
            aria-label={`Open ${skill.name}`}
          >
            Open
          </Button>
        </SettingsRow>
      ))}
    </SettingsCard>
  )
}

export function SkillsSection(): React.JSX.Element {
  const project = useActiveProject()
  const { skills: listed, loading } = useAgentSkills(project?.path)
  const skills = Array.isArray(listed) ? listed : []
  const globalSkills = skillsIn(skills, 'global')
  const projectSkills = skillsIn(skills, 'project')

  const count = (n: number) => (n === 1 ? '1 skill' : `${n} skills`)
  return (
    <SettingsSection id="skills">
      {loading ? (
        <p className="text-sm text-muted-foreground">Loading skills…</p>
      ) : (
        <>
          <SettingsGroup title={`Global · ${count(globalSkills.length)}`}>
            <SkillRows skills={globalSkills} />
          </SettingsGroup>
          <SettingsGroup
            title={
              project ? `Project · ${project.name} · ${count(projectSkills.length)}` : 'Project'
            }
          >
            {project ? (
              <SkillRows skills={projectSkills} />
            ) : (
              <div className="rounded-lg border border-border bg-card px-4 py-3 text-xs text-muted-foreground">
                Open a project to see its skills.
              </div>
            )}
          </SettingsGroup>
        </>
      )}
    </SettingsSection>
  )
}
