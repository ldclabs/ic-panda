<script lang="ts">
  let { image }: { image: string } = $props()
</script>

<div data-crop-image={image}></div>
