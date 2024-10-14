<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { type Principal } from '@dfinity/principal'
  import { type SvelteComponent } from 'svelte'
  import ProfileDetail from './ProfileDetail.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let userId: Principal
</script>

<ModalCard {parent}>
  <ProfileDetail {userId} {myState} />
</ModalCard>
