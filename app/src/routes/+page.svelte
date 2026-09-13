<script lang="ts">
  import UiSelect from "$lib/components/ui/UiSelect.svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import FileTree from "$lib/components/FileTree.svelte";
  import FileDetails from "$lib/components/FileDetails.svelte";
  import DuplicatesPage from "$lib/components/DuplicatesPage.svelte";
  import ScanActivity from "$lib/components/scan/ScanActivity.svelte";
  import StatsPage from "$lib/components/StatsPage.svelte";
  import AppShell, { type View } from "$lib/components/AppShell.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import WorkspaceManagerDialog from "$lib/components/workspace/WorkspaceManagerDialog.svelte";
  import {
    listWorkspaces,
    createWorkspace,
    switchWorkspace,
    deleteWorkspace,
    exportWorkspaces,
    importWorkspaces,
    importWorkspace,
    pickDirectory,
    pickFile,
    type DirEntry,
    type Workspace,
  } from "$lib/api/tauri";
  import { app } from "$lib/state/app.svelte";
  let selectedPath = $state<string | null>(null);
  let selectedEntry = $state<DirEntry | null>(null);
  let browserPath = $state("/");
  let currentView = $state<View>("files");
  let showWorkspaceDialog = $state(false);
  let workspaceDialogMode = $state<"list" | "create" | "import">("list");
  let newWsLabel = $state("");
  let newWsTags = $state("");
  let newWsStorePath = $state("");
  let importWsStorePath = $state("");
  let importWsLabel = $state("");
  let wsError = $state<string | null>(null);
  let workspaceBusy = $state(false);
  let removedWorkspace = $state<Workspace | null>(null);
  const emptyArchive = $derived(
    !(
      app.activeWorkspace?.stats.total_files ||
      app.activeWorkspace?.stats.scans_count
    ),
  );
  const contentKey = $derived(
    `${app.workspacesConfig.active_workspace_id}:${app.treeRefreshKey}`,
  );
  $effect(() => {
    const requested = page.url.searchParams.get("view");
    if (
      requested === "files" ||
      requested === "duplicates" ||
      requested === "activity" ||
      requested === "stats"
    )
      currentView = requested;
  });
  $effect(() => {
    if (app.scanning) currentView = "activity";
  });
  async function workspaceTask(operation: () => Promise<void>) {
    if (workspaceBusy || app.scanning) return;
    workspaceBusy = true;
    wsError = null;
    try {
      await operation();
    } catch (e) {
      wsError = String(e);
    } finally {
      workspaceBusy = false;
    }
  }
  async function activate(id: string) {
    await switchWorkspace(id);
    app.workspacesConfig = await listWorkspaces();
    selectedPath = null;
    selectedEntry = null;
    browserPath = "/";
    app.treeRefreshKey++;
  }
  function openWorkspaceManager() {
    if (workspaceBusy || app.scanning) return;
    wsError = null;
    workspaceDialogMode = "list";
    showWorkspaceDialog = true;
  }
  function openCreateWorkspace() {
    newWsLabel = "";
    newWsTags = "";
    newWsStorePath = "";
    wsError = null;
    workspaceDialogMode = "create";
    showWorkspaceDialog = true;
  }
  function openImportWorkspace() {
    importWsStorePath = "";
    importWsLabel = "";
    wsError = null;
    workspaceDialogMode = "import";
    showWorkspaceDialog = true;
  }
  async function handleCreateWorkspace() {
    if (!newWsLabel.trim() || !newWsStorePath.trim()) return;
    await workspaceTask(async () => {
      const ws = await createWorkspace(
        newWsLabel.trim(),
        newWsTags
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        newWsStorePath.trim(),
      );
      await activate(ws.id);
      showWorkspaceDialog = false;
      goToScan();
    });
  }
  async function handleSwitchWorkspace(id: string) {
    await workspaceTask(async () => {
      await activate(id);
      showWorkspaceDialog = false;
    });
  }
  async function handleDeleteWorkspace(id: string) {
    await workspaceTask(async () => {
      const removed = app.workspacesConfig.workspaces.find((w) => w.id === id);
      await deleteWorkspace(id);
      app.workspacesConfig = await listWorkspaces();
      removedWorkspace = removed ?? null;
      if (app.workspacesConfig.active_workspace_id)
        await activate(app.workspacesConfig.active_workspace_id);
      else {
        selectedPath = null;
        selectedEntry = null;
        app.treeRefreshKey++;
      }
    });
  }
  async function undoRemoval() {
    const removed = removedWorkspace;
    if (!removed) return;
    await workspaceTask(async () => {
      await importWorkspaces(
        JSON.stringify({
          workspaces: [removed],
          active_workspace_id: null,
          custom_scan_rules: [],
        }),
      );
      await activate(removed.id);
      removedWorkspace = null;
    });
  }
  async function handleExportWorkspaces() {
    await workspaceTask(async () => {
      const json = await exportWorkspaces();
      const url = URL.createObjectURL(
        new Blob([json], { type: "application/json" }),
      );
      const a = document.createElement("a");
      a.href = url;
      a.download = "dedup-archives.json";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }
  function handleImportWorkspaces() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = () => {
      const file = input.files?.[0];
      if (file)
        void workspaceTask(async () => {
          app.workspacesConfig = await importWorkspaces(await file.text());
          app.syncCustomScanRulesFromConfig(
            app.workspacesConfig.custom_scan_rules,
          );
          const active =
            app.workspacesConfig.active_workspace_id ??
            app.workspacesConfig.workspaces[0]?.id;
          if (active) await activate(active);
        });
    };
    input.click();
  }
  async function handleImportWorkspace() {
    if (!importWsStorePath.trim() || !importWsLabel.trim()) return;
    await workspaceTask(async () => {
      const ws = await importWorkspace(
        importWsStorePath.trim(),
        importWsLabel.trim(),
      );
      await activate(ws.id);
      showWorkspaceDialog = false;
    });
  }
  async function browseNewStore() {
    await workspaceTask(async () => {
      const dir = await pickDirectory("Choose where to save the archive");
      if (dir) {
        newWsStorePath = dir;
        if (!newWsLabel)
          newWsLabel = dir.split("/").filter(Boolean).pop() ?? "My archive";
      }
    });
  }
  async function browseImportStoreFolder() {
    await workspaceTask(async () => {
      const dir = await pickDirectory("Open an existing archive folder");
      if (dir) {
        importWsStorePath = dir;
        if (!importWsLabel)
          importWsLabel = dir.split("/").filter(Boolean).pop() ?? "My archive";
      }
    });
  }
  async function browseImportStoreFile() {
    await workspaceTask(async () => {
      const file = await pickFile("Open archive metadata", [
        { name: "Archive database", extensions: ["redb"] },
      ]);
      if (file) {
        importWsStorePath = file;
        if (!importWsLabel)
          importWsLabel = file.split("/").slice(-2, -1)[0] ?? "My archive";
      }
    });
  }
  function handleWorkspaceModeChange(mode: "list" | "create" | "import") {
    if (mode === "create") openCreateWorkspace();
    else if (mode === "import") openImportWorkspace();
    else workspaceDialogMode = mode;
  }
  function handleSelect(path: string, entry: DirEntry) {
    selectedPath = path;
    selectedEntry = entry;
  }
  function openArchivedPath(path: string) {
    browserPath = path.slice(0, path.lastIndexOf("/")) || "/";
    selectedPath = path;
    selectedEntry = {
      name: path.split("/").pop() ?? path,
      is_dir: false,
      size: 0,
      modified: 0,
    };
    currentView = "files";
  }
  function goToScan(target?: string) {
    if (app.scanning) return;
    app.prepareScan(target);
    void goto("/scan");
  }
</script>

<AppShell
  {currentView}
  hasWorkspace={app.hasWorkspace}
  scanning={app.scanning}
  onViewChange={(view) => (currentView = view)}
  onScan={() => goToScan()}
>
  {#snippet workspaceControl()}
    <div class="flex items-center gap-1">
      <UiSelect
        class="archive-switch"
        label="Current archive"
        disabled={workspaceBusy || app.scanning || !app.hasWorkspace}
        value={app.workspacesConfig.active_workspace_id ?? ""}
        onValueChange={handleSwitchWorkspace}
        placeholder="No archive yet"
        options={app.workspacesConfig.workspaces.map((ws) => ({
          value: ws.id,
          label: ws.label,
        }))}
      /><button
        class="icon-button"
        type="button"
        aria-label="Manage archives"
        disabled={workspaceBusy || app.scanning}
        onclick={openWorkspaceManager}
        ><Icon name="settings" size={17} /></button
      >
    </div>
  {/snippet}
  <WorkspaceManagerDialog
    open={showWorkspaceDialog}
    config={app.workspacesConfig}
    mode={workspaceDialogMode}
    error={wsError}
    importing={workspaceBusy}
    busy={workspaceBusy}
    removedLabel={removedWorkspace?.label}
    onUndo={undoRemoval}
    newLabel={newWsLabel}
    newTags={newWsTags}
    newStorePath={newWsStorePath}
    importLabel={importWsLabel}
    importStorePath={importWsStorePath}
    onClose={() => {
      if (!workspaceBusy) showWorkspaceDialog = false;
    }}
    onModeChange={handleWorkspaceModeChange}
    onSwitch={handleSwitchWorkspace}
    onDelete={handleDeleteWorkspace}
    onExport={handleExportWorkspaces}
    onImportConfig={handleImportWorkspaces}
    onCreate={handleCreateWorkspace}
    onImportStore={handleImportWorkspace}
    onNewLabelChange={(value) => (newWsLabel = value)}
    onNewTagsChange={(value) => (newWsTags = value)}
    onNewStorePathChange={(value) => (newWsStorePath = value)}
    onImportLabelChange={(value) => (importWsLabel = value)}
    onImportStorePathChange={(value) => (importWsStorePath = value)}
    onBrowseNewStore={browseNewStore}
    onBrowseImportStoreFolder={browseImportStoreFolder}
    onBrowseImportStoreFile={browseImportStoreFile}
  />
  {#if app.workspaceError}<div class="notice notice-error m-4" role="alert">
      <p>Could not refresh archives: {app.workspaceError}</p>
      <button class="btn btn-sm" type="button" onclick={app.loadWorkspaces}
        >Try again</button
      >
    </div>{/if}
  {#if wsError && !showWorkspaceDialog}<div
      class="notice notice-error m-4"
      role="alert"
    >
      {wsError}
    </div>{/if}
  {#if removedWorkspace && !showWorkspaceDialog}<div
      class="notice m-4"
      role="status"
    >
      <span
        >“{removedWorkspace.label}” removed from the list. Its files remain on
        disk.</span
      ><button
        class="btn btn-sm"
        type="button"
        disabled={workspaceBusy}
        onclick={undoRemoval}>Undo</button
      >
    </div>{/if}
  {#if workspaceBusy && !showWorkspaceDialog}<div
      class="empty-panel"
      role="status"
    >
      <span class="loading loading-spinner"></span>
      <p>Opening archive…</p>
    </div>
  {:else if app.workspacesLoading && !app.hasWorkspace}<div
      class="empty-panel"
      role="status"
    >
      Loading archives…
    </div>
  {:else if !app.hasWorkspace}<div class="welcome">
      <div class="welcome-mark"><Icon name="archive" size={38} /></div>
      <span class="eyebrow">LESS DUPLICATION. MORE CLARITY.</span>
      <h2 class="page-title">A home for your files.</h2>
      <p class="muted">
        Keep your files in one archive. Identical content is stored once, and
        your originals stay untouched.
      </p>
      <div class="archive-actions">
        <button
          class="btn btn-primary"
          type="button"
          onclick={openCreateWorkspace}
          ><Icon name="plus" size={18} />Create an archive</button
        ><button class="btn" type="button" onclick={openImportWorkspace}
          >Open existing archive</button
        >
      </div>
    </div>
  {:else if app.scanning || currentView === "activity"}<ScanActivity
      onFiles={() => (currentView = "files")}
      onDuplicates={() => (currentView = "duplicates")}
      onNew={() => goToScan()}
    />
  {:else}
    {#key contentKey}
      {#if currentView === "duplicates"}<DuplicatesPage
          {emptyArchive}
          onOpen={openArchivedPath}
        />
      {:else if currentView === "stats"}<StatsPage
          {emptyArchive}
          archiveName={app.activeWorkspace?.label}
        />
      {:else}<div
          class="files-layout"
          class:has-selection={selectedPath !== null}
        >
          <section class="files-pane" aria-label="Archive files">
            <FileTree
              {selectedPath}
              onSelect={handleSelect}
              onScanInto={goToScan}
              initialPath={browserPath}
              onNavigate={(path) => {
                browserPath = path;
                selectedPath = null;
                selectedEntry = null;
              }}
              expectContent={!emptyArchive}
            />
          </section>
          {#if selectedPath && selectedEntry}<section
              class="details-pane"
              aria-label="Selected file"
            >
              <FileDetails
                path={selectedPath}
                entry={selectedEntry}
                onClose={() => {
                  selectedPath = null;
                  selectedEntry = null;
                }}
                onOpenPath={openArchivedPath}
              />
            </section>{/if}
        </div>{/if}
    {/key}
  {/if}
</AppShell>
