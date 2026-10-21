<script lang="ts">
  import UiSelect from "./ui/UiSelect.svelte";
  import {
    listDir,
    createDir,
    formatSize,
    formatTimestamp,
    type DirEntry,
  } from "$lib/api/tauri";
  import Icon from "./ui/Icon.svelte";
  import UiDialog from "./ui/UiDialog.svelte";
  import MoveEntriesDialog from "./MoveEntriesDialog.svelte";
  import { app } from "$lib/state/app.svelte";
  let {
    selectedPath,
    onSelect,
    onScanInto,
    initialPath = "/",
    expectContent = false,
    onNavigate,
    onChanged,
  }: {
    selectedPath: string | null;
    onSelect: (path: string, entry: DirEntry) => void;
    onScanInto: (path: string) => void;
    initialPath?: string;
    expectContent?: boolean;
    onNavigate?: (path: string) => void;
    onChanged?: () => void;
  } = $props();
  let currentPath = $state("/");
  let entries = $state<DirEntry[]>([]);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let query = $state("");
  let sort = $state("name");
  let page = $state(1);
  let refresh = $state(0);
  let checked = $state<string[]>([]);
  let showCreate = $state(false);
  let showMove = $state(false);
  let folderName = $state("");
  let editError = $state<string | null>(null);
  let editing = $state(false);
  const editsDisabled = $derived(app.scanning || app.migrating || editing);
  const validFolderName = $derived(folderName.trim() !== "" && !/[\/\\\x00-\x1f]/.test(folderName)
    && folderName.trim() !== "." && folderName.trim() !== "..");
  let searchInput: HTMLInputElement;
  const pageSize = 100;
  const crumbs = $derived(currentPath.split("/").filter(Boolean));
  const filtered = $derived(
    [...entries]
      .filter((e) =>
        e.name.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      )
      .sort(
        (a, b) =>
          Number(b.is_dir) - Number(a.is_dir) ||
          (sort === "size"
            ? b.size - a.size
            : sort === "modified"
              ? b.modified - a.modified
              : a.name.localeCompare(b.name)),
      ),
  );
  const pages = $derived(Math.max(1, Math.ceil(filtered.length / pageSize)));
  const visible = $derived(
    filtered.slice((page - 1) * pageSize, page * pageSize),
  );
  const allVisibleChecked = $derived(visible.length > 0 && visible.every(entry => checked.includes(entry.name)));
  $effect(() => {
    currentPath = initialPath;
  });
  $effect(() => {
    const path = currentPath;
    refresh;
    let active = true;
    loading = true;
    error = null;
    entries = [];
    listDir(path)
      .then((result) => {
        if (active) {
          entries = result;
          checked = checked.filter(name => result.some(entry => entry.name === name));
        }
      })
      .catch((e) => {
        if (
          active &&
          !(!expectContent && String(e).includes("No store loaded"))
        )
          error = String(e);
      })
      .finally(() => {
        if (active) loading = false;
      });
    return () => {
      active = false;
    };
  });
  $effect(() => {
    query;
    sort;
    currentPath;
    page = 1;
  });
  $effect(() => {
    if (page > pages) page = pages;
  });
  function navigate(path: string) {
    currentPath = path;
    checked = [];
    query = "";
    onNavigate?.(path);
  }
  function toggle(name: string) {
    checked = checked.includes(name) ? checked.filter(item => item !== name) : [...checked, name];
  }
  function toggleVisible() {
    checked = allVisibleChecked ? checked.filter(name => !visible.some(entry => entry.name === name))
      : [...new Set([...checked, ...visible.map(entry => entry.name)])];
  }
  async function createFolder() {
    if (editsDisabled || !validFolderName) return;
    editing = true;
    editError = null;
    try {
      await createDir(`${currentPath === "/" ? "" : currentPath}/${folderName.trim()}`);
      showCreate = false;
      query = "";
      refresh++;
      onChanged?.();
    } catch (e) { editError = String(e); }
    finally { editing = false; }
  }
  function open(entry: DirEntry) {
    const path = `${currentPath === "/" ? "" : currentPath}/${entry.name}`;
    if (entry.is_dir) navigate(path);
    else onSelect(path, entry);
  }
  function shortcut(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      searchInput?.focus();
    }
  }
</script>

