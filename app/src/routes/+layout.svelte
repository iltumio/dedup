<script lang="ts">
  import "../app.css";
  import { app } from "$lib/state/app.svelte";
  let { children } = $props();
  $effect(() => {
    app.loadWorkspaces();
    app.loadCustomScanRules();
  });
  $effect(() => {
    try {
      const stored = localStorage.getItem("dedup-theme");
      if (stored === "dark" || stored === "light" || stored === "system")
        app.theme = stored;
    } catch {
      /* Optional preference. */
    }
  });
  $effect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () =>
      (document.documentElement.dataset.theme =
        app.theme === "system"
          ? media.matches
            ? "night"
            : "light"
          : app.theme === "dark"
            ? "night"
            : "light");
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  });
</script>

{@render children()}
