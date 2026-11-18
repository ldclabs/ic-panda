<script lang="ts">
  import Avatar from '../../components/Avatar.svelte'
  import { visibility } from '../../components/visibility'
  import { avatarOf, label } from './names.svelte'
  let { account, size = 'md' }: { account: string; size?: 'sm' | 'md' | 'lg' } = $props()
  let visible = $state(false)
  const observe = visibility(value => { visible = value })
</script>

<span {@attach observe} class="shrink-0">
  <Avatar id={account} {size} name={label(account, visible)} src={visible ? avatarOf(account) : null} />
</span>
