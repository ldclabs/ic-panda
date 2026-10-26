<script lang="ts">
  import Id from '../../components/page/Id.svelte'
  import { buttonClass } from '../../components/ui'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  let {
    index,
    navigate
  }: { index: any; navigate?: (tab: string, id?: string) => void } = $props()
  const destination = (kind: string) =>
    ({
      policy: 'policies',
      policy_grant: 'policies',
      mount: 'mounts',
      ssh: 'ssh',
      snapshot_link: 'sharing',
      snapshot_channel: 'sharing',
      process_observation: 'activity'
    })[kind] ?? 'activity'
</script>

<div class="space-y-2" data-testid="secret-dependencies">
  {#each index.dependencies as entry}<div
      class="rounded-lg border border-line p-3 text-sm"
    >
      <div class="flex flex-wrap justify-between gap-2">
        <span class="font-medium break-all">{entry.label}</span><span
          class="text-xs text-muted"
          >{term(entry.kind)} · {term(entry.state)}</span
        >
      </div>
      {#if navigate}<button
          class={buttonClass('ghost')}
          onclick={() => navigate?.(destination(entry.kind), entry.id)}
          >{w('review')}</button
        >{/if}
      <details>
        <summary class="cursor-pointer text-xs text-muted"
          >{ux('details')}</summary
        ><Id id={entry.id} full wrap />
        <p class="break-all text-xs">{entry.action}</p>
      </details>
    </div>{:else}<p class="text-sm text-muted">{w('noDependencies')}</p>{/each}
</div>
