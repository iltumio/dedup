<script lang="ts">
  let { name, size = 20 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    folder:
      "M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z",
    file: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Zm0 0v6h6",
    copies: "M8 8h12v12H8zM16 8V4H4v12h4",
    activity: "M3 12h4l3-8 4 16 3-8h4",
    chart: "M4 3v17h17M9 16v-5m5 5V6m5 10V9",
    plus: "M12 5v14M5 12h14",
    chevron: "m9 5 7 7-7 7",
    back: "m12 5-7 7 7 7M5 12h15",
    search: "M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
    check: "m5 12 4 4L19 6",
    close: "m6 6 12 12M6 18 18 6",
    archive: "M3 3h18v5H3zM5 8v13h14V8M10 12h4",
    settings: "M4 6h16M4 12h16M4 18h16M8 3v6m8 0v6m-6 0v6",
    refresh: "M20 7v5h-5M4 17v-5h5M6 6a8 8 0 0 1 14 6M4 12a8 8 0 0 0 14 6",
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  class="shrink-0"><path d={paths[name] ?? paths.file} /></svg
>
