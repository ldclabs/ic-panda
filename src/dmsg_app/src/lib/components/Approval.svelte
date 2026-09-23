<script lang="ts">
  import { onMount } from 'svelte'
  import { listRequests } from '../requests'
  import { session } from '../session.svelte'
  import DocumentApproval from './DocumentApproval.svelte'
  import AuthenticationApproval from './AuthenticationApproval.svelte'
  let requested = $state<'authentication' | 'document' | null>(null)
  let kind = $state<'authentication' | 'document' | null>(null)
  onMount(() => {
    const id = new URLSearchParams(location.search).get('id')
    void listRequests()
      .then((rows) => {
        const record = rows.find((r) => r.id === id)
        if (!record || !['authentication', 'document'].includes(record.kind))
          throw new Error('请求不存在或格式无效。')
        requested = record.kind
      })
      .catch((error) => {
        session.error = error instanceof Error ? error.message : '无法读取请求。'
      })
  })
  // Child initialization uses the session mutex. Mount only after unlock has
  // released it; do not unmount the child when its own operation becomes busy.
  $effect(() => {
    if (!kind && requested && !session.busy) kind = requested
  })
</script>

{#if kind === 'authentication'}<AuthenticationApproval
  />{:else if kind === 'document'}<DocumentApproval />{:else if session.error}<p role="alert">
    {session.error}
  </p>{/if}
