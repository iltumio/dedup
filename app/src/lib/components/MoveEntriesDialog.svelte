<script lang="ts">
  import { createDir, listDir, moveEntries, type DirEntry } from "$lib/api/tauri";
  import UiDialog from "./ui/UiDialog.svelte";
  import Icon from "./ui/Icon.svelte";

  let { sources, initialPath, onClose, onMoved, onCreated }: {
    sources: string[];
    initialPath: string;
    onClose: () => void;
    onMoved: () => void;
    onCreated: () => void;
  } = $props();
  let destination = $state("/");
  $effect(() => { destination = initialPath; });
  let folders = $state<DirEntry[]>([]);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let folderName = $state("");
  const crumbs = $derived(destination.split("/").filter(Boolean));
  const sameFolder = $derived(destination === initialPath);
  const validName = $derived(folderName.trim() !== "" && !/[\/\\\x00-\x1f]/.test(folderName)
    && folderName.trim() !== "." && folderName.trim() !== "..");

  $effect(() => {
    const path = destination;
    let active = true;
    loading = true;
    error = null;
    folders = [];
    listDir(path).then(entries => {
      if (active) folders = entries.filter(entry => entry.is_dir && !sources.some(source => {
        const candidate = `${path === "/" ? "" : path}/${entry.name}`;
        return candidate === source || candidate.startsWith(`${source}/`);
      }));
    }).catch(e => { if (active) error = String(e); })
      .finally(() => { if (active) loading = false; });
    return () => { active = false; };
  });

  async function createFolder() {
    if (busy || loading || !validName) return;
    busy = true;
    error = null;
    try {
      const path = `${destination === "/" ? "" : destination}/${folderName.trim()}`;
      await createDir(path);
      folderName = "";
      destination = path;
      onCreated();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  async function move() {
    if (busy || loading || sameFolder) return;
    busy = true;
    error = null;
    try {
      await moveEntries(sources, destination);
      onMoved();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<UiDialog open title="Move selected items"
  description={`Choose a destination for ${sources.length} selected ${sources.length === 1 ? "item" : "items"}. Existing files will not be replaced.`}
  closeDisabled={busy} {onClose}>
  <nav aria-label="Destination folder path" class="flex flex-wrap items-center gap-1 mb-3">
    <button class="btn btn-sm" type="button" disabled={busy} onclick={() => destination = "/"}>All files</button>
    {#each crumbs as crumb, i}
      <Icon name="chevron" size={12} />
      <button class="btn btn-sm max-w-full truncate" type="button" disabled={busy}
        aria-current={i === crumbs.length - 1 ? "location" : undefined}
        onclick={() => destination = "/" + crumbs.slice(0, i + 1).join("/")}>{crumb}</button>
    {/each}
  </nav>
  <p class="text-sm break-all mb-3">Destination: <strong>{destination}</strong></p>
  <div class="border border-base-300 rounded-lg max-h-48 overflow-auto" aria-busy={loading}>
    {#if loading}<p class="p-3 text-sm" role="status">Loading folders…</p>
    {:else if folders.length}
      {#each folders as folder}
        <button class="btn btn-ghost w-full justify-start rounded-none" type="button" disabled={busy}
          onclick={() => destination = `${destination === "/" ? "" : destination}/${folder.name}`}>
          <Icon name="folder" size={18} /><span class="truncate">{folder.name}</span><Icon name="chevron" size={14} />
        </button>
      {/each}
    {:else}<p class="p-3 text-sm text-base-content/70">No subfolders. You can create one below.</p>{/if}
  </div>
  <form class="mt-4" onsubmit={(event) => { event.preventDefault(); void createFolder(); }}>
    <label class="block text-sm mb-1" for="move-new-folder">New folder name</label>
    <div class="flex gap-2">
      <input id="move-new-folder" class="input input-bordered min-w-0 flex-1" bind:value={folderName} disabled={busy || loading} autocomplete="off" />
      <button class="btn" type="submit" disabled={busy || loading || !validName}>Create folder</button>
    </div>
  </form>
  {#if error}<p role="alert" class="notice notice-error mt-3 break-all">{error}</p>{/if}
  {#if sameFolder}<p class="text-sm text-base-content/70 mt-3">Choose another folder or create a new one.</p>{/if}
  {#snippet actions()}
    <button class="btn" type="button" disabled={busy} onclick={onClose}>Cancel</button>
    <button class="btn btn-primary" type="button" disabled={busy || loading || sameFolder} onclick={move}>
      {busy ? "Working…" : "Move here"}
    </button>
  {/snippet}
</UiDialog>
