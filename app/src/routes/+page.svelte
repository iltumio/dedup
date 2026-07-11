<script lang="ts">
	import { goto } from '$app/navigation';
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import StatsPage from '$lib/components/StatsPage.svelte';
	import AppShell from '$lib/components/AppShell.svelte';
	import { UiButton, UiEmptyState } from '$lib/components/ui';
	import WorkspaceManagerDialog from '$lib/components/workspace/WorkspaceManagerDialog.svelte';
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
		type DirEntry
	} from '$lib/api/tauri';
	import { app } from '$lib/state/app.svelte';

	// ── File browser state ──
	let selectedPath = $state<string | null>(null);
	let selectedEntry = $state<DirEntry | null>(null);
	let currentView = $state<'files' | 'stats'>('files');

	// ── Workspace dialog state ──
	let showWorkspaceDialog = $state(false);
	let workspaceDialogMode = $state<'list' | 'create' | 'import'>('list');
	let newWsLabel = $state('');
	let newWsTags = $state('');
	let newWsStorePath = $state('');
	let wsError = $state<string | null>(null);
	let importWsStorePath = $state('');
	let importWsLabel = $state('');
	let importingWs = $state(false);

	// ── Workspace actions ──
	function openWorkspaceManager() {
		wsError = null;
		workspaceDialogMode = 'list';
		showWorkspaceDialog = true;
	}

	function openCreateWorkspace() {
		newWsLabel = '';
		newWsTags = '';
		newWsStorePath = '';
		wsError = null;
		workspaceDialogMode = 'create';
	}

	async function handleCreateWorkspace() {
		if (!newWsLabel.trim() || !newWsStorePath.trim()) return;
		wsError = null;
		try {
			const tags = newWsTags
				.split(',')
				.map((t) => t.trim())
				.filter(Boolean);
			await createWorkspace(newWsLabel, tags, newWsStorePath);
			app.workspacesConfig = await listWorkspaces();
			workspaceDialogMode = 'list';
		} catch (e) {
			wsError = String(e);
		}
	}

	async function handleSwitchWorkspace(id: string) {
		wsError = null;
		try {
			await switchWorkspace(id);
			app.workspacesConfig = await listWorkspaces();
			selectedPath = null;
			selectedEntry = null;
			app.scanResult = null;
			app.treeRefreshKey++;
			showWorkspaceDialog = false;
		} catch (e) {
			wsError = String(e);
		}
	}

	async function handleDeleteWorkspace(id: string) {
		wsError = null;
		try {
			await deleteWorkspace(id);
			app.workspacesConfig = await listWorkspaces();
			if (app.workspacesConfig.active_workspace_id) {
				app.treeRefreshKey++;
			}
		} catch (e) {
			wsError = String(e);
		}
	}

	async function handleExportWorkspaces() {
		try {
			const json = await exportWorkspaces();
			const blob = new Blob([json], { type: 'application/json' });
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = 'dedup-workspaces.json';
			a.click();
			URL.revokeObjectURL(url);
		} catch (e) {
			wsError = String(e);
		}
	}

	async function handleImportWorkspaces() {
		try {
			const input = document.createElement('input');
			input.type = 'file';
			input.accept = '.json';
			input.onchange = async () => {
				const file = input.files?.[0];
				if (!file) return;
				wsError = null;
				try {
					const json = await file.text();
					app.workspacesConfig = await importWorkspaces(json);
					app.syncCustomScanRulesFromConfig(app.workspacesConfig.custom_scan_rules);
					app.treeRefreshKey++;
				} catch (e) {
					wsError = String(e);
				}
			};
			input.click();
		} catch (e) {
			wsError = String(e);
		}
	}

	function openImportWorkspace() {
		importWsStorePath = '';
		importWsLabel = '';
		wsError = null;
		workspaceDialogMode = 'import';
	}

	async function handleImportWorkspace() {
		if (!importWsStorePath.trim() || !importWsLabel.trim()) return;
		wsError = null;
		importingWs = true;
		try {
			await importWorkspace(importWsStorePath, importWsLabel);
			app.workspacesConfig = await listWorkspaces();
			workspaceDialogMode = 'list';
			app.treeRefreshKey++;
		} catch (e) {
			wsError = String(e);
		} finally {
			importingWs = false;
		}
	}

	async function browseNewStore() {
		try {
			const dir = await pickDirectory('Select workspace store directory');
			if (dir) newWsStorePath = dir;
		} catch (e) {
			wsError = String(e);
		}
	}

	async function browseImportStoreFolder() {
		try {
			const dir = await pickDirectory('Select .store directory');
			if (dir) importWsStorePath = dir;
		} catch (e) {
			wsError = String(e);
		}
	}

	async function browseImportStoreFile() {
		try {
			const file = await pickFile('Select metadata.redb', [
				{ name: 'redb metadata', extensions: ['redb'] }
			]);
			if (file) importWsStorePath = file;
		} catch (e) {
			wsError = String(e);
		}
	}

	function handleWorkspaceModeChange(mode: 'list' | 'create' | 'import') {
		if (mode === 'create') {
			openCreateWorkspace();
			return;
		}

		if (mode === 'import') {
			openImportWorkspace();
			return;
		}

		workspaceDialogMode = 'list';
	}

	// ── File browser actions ──
	function handleSelect(path: string, entry: DirEntry) {
		selectedPath = path;
		selectedEntry = entry;
	}

	function goToScan(presetTarget?: string) {
		app.prepareScan(presetTarget);
		goto('/scan');
	}
