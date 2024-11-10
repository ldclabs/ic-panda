<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { type Principal } from '@dfinity/principal'
  import { type SvelteComponent } from 'svelte'
  import ProfileDetail from './ProfileDetail.svelte'
  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
    userId: Principal
  }

  let { parent, myState, userId }: Props = $props()
</script>

<ModalCard {parent}>
  <ProfileDetail {userId} {myState} />
</ModalCard>
