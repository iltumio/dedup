<script lang="ts">
  import { findAllDuplicates } from "$lib/api/tauri";
  import Icon from "./ui/Icon.svelte";
  let {
    onOpen,
    emptyArchive = false,
  }: { onOpen: (path: string) => void; emptyArchive?: boolean } = $props();
  let groups = $state<[string, string[]][]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let query = $state("");
  let page = $state(1);
  let refresh = $state(0);
  let expanded = $state<Record<string, number>>({});
  const filtered = $derived(
    groups.filter(([, paths]) =>
      paths.some((p) =>
        p.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      ),
    ),
  );
  const pages = $derived(Math.max(1, Math.ceil(filtered.length / 20)));
  const visible = $derived(filtered.slice((page - 1) * 20, page * 20));
  $effect(() => {
    refresh;
    let active = true;
    loading = true;
    error = null;
    findAllDuplicates()
      .then((data) => {
        if (active) groups = data.sort((a, b) => b[1].length - a[1].length);
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
  $effect(() => {
    if (page > pages) page = pages;
  });
  $effect(() => {
    query;
    page = 1;
  });
</script>

<div class="duplicates-page">
  <div class="view-intro">
    <div>
      <h2>Every copy, together.</h2>
      <p>
        Identical content is stored once. These paths refer to the same archived
        content.
      </p>
    </div>
    <button
      class="icon-button"
      type="button"
      aria-label="Refresh duplicates"
      disabled={loading}
      onclick={() => refresh++}><Icon name="refresh" /></button
    >
  </div>
  <label class="search-box"
    ><Icon name="search" size={18} /><input
      aria-label="Search duplicate paths"
      placeholder="Find a name or path…"
      bind:value={query}
    /></label
  >
  <div class="duplicates-scroll" aria-busy={loading}>
    {#if loading}<div class="empty-panel" role="status">
        <span class="loading loading-spinner"></span>
        <p>Finding duplicate groups…</p>
      </div>{:else if error}<div class="empty-panel">
        <h2>Duplicates unavailable</h2>
        <p>{error}</p>
        <button class="btn" type="button" onclick={() => refresh++}
          >Try again</button
        >
      </div>{:else if !filtered.length}<div class="empty-panel">
        <Icon name="copies" size={36} />
        <h2>{query ? "No matching groups" : "No duplicate groups"}</h2>
        <p>
          {query
            ? "Try another name or path."
            : "Each archived file has unique content, or the archive is still empty."}
        </p>
      </div>{:else}
      {#each visible as [cid, paths] (cid)}<article class="duplicate-group">
          <header>
            <span class="group-icon"><Icon name="copies" /></span>
            <div class="min-w-0">
              <h3 title={paths[0]}>{paths[0].split("/").pop()}</h3>
              <span class="muted"
                >{paths.length.toLocaleString()} paths · 1 stored content</span
              >
            </div>
            <span class="count-badge">{paths.length} copies</span>
          </header>
          <ul>
            {#each paths.slice(0, expanded[cid] ?? 5) as path}<li>
                <button
                  class="path-link"
                  type="button"
                  onclick={() => onOpen(path)}
                  ><Icon name="file" size={16} /><span>{path}</span><Icon
                    name="chevron"
                    size={14}
                  /></button
                >
              </li>{/each}
          </ul>
          {#if paths.length > (expanded[cid] ?? 5)}<button
              class="btn btn-ghost btn-sm"
              type="button"
              onclick={() => (expanded[cid] = (expanded[cid] ?? 5) + 100)}
              >Show more paths ({paths.length - (expanded[cid] ?? 5)} remaining)</button
            >{/if}
        </article>{/each}
    {/if}
  </div>
  <footer class="browser-footer">
    <span>{filtered.length.toLocaleString()} content groups</span
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