</script>

<AppShell
	{currentView}
	hasWorkspace={app.hasWorkspace}
	scanning={app.scanning}
	stats={app.shellStats}
	onViewChange={(view) => (currentView = view)}
	onScan={() => goToScan()}
>
	{#snippet workspaceControl()}
		<button class="btn btn-neutral btn-sm min-w-0 max-w-72 w-full justify-start" type="button" onclick={openWorkspaceManager}>
			{#if app.activeWorkspace}
				<span class="truncate">{app.activeWorkspace.label}</span>
			{:else}
				<span class="text-base-content/50">No workspace</span>
			{/if}
		</button>
	{/snippet}

	<WorkspaceManagerDialog
		open={showWorkspaceDialog}
		config={app.workspacesConfig}
		mode={workspaceDialogMode}
		error={wsError}
		importing={importingWs}
		newLabel={newWsLabel}
		newTags={newWsTags}
		newStorePath={newWsStorePath}
		importLabel={importWsLabel}
		importStorePath={importWsStorePath}
		onClose={() => (showWorkspaceDialog = false)}
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

	<div class="min-h-0 flex-1 overflow-hidden">
		{#if !app.hasWorkspace}
			<UiEmptyState title="Welcome to dedup" message="Create or import a workspace to start scanning.">
				{#snippet actions()}
					<UiButton variant="primary" onclick={openWorkspaceManager}>Manage Workspaces</UiButton>
				{/snippet}
			</UiEmptyState>
		{:else if currentView === 'stats'}
			<div class="h-full overflow-auto p-3">
				<StatsPage />
			</div>
		{:else}
			<div class="grid h-full min-h-0 grid-cols-1 lg:grid-cols-[20rem_minmax(0,1fr)]">
				<aside class="min-h-0 border-r border-base-300 bg-base-100">
					{#key app.treeRefreshKey}
						<FileTree {selectedPath} onSelect={handleSelect} onScanInto={goToScan} />
					{/key}
				</aside>
				<section class="min-h-0 overflow-hidden bg-base-100">
					{#if selectedPath && selectedEntry}
						<FileDetails path={selectedPath} entry={selectedEntry} />
					{:else}
						<UiEmptyState title="Select a file" message="Choose a stored path to inspect metadata and duplicate locations." />
					{/if}
				</section>
			</div>
		{/if}
	</div>
</AppShell>
