<script lang="ts">
  import { ICON_CIRCLES, ICON_PATHS, type IconName } from "../lib/icons";

  interface Props {
    name: IconName;
    size?: number;
  }

  let { name, size = 16 }: Props = $props();

  const circles = $derived(ICON_CIRCLES[name] ?? []);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={ICON_PATHS[name]} />
  {#each circles as circle (`${circle.cx}:${circle.cy}`)}
    <circle
      cx={circle.cx}
      cy={circle.cy}
      r={circle.r}
      fill={circle.filled ? "currentColor" : "none"}
      stroke={circle.filled ? "none" : "currentColor"}
    />
  {/each}
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
