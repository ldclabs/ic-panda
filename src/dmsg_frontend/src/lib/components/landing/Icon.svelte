<script lang="ts">
  let { name }: { name: string } = $props()
</script>

<img
  class="icon"
  src={`/_assets/dmsg/${name}.svg`}
  width="20"
  height="20"
  alt=""
  aria-hidden="true"
/>
