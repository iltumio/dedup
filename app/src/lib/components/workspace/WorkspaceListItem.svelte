<script lang="ts">
  import { formatTimestamp, type Workspace } from "$lib/api/tauri";
  import Icon from "../ui/Icon.svelte";
  let {
    workspace,
    active,
    onSelect,
    onDelete,
  }: {
    workspace: Workspace;
    active: boolean;
    onSelect: (id: string) => void;
    onDelete: (id: string) => void;
  } = $props();
</script>

<li class="duplicate-group !mb-0">
  <div class="flex items-start gap-3">
    <span class="folder-color pt-2"><Icon name="archive" /></span><button
      class="min-w-0 flex-1 text-left"
      type="button"
      onclick={() => onSelect(workspace.id)}
      ><span class="flex items-center gap-2"
        ><strong class="truncate">{workspace.label}</strong>{#if active}<span
            class="count-badge">Current</span
          >{/if}</span
      ><span
        class="block font-path text-xs muted truncate my-2"
        title={workspace.store_path}>{workspace.store_path}</span
      ><span class="text-xs muted"
        >{workspace.stats.last_scan_at
          ? `Last scan: ${formatTimestamp(workspace.stats.last_scan_at)}`
          : "Ready for its first scan"}</span
      ></button
    >
  </div>
  <details class="mt-3">
    <summary class="muted text-xs py-2">Archive options</summary>
    <p class="muted text-sm mb-2">
      Removing this archive from the list keeps all its data on disk.
    </p>
    <button
      class="btn btn-sm btn-ghost"
      type="button"
      onclick={() => onDelete(workspace.id)}>Remove from list</button
    >
  </details>
</li>
