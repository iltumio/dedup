<script lang="ts">
  import {
    getFileMetadata,
    findDuplicates,
    readFile,
    openFile,
    canOpenFile,
    formatSize,
    formatTimestamp,
    type FileMetadata,
    type DirEntry,
  } from "$lib/api/tauri";
  import Icon from "./ui/Icon.svelte";
  let {
    path,
    entry,
    onClose,
    onOpenPath,
  }: {
    path: string;
    entry: DirEntry;
    onClose?: () => void;
    onOpenPath?: (path: string) => void;
  } = $props();
  let metadata = $state<FileMetadata | null>(null);
  let duplicates = $state<string[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let previewUrl = $state<string | null>(null);
  let openSupported = $state<boolean | null>(null);
  let checkingOpen = $state(true);
  let opening = $state(false);
  let actionMessage = $state("");
  let retry = $state(0);
  let pathLimit = $state(20);
  const imageTypes: Record<string, string> = {
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    gif: "image/gif",
    webp: "image/webp",
    bmp: "image/bmp",
    avif: "image/avif",
  };
  const cid = $derived(
    metadata?.cid.map((b) => b.toString(16).padStart(2, "0")).join("") ?? "",
  );
  $effect(() => {
    const selected = path;
    const isDir = entry.is_dir;
    retry;
    let active = true;
    let objectUrl: string | null = null;
    metadata = null;
    duplicates = [];
    previewUrl = null;
    error = null;
    previewError = null;
    actionMessage = "";
    pathLimit = 20;
    loading = true;
    (async () => {
      try {
        if (isDir) return;
        const [meta, copies] = await Promise.all([
          getFileMetadata(selected),
          findDuplicates(selected),
        ]);
        if (!active) return;
        metadata = meta;
        duplicates = copies;
        if (!meta) {
          error = "This file is no longer available in the archive.";
          return;
        }
        const mime = imageTypes[selected.split(".").pop()?.toLowerCase() ?? ""];
        if (mime) {
          if (meta.original_size > 12 * 1024 * 1024)
            previewError =
              "Preview skipped for this large image.";
          else
            try {
              const bytes = await readFile(selected);
              if (!active) return;
              objectUrl = URL.createObjectURL(
                new Blob([new Uint8Array(bytes)], { type: mime }),
              );
              previewUrl = objectUrl;
            } catch {
              if (active)
                previewError =
                  "Preview unavailable.";
            }
        }
      } catch (e) {
        if (active) error = String(e);
      } finally {
        if (active) loading = false;
      }
    })();
    return () => {
      active = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  });
  $effect(() => {
    const selected = path;
    const isDir = entry.is_dir;
    retry;
    let active = true;
    openSupported = null;
    checkingOpen = true;
    if (!isDir) {
      canOpenFile(selected)
        .then((supported) => {
          if (active) openSupported = supported;
        })
        .catch(() => {
          /* An unavailable check does not mean unsupported. */
        })
        .finally(() => {
          if (active) checkingOpen = false;
        });
    }
    return () => {
      active = false;
    };
  });
  async function handleOpen() {
    if (opening || checkingOpen || openSupported === false) return;
    const selected = path;
    opening = true;
    actionMessage = "";
    try {
      await openFile(selected);
    } catch (e) {
      if (path === selected) actionMessage = `Could not open file: ${e}`;
    } finally {
      opening = false;
    }
  }
  async function copy(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      actionMessage = "Copied to clipboard";
    } catch {
      actionMessage =
        "Could not access the clipboard. Select the text to copy it manually.";
    }
  }
</script>

<div class="file-inspector">
  <header>
    <span class="eyebrow">FILE DETAILS</span>{#if onClose}<button
        class="icon-button"
        type="button"
        aria-label="Back to file list"
        onclick={onClose}><Icon name="close" size={18} /></button
      >{/if}
  </header>
  <div class="inspector-scroll">
    <div class="inspector-file-icon">
      <Icon name={entry.is_dir ? "folder" : "file"} size={32} />
    </div>
    <h2 class="inspector-name">{entry.name}</h2>
    <button
      class="inspector-path font-path"
      type="button"
      title="Copy archive path"
      onclick={() => copy(path)}>{path}</button
    >
    {#if !entry.is_dir && !checkingOpen && openSupported !== false}<button
        class="btn btn-primary mt-4"
        type="button"
        disabled={opening || loading}
        onclick={handleOpen}
        >{opening ? "Opening…" : "Open file"}<Icon
          name="chevron"
          size={16}
        /></button
      >{/if}
    {#if !entry.is_dir && checkingOpen}<p
        class="muted text-sm mt-3"
        role="status"
      >
        Checking available applications…
      </p>
    {:else if !entry.is_dir && openSupported === false}<p
        class="muted text-sm mt-3"
      >
        No default application is associated with this file type.
      </p>{/if}
    {#if actionMessage}<p class="notice mt-3" role="status">
        {actionMessage}
      </p>{/if}
    {#if error}<div class="notice notice-error mt-4" role="alert">
        <p>{error}</p>
        <button class="btn btn-sm" type="button" onclick={() => retry++}
          >Try again</button
        >
      </div>
    {:else if loading}<div class="py-8 muted" role="status">
        Loading file details…
      </div>
    {:else if metadata}
      {#if previewUrl}<div class="image-preview">
          <img
            src={previewUrl}
            alt={entry.name}
            onerror={() => {
              previewError = "This image format could not be displayed.";
              previewUrl = null;
            }}
          />
        </div>{/if}
      {#if previewError}<p class="notice mt-4">{previewError}</p>{/if}
      <dl class="metadata-list">
        <div>
          <dt>Original size</dt>
          <dd>{formatSize(metadata.original_size)}</dd>
        </div>
        <div>
          <dt>Stored content</dt>
          <dd>{formatSize(metadata.compressed_size)}</dd>
        </div>
        <div>
          <dt>Modified</dt>
          <dd>{formatTimestamp(metadata.modified)}</dd>
        </div>
      </dl>
      {#if duplicates.length > 1}<section class="inspector-copies">
          <h3>
            <Icon name="copies" size={18} />{duplicates.length} archived paths
          </h3>
          <p class="muted text-sm">These files share one stored content.</p>
          <ul>
            {#each duplicates.slice(0, pathLimit) as duplicate}<li>
                <button
                  class="path-link"
                  type="button"
                  disabled={duplicate === path || !onOpenPath}
                  onclick={() => onOpenPath?.(duplicate)}
                  ><span>{duplicate}</span>{#if duplicate === path}<span
                      class="count-badge">Current</span
                    >{:else}<Icon name="chevron" size={14} />{/if}</button
                >
              </li>{/each}
          </ul>
          {#if duplicates.length > pathLimit}<button
              class="btn btn-sm"
              type="button"
              onclick={() => (pathLimit += 100)}>Show more paths</button
            >{/if}
        </section>{/if}
      <details class="disclosure mt-5">
        <summary>Technical details</summary>
        <div class="detail-content">
          <p class="eyebrow">CONTENT IDENTIFIER (HEX)</p>
          <p class="font-path break-all text-xs my-2">{cid}</p>
          <button class="btn btn-sm" type="button" onclick={() => copy(cid)}
            >Copy identifier</button
          >
          <p class="muted text-sm mt-3">
            Created: {formatTimestamp(metadata.created)}
          </p>
        </div>
      </details>
    {/if}
  </div>
</div>