<svelte:window onkeydown={shortcut} />
<div class="file-browser">
  <div class="browser-tools">
    <nav aria-label="Folder path" class="breadcrumbs">
      <button type="button" onclick={() => navigate("/")}
        ><Icon name="archive" size={17} />All files</button
      >{#each crumbs as crumb, i}<Icon name="chevron" size={12} /><button
          type="button"
          title={crumb}
          aria-current={i === crumbs.length - 1 ? "location" : undefined}
          onclick={() => navigate("/" + crumbs.slice(0, i + 1).join("/"))}
          >{crumb}</button
        >{/each}
    </nav>
    <button
      class="icon-button"
      type="button"
      aria-label="Refresh folder"
      disabled={loading}
      onclick={() => refresh++}><Icon name="refresh" size={18} /></button
    >
  </div>
  <div class="browser-search">
    <label class="search-box"
      ><Icon name="search" size={18} /><input
        bind:this={searchInput}
        bind:value={query}
        aria-label="Search this folder"
        placeholder="Search this folder…"
      /><span class="shortcut">⌘ / Ctrl F</span></label
    ><UiSelect
      class="file-sort"
      label="Sort files"
      bind:value={sort}
      options={[
        { value: "name", label: "Name" },
        { value: "size", label: "Largest first" },
        { value: "modified", label: "Recently modified" },
      ]}
    />
  </div>
  <div class="organize-tools">
    <span class="text-sm text-base-content/70" role="status">{checked.length ? `${checked.length} selected` : "Select items to move"}</span>
    <div class="flex gap-2">
      {#if checked.length}<button class="btn btn-sm" type="button" onclick={() => checked = []}>Clear selection</button>{/if}
      <button class="btn btn-sm" type="button" disabled={editsDisabled || loading || !!error}
        onclick={() => { folderName = ""; editError = null; showCreate = true; }}><Icon name="plus" size={15} />New folder</button>
      <button class="btn btn-sm" type="button" disabled={editsDisabled || loading || !checked.length}
        onclick={() => showMove = true}>Move selected</button>
    </div>
  </div>
  <div class="browser-scroll" aria-busy={loading}>
    {#if error}<div class="empty-panel">
        <h2>Folder unavailable</h2>
        <p class="break-all">{error}</p>
        <button class="btn" type="button" onclick={() => refresh++}
          >Try again</button
        >
      </div>
    {:else if loading}<div class="empty-panel" role="status">
        <span class="loading loading-spinner"></span>
        <p>Loading files…</p>
      </div>
    {:else if !filtered.length}<div class="empty-panel">
        <Icon name={query ? "search" : "folder"} size={36} />
        <h2>{query ? "No matching files" : "A little room for your files"}</h2>
        <p>
          {query
            ? "Try a different name. Search is limited to this folder."
            : "Add a folder to start building your archive."}
        </p>
        {#if query}<button
            class="btn"
            type="button"
            onclick={() => (query = "")}>Clear search</button
          >{:else}<button
            class="btn btn-primary"
            type="button"
            onclick={() => onScanInto(currentPath)}
            >{currentPath === "/"
              ? "Add your first folder"
              : "Add a folder here"}</button
          >{/if}
      </div>
    {:else}<table class="file-table">
        <thead
          ><tr
            ><th scope="col"><div class="name-with-selection"><input class="checkbox checkbox-sm" type="checkbox" aria-label="Select visible items" checked={allVisibleChecked} indeterminate={visible.some(entry => checked.includes(entry.name)) && !allVisibleChecked} disabled={editsDisabled} onchange={toggleVisible} /><span>Name</span></div></th><th scope="col">Size</th><th
              scope="col"
              class="modified-column">Modified</th
            ></tr
          ></thead
        ><tbody
          >{#each visible as entry (entry.name)}{@const fullPath = `${currentPath === "/" ? "" : currentPath}/${entry.name}`}<tr
              class:selected={selectedPath === fullPath}
              aria-selected={selectedPath === fullPath}
              ><td
                ><div class="name-with-selection"><input class="checkbox checkbox-sm" type="checkbox" aria-label={`Select ${entry.name}`} checked={checked.includes(entry.name)} disabled={editsDisabled} onchange={() => toggle(entry.name)} /><button
                  class="file-name"
                  type="button"
                  title={entry.name}
                  onclick={() => open(entry)}
                  ><span class:folder-color={entry.is_dir}
                    ><Icon
                      name={entry.is_dir ? "folder" : "file"}
                      size={19}
                    /></span
                  ><span>{entry.name}</span>{#if entry.is_dir}<Icon
                      name="chevron"
                      size={14}
                    />{/if}</button
                ></div></td
              ><td class="font-path"
                >{entry.is_dir ? "—" : formatSize(entry.size)}</td
              ><td class="modified-column">{formatTimestamp(entry.modified)}</td
              ></tr
            >{/each}</tbody
        >
      </table>{/if}
  </div>
  <footer class="browser-footer">
    <span
      >{filtered.length.toLocaleString()}
      {query ? "matches in this folder" : "items"}</span
    >{#if pages > 1}<div class="pagination">
        <button
          class="btn btn-sm"
          type="button"
          disabled={page === 1}
          onclick={() => page--}>Previous</button
        ><span>{page} / {pages}</span><button
          class="btn btn-sm"
          type="button"
          disabled={page >= pages}
          onclick={() => page++}>Next</button
        >
      </div>{/if}
  </footer>
</div>

<UiDialog open={showCreate} title="New folder" description={`Create a folder inside ${currentPath}.`}
  closeDisabled={editing} onClose={() => showCreate = false}>
  <form id="create-archive-folder" onsubmit={(event) => { event.preventDefault(); void createFolder(); }}>
    <label class="block text-sm mb-1" for="archive-folder-name">Folder name</label>
    <input id="archive-folder-name" class="input input-bordered w-full" bind:value={folderName} disabled={editing} autocomplete="off" />
    {#if editError}<p class="notice notice-error mt-3 break-all" role="alert">{editError}</p>{/if}
  </form>
  {#snippet actions()}
    <button class="btn" type="button" disabled={editing} onclick={() => showCreate = false}>Cancel</button>
    <button class="btn btn-primary" type="submit" form="create-archive-folder" disabled={editsDisabled || !validFolderName}>{editing ? "Creating…" : "Create folder"}</button>
  {/snippet}
</UiDialog>
{#if showMove}
  <MoveEntriesDialog sources={checked.map(name => `${currentPath === "/" ? "" : currentPath}/${name}`)} initialPath={currentPath}
    onClose={() => showMove = false} onCreated={() => refresh++}
    onMoved={() => { showMove = false; checked = []; refresh++; onChanged?.(); }} />
{/if}

<style>
  .organize-tools { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; padding: 0 20px 12px; }
  .name-with-selection { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .name-with-selection > input { flex-shrink: 0; }
  .name-with-selection > button { min-width: 0; }
</style>
