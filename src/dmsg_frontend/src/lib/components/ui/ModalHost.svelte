<script lang="ts">
  import { t } from '$lib/i18n'
  import { getModalStore } from '$lib/ui/stores'
  import { Dialog } from 'bits-ui'

  const modalStore = getModalStore()

  // The only member of Skeleton's `parent` object the application ever used.
  const parent = {
    onClose: () => modalStore.close()
  }

  const current = $derived($modalStore[0])
  const Component = $derived(current?.component.ref)
</script>

{#if current}
  <Dialog.Root
    open={true}
    onOpenChange={(open: boolean) => {
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
        <!-- The modal bodies draw their own heading; this keeps the dialog
             labelled for assistive technology without showing twice. -->
        <Dialog.Title class="sr-only"
          >{$t(current.title ?? 'Dialog')}</Dialog.Title
        >
        {#key current}
          <Component {...current.component.props ?? {}} {parent} />
        {/key}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>
{/if}
