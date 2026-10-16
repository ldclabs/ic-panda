<script lang="ts">
  // Remix Icon 4.6.0 files in static/_assets/dmsg (Apache-2.0, see
  // Remix-LICENSE.txt there); icon.svg and mark.svg are dMsg's own.
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
