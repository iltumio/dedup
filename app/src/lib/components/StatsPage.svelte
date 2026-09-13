<script lang="ts">
  import UiSelect from "./ui/UiSelect.svelte";
  import {
    getExtensionStats,
    formatSize,
    type ExtensionStats,
  } from "$lib/api/tauri";
  import Icon from "./ui/Icon.svelte";
  let {
    archiveName = "Current archive",
    emptyArchive = false,
  }: { archiveName?: string; emptyArchive?: boolean } = $props();
  let stats = $state<ExtensionStats[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let refresh = $state(0);
  let sort = $state("files");
  const sorted = $derived(
    [...stats].sort((a, b) =>
      sort === "copies"
        ? b.duplicate_files - a.duplicate_files
        : sort === "size"
          ? b.total_original_bytes - a.total_original_bytes
          : b.total_files - a.total_files,
    ),
  );
  const files = $derived(stats.reduce((s, e) => s + e.total_files, 0));
  const copies = $derived(stats.reduce((s, e) => s + e.duplicate_files, 0));
  const original = $derived(
    stats.reduce((s, e) => s + e.total_original_bytes, 0),
  );
  const stored = $derived(stats.reduce((s, e) => s + e.total_stored_bytes, 0));
  let limit = $state(100);
  $effect(() => {
    refresh;
    let active = true;
    loading = true;
    error = null;
    getExtensionStats()
      .then((data) => {
        if (active) stats = data;
      })
      .catch((e) => {
        if (active && !(emptyArchive && String(e).includes("No store loaded")))
          error = String(e);
      })
      .finally(() => {
        if (active) loading = false;
      });
    return () => {
      active = false;
    };
  });
</script>

<div class="overview-page">
  <div class="flex justify-between gap-4 items-center">
    <div>
      <h2 class="page-title">Your archive at a glance</h2>
      <p class="muted">
        Current contents of {archiveName}. Original files are unchanged.
      </p>
    </div>
    <button
      class="icon-button"
      type="button"
      disabled={loading}
      aria-label="Refresh overview"
      onclick={() => refresh++}><Icon name="refresh" /></button
    >
  </div>
  {#if loading}<div class="empty-panel" role="status">
      Reading archive statistics…
    </div>{:else if error}<div class="notice notice-error mt-6" role="alert">
      <p>{error}</p>
      <button class="btn" type="button" onclick={() => refresh++}
        >Try again</button
      >
    </div>{:else}
    <div class="scan-metrics">
      <div>
        <span>Archived files</span><strong>{files.toLocaleString()}</strong>
      </div>
      <div>
        <span>Duplicate files</span><strong>{copies.toLocaleString()}</strong>
      </div>
      <div>
        <span>Original data</span><strong>{formatSize(original)}</strong>
      </div>
      <div>
        <span>Stored content</span><strong>{formatSize(stored)}</strong>
      </div>
    </div>
    <p class="muted text-sm mb-6">
      {stored <= original
        ? `${formatSize(original - stored)} less content storage in this archive through deduplication and compression.`
        : "Content storage includes compression overhead."} Database and filesystem
      overhead are not included.
    </p>
    <div class="flex justify-between items-center mb-4">
      <h3 class="section-heading">File types</h3>
      <UiSelect
        label="Sort file types"
        bind:value={sort}
        options={[
          { value: "files", label: "Most files" },
          { value: "copies", label: "Most duplicates" },
          { value: "size", label: "Largest total size" },
        ]}
      />
    </div>
    {#if !stats.length}<div class="empty-panel">
        <h2>No files archived yet</h2>
        <p>Add a folder to see its storage breakdown.</p>
      </div>{:else}<table class="overview-table">
        <thead
          ><tr
            ><th scope="col">Type</th><th scope="col">Files</th><th scope="col"
              >Duplicates</th
            ><th scope="col">Original size</th></tr
          ></thead
        ><tbody
          >{#each sorted.slice(0, limit) as item (item.extension)}<tr
              ><td
                >{item.extension === "(none)"
                  ? "No extension"
                  : `.${item.extension}`}</td
              ><td>{item.total_files.toLocaleString()}</td><td
                >{item.duplicate_files.toLocaleString()}</td
              ><td>{formatSize(item.total_original_bytes)}</td></tr
            >{/each}</tbody
        >
      </table>
      {#if sorted.length > limit}<button
          class="btn mt-4"
          type="button"
          onclick={() => (limit += 100)}>Show more types</button
        >{/if}{/if}
  {/if}
</div>
