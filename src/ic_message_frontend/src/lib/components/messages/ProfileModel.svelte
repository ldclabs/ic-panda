<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { type SvelteComponent } from 'svelte'
  import ProfileDetail from './ProfileDetail.svelte'
  import { type Principal } from '@dfinity/principal'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let userId: Principal
</script>

<ModalCard {parent}>
  <ProfileDetail {userId} {myState} />
</ModalCard>
