<script lang="ts">
  import Avatar from '../../components/Avatar.svelte'
  import { avatarOf, label } from './names.svelte'
  let { account, size = 'md' }: { account: string; size?: 'sm' | 'md' | 'lg' } = $props()
  let visible = $state(false)
  const observe = (node: Element) => {
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return
      visible = true
      observer.disconnect()
    })
    observer.observe(node)
    return () => observer.disconnect()
  }
</script>

<span {@attach observe} class="shrink-0">
  <Avatar id={account} {size} name={label(account, visible)} src={visible ? avatarOf(account) : null} />
</span>
