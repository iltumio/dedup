<script lang="ts">
  import UiSelect from "./ui/UiSelect.svelte";
  import {
    listDir,
    formatSize,
    formatTimestamp,
    type DirEntry,
  } from "$lib/api/tauri";
  import Icon from "./ui/Icon.svelte";
  let {
    selectedPath,
    onSelect,
    onScanInto,
    initialPath = "/",
    expectContent = false,
    onNavigate,
  }: {
    selectedPath: string | null;
    onSelect: (path: string, entry: DirEntry) => void;
    onScanInto: (path: string) => void;
    initialPath?: string;
    expectContent?: boolean;
    onNavigate?: (path: string) => void;
  } = $props();
  let currentPath = $state("/");
  let entries = $state<DirEntry[]>([]);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let query = $state("");
  let sort = $state("name");
  let page = $state(1);
  let refresh = $state(0);
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
        if (active) entries = result;
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
    query = "";
    onNavigate?.(path);
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
            onclick={() => onScanInto(currentPath)}>Add a folder here</button
          >{/if}
      </div>
    {:else}<table class="file-table">
        <thead
          ><tr
            ><th scope="col">Name</th><th scope="col">Size</th><th
              scope="col"
              class="modified-column">Modified</th
            ></tr
          ></thead
        ><tbody
          >{#each visible as entry (entry.name)}{@const fullPath = `${currentPath === "/" ? "" : currentPath}/${entry.name}`}<tr
              class:selected={selectedPath === fullPath}
              aria-selected={selectedPath === fullPath}
              ><td
                ><button
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
                ></td
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
      </div>{:else}<button
        class="btn btn-ghost btn-sm"
        type="button"
        onclick={() => onScanInto(currentPath)}>Add here</button
      >{/if}
  </footer>
</div>
