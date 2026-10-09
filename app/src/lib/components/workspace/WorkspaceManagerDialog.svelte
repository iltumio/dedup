<script lang="ts">
  import type { StorageFormat, WorkspacesConfig } from "$lib/api/tauri";
  import { UiButton, UiDialog, UiEmptyState } from "$lib/components/ui";
  import WorkspaceForm from "./WorkspaceForm.svelte";
  import WorkspaceListItem from "./WorkspaceListItem.svelte";

  type Mode = "list" | "create" | "import";

  interface Props {
    open: boolean;
    config: WorkspacesConfig;
    mode: Mode;
    error: string | null;
    importing: boolean;
    busy?: boolean;
    removedLabel?: string;
    onUndo?: () => void;
    newLabel: string;
    newTags: string;
    newStorePath: string;
    newFormat: StorageFormat;
    importLabel: string;
    importStorePath: string;
    onClose: () => void;
    onModeChange: (mode: Mode) => void;
    onSwitch: (id: string) => void;
    onDelete: (id: string) => void;
    onMigrate: (id: string) => void;
    onExport: () => void;
    onImportConfig: () => void;
    onCreate: () => void;
    onImportStore: () => void;
    onNewLabelChange: (value: string) => void;
    onNewTagsChange: (value: string) => void;
    onNewStorePathChange: (value: string) => void;
    onNewFormatChange: (value: StorageFormat) => void;
    onImportLabelChange: (value: string) => void;
    onImportStorePathChange: (value: string) => void;
    onBrowseNewStore: () => void;
    onBrowseImportStoreFolder: () => void;
    onBrowseImportStoreFile: () => void;
  }

  let {
    open,
    config,
    mode,
    error,
    importing,
    busy = false,
    removedLabel,
    onUndo,
    newLabel,
    newTags,
    newStorePath,
    newFormat,
    importLabel,
    importStorePath,
    onClose,
    onModeChange,
    onSwitch,
    onDelete,
    onMigrate,
    onExport,
    onImportConfig,
    onCreate,
    onImportStore,
    onNewLabelChange,
    onNewTagsChange,
    onNewStorePathChange,
    onNewFormatChange,
    onImportLabelChange,
    onImportStorePathChange,
    onBrowseNewStore,
    onBrowseImportStoreFolder,
    onBrowseImportStoreFile,
  }: Props = $props();
</script>

<UiDialog {open} title="Your archives" wide {onClose} closeDisabled={busy}>
  {#if busy}<p class="notice mb-4" role="status">
      Working on your archive…
    </p>{/if}
  <fieldset disabled={busy} class="min-w-0">
    {#if mode === "create"}
      <WorkspaceForm
        mode="create"
        label={newLabel}
        tags={newTags}
        storePath={newStorePath}
        format={newFormat}
        loading={busy}
        {error}
        onBack={() => onModeChange("list")}
        onSubmit={onCreate}
        onLabelChange={onNewLabelChange}
        onTagsChange={onNewTagsChange}
        onStorePathChange={onNewStorePathChange}
        onFormatChange={onNewFormatChange}
        onBrowseFolder={onBrowseNewStore}
      />
    {:else if mode === "import"}
      <WorkspaceForm
        mode="import"
        label={importLabel}
        storePath={importStorePath}
        loading={importing}
        {error}
        onBack={() => onModeChange("list")}
        onSubmit={onImportStore}
        onLabelChange={onImportLabelChange}
        onStorePathChange={onImportStorePathChange}
        onBrowseFolder={onBrowseImportStoreFolder}
        onBrowseFile={onBrowseImportStoreFile}
      />
    {:else}
      <div class="flex flex-col gap-4">
        {#if config.workspaces.length === 0}
          <UiEmptyState
            title="No archives yet"
            message="Create an archive or open one you already have."
          />
        {:else}
          <ul class="flex list-none flex-col gap-2 p-0">
            {#each config.workspaces as workspace (workspace.id)}
              <WorkspaceListItem
                {workspace}
                active={workspace.id === config.active_workspace_id}
                onSelect={onSwitch}
                {onDelete}
                {onMigrate}
                pendingMigration={config.pending_migrations?.some(p => p.workspace_id === workspace.id) ?? false}
              />
            {/each}
          </ul>
        {/if}

        {#if error}
          <div class="alert alert-error py-2 text-sm" role="alert">
            <span>{error}</span>
          </div>
        {/if}

        {#if removedLabel}<div class="notice" role="status">
            <span>“{removedLabel}” removed. Its files remain on disk.</span
            ><UiButton onclick={onUndo}>Undo</UiButton>
          </div>{/if}
        <div class="archive-actions">
          <details class="disclosure">
            <summary>Configuration</summary>
            <div class="detail-content">
              <p class="muted text-sm mb-3">
                Transfer archive locations and rules. Archive contents are not
                included.
              </p>
              <div class="flex gap-2">
                <UiButton onclick={onExport}>Export settings</UiButton><UiButton
                  onclick={onImportConfig}>Import settings</UiButton
                >
              </div>
            </div>
          </details>
          <div class="grow"></div>
          <UiButton onclick={() => onModeChange("import")}
            >Open existing</UiButton
          ><UiButton variant="primary" onclick={() => onModeChange("create")}
            >Create archive</UiButton
          >
        </div>
      </div>
    {/if}
  </fieldset>
</UiDialog>
