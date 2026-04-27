<script lang="ts">
  type Props = {
    listening: boolean;
    label?: string;
  };

  let { listening, label = "Listening" }: Props = $props();
</script>

<div
  data-testid="listen-pill"
  data-listening={listening}
  class="bg-foreground text-background flex items-center gap-2 rounded-full px-4 py-2 text-sm font-medium shadow-lg"
  class:animate-pulse={listening}
>
  <span class="block size-2 rounded-full {listening ? 'bg-red-500' : 'bg-muted-foreground'}"></span>
  <span>{listening ? label : "Idle"}</span>
</div>
