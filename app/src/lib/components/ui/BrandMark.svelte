<script lang="ts">
  // Same artwork as src-tauri/icons/icon.svg, cropped to the plate.
  let { size = 32 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="100 100 824 824"
  aria-hidden="true"
  class="shrink-0"
  ><defs
    ><linearGradient id="brand-plate" x1="0" y1="0" x2="0" y2="1"
      ><stop offset="0" stop-color="#3f74e6" /><stop
        offset="1"
        stop-color="#2553bd"
      /></linearGradient
    ></defs
  ><rect
    x="100"
    y="100"
    width="824"
    height="824"
    rx="185"
    fill="url(#brand-plate)"
  /><g
    fill="none"
    stroke="#fff"
    stroke-width="34"
    stroke-linejoin="round"
    stroke-linecap="round"
    stroke-opacity=".55"
    ><path d="M262 282h94l46 46v130H262z" /><path
      d="M262 566h94l46 46v130H262z"
    /><path
      d="M438 393c60 0 70 119 132 119M438 677c60 0 70-165 132-165"
    /></g
  ><path
    d="M600 362h112l70 70v218a22 22 0 0 1-22 22H600a22 22 0 0 1-22-22V384a22 22 0 0 1 22-22z"
    fill="#fff"
  /><path d="M712 362v48a22 22 0 0 0 22 22h48z" fill="#b9cdf8" /></svg
>
