<script lang="ts">
  import { getModalStore } from '$lib/ui/stores'
  import { Dialog } from 'bits-ui'

  const modalStore = getModalStore()
  const parent = {
    onClose: () => modalStore.close(),
    regionFooter: 'flex items-center'
  }

  $: current = $modalStore[0]
</script>

{#if current}
  <Dialog.Root
    open={true}
    onOpenChange={(open) => {
      if (!open) modalStore.close()
    }}
  >
    <Dialog.Portal>
      <Dialog.Overlay
        class="fixed inset-0 z-[9998] bg-black/50 backdrop-blur-sm"
      />
      <Dialog.Content
        class="fixed inset-0 z-[9999] flex items-start justify-center overflow-y-auto p-4 outline-none"
      >
        <Dialog.Title class="sr-only">{current.title ?? 'Dialog'}</Dialog.Title>
        <svelte:component
          this={current.component.ref}
          {...current.component.props ?? {}}
          {parent}
        />
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>
{/if}
