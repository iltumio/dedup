<script lang="ts">
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import StatsPage from '$lib/components/StatsPage.svelte';
	import {
		scanDirectory,
		onScanProgress,
		formatSize,
		listWorkspaces,
		createWorkspace,
		switchWorkspace,
		deleteWorkspace,
		exportWorkspaces,
		importWorkspaces,
		importWorkspace,
		type DirEntry,
		type ScanStats,
		type ScanProgress,
		type Workspace,
		type WorkspacesConfig
	} from '$lib/api/tauri';
	import type { UnlistenFn } from '@tauri-apps/api/event';

	// ── File browser state ──
	let selectedPath = $state<string | null>(null);
	let selectedEntry = $state<DirEntry | null>(null);
	let scanSource = $state('');
	let targetPath = $state('/');
	let scanning = $state(false);
	let scanResult = $state<ScanStats | null>(null);
	let scanError = $state<string | null>(null);
	let showScanDialog = $state(false);
	let progress = $state<ScanProgress | null>(null);
	let treeRefreshKey = $state(0);
	let currentView = $state<'files' | 'stats'>('files');

	// ── Workspace state ──
	let workspacesConfig = $state<WorkspacesConfig>({ workspaces: [], active_workspace_id: null });
	let showWorkspaceDialog = $state(false);
	let showCreateWorkspace = $state(false);
	let newWsLabel = $state('');
	let newWsTags = $state('');
	let newWsStorePath = $state('');
	let wsError = $state<string | null>(null);
	let showImportWorkspace = $state(false);
	let importWsStorePath = $state('');
	let importWsLabel = $state('');
	let importingWs = $state(false);

	let activeWorkspace = $derived(
		workspacesConfig.workspaces.find((w) => w.id === workspacesConfig.active_workspace_id) ?? null
	);
	let hasWorkspace = $derived(activeWorkspace !== null);

	// Aggregated stats across all workspaces
	let aggFiles = $derived(workspacesConfig.workspaces.reduce((s, w) => s + w.stats.total_files, 0));
	let aggDuplicates = $derived(workspacesConfig.workspaces.reduce((s, w) => s + w.stats.duplicate_files, 0));
	let aggOriginal = $derived(workspacesConfig.workspaces.reduce((s, w) => s + w.stats.total_original_bytes, 0));
	let aggStored = $derived(workspacesConfig.workspaces.reduce((s, w) => s + w.stats.total_stored_bytes, 0));
	let aggSavedBytes = $derived(aggOriginal - aggStored);
	let aggSavedPct = $derived(aggOriginal > 0 ? ((1 - aggStored / aggOriginal) * 100).toFixed(1) : '0.0');

	// Load workspaces on mount
	$effect(() => {
		loadWorkspaces();
	});

	async function loadWorkspaces() {
		try {
			workspacesConfig = await listWorkspaces();
			treeRefreshKey++;
		} catch (e) {
			console.error('Failed to load workspaces:', e);
		}
	}

	// ── Workspace actions ──
	function openWorkspaceManager() {
		wsError = null;
		showCreateWorkspace = false;
		showImportWorkspace = false;
		showWorkspaceDialog = true;
	}

	function openCreateWorkspace() {
		newWsLabel = '';
		newWsTags = '';
		newWsStorePath = '';
		wsError = null;
		showCreateWorkspace = true;
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
			workspacesConfig = await listWorkspaces();
			showCreateWorkspace = false;
		} catch (e) {
			wsError = String(e);
		}
	}

	async function handleSwitchWorkspace(id: string) {
		wsError = null;
		try {
			await switchWorkspace(id);
			workspacesConfig = await listWorkspaces();
			selectedPath = null;
			selectedEntry = null;
			scanResult = null;
			treeRefreshKey++;
			showWorkspaceDialog = false;
		} catch (e) {
			wsError = String(e);
		}
	}

	async function handleDeleteWorkspace(id: string) {
		wsError = null;
		try {
			await deleteWorkspace(id);
			workspacesConfig = await listWorkspaces();
			if (workspacesConfig.active_workspace_id) {
				treeRefreshKey++;
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
				const json = await file.text();
				workspacesConfig = await importWorkspaces(json);
				treeRefreshKey++;
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
		showImportWorkspace = true;
	}

	async function handleImportWorkspace() {
		if (!importWsStorePath.trim() || !importWsLabel.trim()) return;
		wsError = null;
		importingWs = true;
		try {
			await importWorkspace(importWsStorePath, importWsLabel);
			workspacesConfig = await listWorkspaces();
			showImportWorkspace = false;
			treeRefreshKey++;
		} catch (e) {
			wsError = String(e);
		} finally {
			importingWs = false;
		}
	}

	// ── File browser actions ──
	function handleSelect(path: string, entry: DirEntry) {
		selectedPath = path;
		selectedEntry = entry;
	}

	function openScanDialog(presetTarget?: string) {
		targetPath = presetTarget ?? '/';
		scanError = null;
		progress = null;
		showScanDialog = true;
	}

	async function handleScan() {
		if (!scanSource.trim()) return;
		scanning = true;
		scanError = null;
		progress = null;

		let unlisten: UnlistenFn | null = null;

		try {
			unlisten = await onScanProgress((p) => {
				progress = p;
			});

			scanResult = await scanDirectory(scanSource, targetPath);
			showScanDialog = false;
			treeRefreshKey++;
			// Refresh workspace stats
			workspacesConfig = await listWorkspaces();
		} catch (e) {
			scanError = String(e);
		} finally {
			unlisten?.();
			scanning = false;
		}
	}

	let savedBytes = $derived(
		scanResult ? scanResult.total_original_bytes - scanResult.total_stored_bytes : 0
	);
	let savedPct = $derived(
		scanResult && scanResult.total_original_bytes > 0
			? ((1 - scanResult.total_stored_bytes / scanResult.total_original_bytes) * 100).toFixed(1)
			: '0.0'
	);
</script>

<div class="app">
	<header class="toolbar">
		<h1 class="logo">dedup</h1>

		<button class="workspace-btn" onclick={openWorkspaceManager}>
			{#if activeWorkspace}
				<span class="ws-label">{activeWorkspace.label}</span>
				{#each activeWorkspace.tags.slice(0, 2) as tag}
					<span class="ws-tag">{tag}</span>
				{/each}
			{:else}
				<span class="ws-none">No workspace</span>
			{/if}
		</button>

		<button class="scan-btn" onclick={() => openScanDialog()} disabled={!hasWorkspace}>
			Scan Directory
		</button>


		{#if hasWorkspace}
			<div class="view-toggle">
				<button class:active={currentView === 'files'} onclick={() => (currentView = 'files')}>Files</button>
				<button class:active={currentView === 'stats'} onclick={() => (currentView = 'stats')}>Stats</button>
			</div>
		{/if}

		{#if aggFiles > 0}
			<div class="scan-stats">
				<span>{aggFiles} files</span>
				<span class="sep">·</span>
				<span class="highlight">{aggDuplicates} duplicates</span>
				<span class="sep">·</span>
				<span class="saved">saved {formatSize(aggSavedBytes)} ({aggSavedPct}%)</span>
			</div>
		{/if}
	</header>

	<!-- Scan Dialog -->
	{#if showScanDialog}
		<div class="dialog-overlay" role="dialog">
			<div class="dialog-content">
				<h2>Scan Directory</h2>
				<label>
					<span>Source directory</span>
					<input
						type="text"
						bind:value={scanSource}
						placeholder="/path/to/directory"
						disabled={scanning}
					/>
				</label>
				<label>
					<span>Place content under (virtual path)</span>
					<input
						type="text"
						bind:value={targetPath}
						placeholder="/"
						disabled={scanning}
					/>
					<span class="hint">Use "/" for root, or e.g. "/photos/vacation" to nest</span>
				</label>

				{#if scanning && progress}
					<div class="progress-section">
						<div class="progress-bar-track">
							<div class="progress-bar-fill" style="width: 100%"></div>
						</div>
						<div class="progress-stats">
							<div class="stat-row">
								<span class="stat-label">Files</span>
								<span class="stat-value">{progress.files_processed}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Processed</span>
								<span class="stat-value">{formatSize(progress.bytes_processed)}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Stored</span>
								<span class="stat-value">{formatSize(progress.bytes_stored)}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Duplicates</span>
								<span class="stat-value highlight">{progress.duplicates_found}</span>
							</div>
							{#if progress.bytes_processed > 0}
								<div class="stat-row">
									<span class="stat-label">Space saved</span>
									<span class="stat-value saved">
										{formatSize(progress.bytes_processed - progress.bytes_stored)}
									</span>
								</div>
							{/if}
						</div>
						<div class="current-file" title={progress.current_file}>
							{progress.current_file}
						</div>
					</div>
				{:else if scanning}
					<div class="progress-section">
						<div class="progress-bar-track">
							<div class="progress-bar-fill indeterminate"></div>
						</div>
						<div class="current-file">Starting scan...</div>
					</div>
				{/if}

				{#if scanError}
					<div class="error">{scanError}</div>
				{/if}

				<div class="dialog-actions">
					<button class="cancel" onclick={() => (showScanDialog = false)} disabled={scanning}>
						Cancel
					</button>
					<button class="primary" onclick={handleScan} disabled={scanning}>
						{scanning ? 'Scanning...' : 'Start Scan'}
					</button>
				</div>
			</div>
		</div>
	{/if}

	<!-- Workspace Manager Dialog -->
	{#if showWorkspaceDialog}
		<div class="dialog-overlay" role="dialog">
			<div class="dialog-content dialog-wide">
				{#if showCreateWorkspace}
					<h2>New Workspace</h2>
					<label>
						<span>Label</span>
						<input type="text" bind:value={newWsLabel} placeholder="My Photos" />
					</label>
					<label>
						<span>Tags (comma-separated)</span>
						<input type="text" bind:value={newWsTags} placeholder="photos, backup" />
					</label>
					<label>
						<span>Store path</span>
						<input
							type="text"
							bind:value={newWsStorePath}
							placeholder="/path/to/workspace/.store"
						/>
						<span class="hint">Directory where blobs and metadata will be stored</span>
					</label>

					{#if wsError}
						<div class="error">{wsError}</div>
					{/if}

					<div class="dialog-actions">
						<button class="cancel" onclick={() => (showCreateWorkspace = false)}>
							Back
						</button>
						<button class="primary" onclick={handleCreateWorkspace}>
							Create
						</button>
					</div>
				{:else if showImportWorkspace}
					<h2>Import Existing Store</h2>
					<label>
						<span>Label</span>
						<input type="text" bind:value={importWsLabel} placeholder="My Imported Store" disabled={importingWs} />
					</label>
					<label>
						<span>Store path</span>
						<input
							type="text"
							bind:value={importWsStorePath}
							placeholder="/path/to/.store or /path/to/.store/metadata.redb"
							disabled={importingWs}
						/>
						<span class="hint">Path to an existing .store directory or its metadata.redb file</span>
					</label>

					{#if wsError}
						<div class="error">{wsError}</div>
					{/if}

					<div class="dialog-actions">
						<button class="cancel" onclick={() => (showImportWorkspace = false)} disabled={importingWs}>
							Back
						</button>
						<button class="primary" onclick={handleImportWorkspace} disabled={importingWs}>
							{importingWs ? 'Importing...' : 'Import'}
						</button>
					</div>
				{:else}
					<h2>Workspaces</h2>

					{#if workspacesConfig.workspaces.length === 0}
						<div class="ws-empty">
							<p>No workspaces yet. Create one to get started.</p>
						</div>
					{:else}
						<ul class="ws-list">
							{#each workspacesConfig.workspaces as ws (ws.id)}
								<li
									class="ws-item"
									class:active={ws.id === workspacesConfig.active_workspace_id}
								>
									<button
										class="ws-item-main"
										onclick={() => handleSwitchWorkspace(ws.id)}
									>
										<span class="ws-item-label">{ws.label}</span>
										<span class="ws-item-path">{ws.store_path}</span>
										{#if ws.tags.length > 0}
											<div class="ws-item-tags">
												{#each ws.tags as tag}
													<span class="ws-tag">{tag}</span>
												{/each}
											</div>
										{/if}
										{#if ws.stats.total_files > 0}
											<div class="ws-item-stats">
												<span>{ws.stats.total_files} files</span>
												<span class="sep">·</span>
												<span class="highlight">{ws.stats.duplicate_files} dups</span>
												<span class="sep">·</span>
												<span class="saved">saved {formatSize(ws.stats.total_original_bytes - ws.stats.total_stored_bytes)}</span>
												<span class="sep">·</span>
												<span>{ws.stats.scans_count} scans</span>
											</div>
										{/if}
									</button>
									<button
										class="ws-delete-btn"
										onclick={() => handleDeleteWorkspace(ws.id)}
										title="Delete workspace"
									>
										×
									</button>
								</li>
							{/each}
						</ul>
					{/if}

					{#if wsError}
						<div class="error">{wsError}</div>
					{/if}

					<div class="dialog-actions">
						<button class="secondary" onclick={handleExportWorkspaces}>Export</button>
						<button class="secondary" onclick={handleImportWorkspaces}>Import Config</button>
						<div class="spacer"></div>
						<button class="cancel" onclick={() => (showWorkspaceDialog = false)}>
							Close
						</button>
						<button class="primary" onclick={openImportWorkspace}>
							Import Existing
						</button>
						<button class="primary" onclick={openCreateWorkspace}>
							New Workspace
						</button>
					</div>
				{/if}
			</div>
		</div>
	{/if}

	<main class="content">
		{#if !hasWorkspace}
			<div class="no-workspace">
				<div class="no-ws-content">
					<h2>Welcome to dedup</h2>
					<p>Create a workspace to get started</p>
					<button class="primary" onclick={openWorkspaceManager}>
						Manage Workspaces
					</button>
				</div>
			</div>
		{:else if currentView === 'stats'}
			<StatsPage />
		{:else}
			<aside class="sidebar">
				{#key treeRefreshKey}
					<FileTree {selectedPath} onSelect={handleSelect} onScanInto={openScanDialog} />
				{/key}
			</aside>
			<section class="details">
				{#if selectedPath && selectedEntry}
					<FileDetails path={selectedPath} entry={selectedEntry} />
				{:else}
					<div class="placeholder">
						<p>Select a file to view details</p>
					</div>
				{/if}
			</section>
		{/if}
	</main>
</div>

<style>
	.app {
		display: flex;
		flex-direction: column;
		height: 100vh;
	}

	.toolbar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.logo {
		font-size: 16px;
		font-weight: 700;
		letter-spacing: -0.5px;
	}

	/* View toggle */
	.view-toggle {
		display: flex;
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
	}

	.view-toggle button {
		padding: 5px 12px;
		font-size: 12px;
		background: var(--bg);
		border: none;
		border-right: 1px solid var(--border);
		color: var(--text-muted);
		cursor: pointer;
		transition: all 0.15s;
	}

	.view-toggle button:last-child {
		border-right: none;
	}

	.view-toggle button:hover {
		color: var(--text);
	}

	.view-toggle button.active {
		background: var(--accent);
		color: var(--text);
		font-weight: 500;
	}

	/* Workspace button */
	.workspace-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 12px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		cursor: pointer;
		transition: border-color 0.15s;
	}

	.workspace-btn:hover {
		border-color: var(--accent);
	}

	.ws-label {
		font-weight: 500;
	}

	.ws-none {
		color: var(--text-muted);
		font-style: italic;
	}

	.ws-tag {
		display: inline-block;
		padding: 1px 6px;
		background: var(--accent);
		border-radius: 3px;
		font-size: 10px;
		font-weight: 500;
		opacity: 0.8;
	}

	.scan-btn {
		padding: 6px 14px;
		background: var(--accent);
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		transition: background 0.15s;
	}

	.scan-btn:hover:not(:disabled) {
		background: var(--accent-light);
	}

	.scan-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.scan-stats {
		display: flex;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.scan-stats .highlight {
		color: var(--duplicate);
		font-weight: 600;
	}

	.scan-stats .saved {
		color: var(--success);
		font-weight: 600;
	}

	.sep {
		opacity: 0.4;
	}

	.content {
		display: flex;
		flex: 1;
		overflow: hidden;
	}

	.sidebar {
		width: 320px;
		border-right: 1px solid var(--border);
		overflow: hidden;
	}

	.details {
		flex: 1;
		overflow: hidden;
	}

	.placeholder {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--text-muted);
		font-size: 14px;
	}

	/* No workspace landing */
	.no-workspace {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.no-ws-content {
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
	}

	.no-ws-content h2 {
		font-size: 20px;
		font-weight: 600;
	}

	.no-ws-content p {
		color: var(--text-muted);
		font-size: 14px;
	}

	/* Dialog */
	.dialog-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}

	.dialog-content {
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 24px;
		width: 480px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.dialog-wide {
		width: 540px;
		max-height: 70vh;
		overflow-y: auto;
	}

	.dialog-content h2 {
		font-size: 16px;
		font-weight: 600;
	}

	.dialog-content label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 13px;
	}

	.dialog-content label span {
		color: var(--text-muted);
	}

	.hint {
		font-size: 11px !important;
		opacity: 0.6;
	}

	.dialog-content input {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 8px 12px;
		font-size: 13px;
		font-family: var(--font-mono);
	}

	.dialog-content input:focus {
		outline: none;
		border-color: var(--accent-light);
	}

	.dialog-content input:disabled {
		opacity: 0.5;
	}

	/* Progress */
	.progress-section {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		background: var(--bg);
		border-radius: 8px;
		border: 1px solid var(--border);
	}

	.progress-bar-track {
		height: 6px;
		background: var(--border);
		border-radius: 3px;
		overflow: hidden;
	}

	.progress-bar-fill {
		height: 100%;
		background: var(--accent-light);
		border-radius: 3px;
		transition: width 0.3s;
	}

	.progress-bar-fill.indeterminate {
		width: 30% !important;
		animation: indeterminate 1.5s infinite ease-in-out;
	}

	@keyframes indeterminate {
		0% { transform: translateX(-100%); }
		100% { transform: translateX(400%); }
	}

	.progress-stats {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 4px 16px;
	}

	.stat-row {
		display: flex;
		justify-content: space-between;
		font-size: 12px;
	}

	.stat-label {
		color: var(--text-muted);
	}

	.stat-value {
		font-family: var(--font-mono);
		font-size: 11px;
	}

	.stat-value.highlight {
		color: var(--duplicate);
		font-weight: 600;
	}

	.stat-value.saved {
		color: var(--success);
		font-weight: 600;
	}

	.current-file {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.dialog-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.spacer {
		flex: 1;
	}

	.dialog-actions button {
		padding: 8px 16px;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
	}

	.cancel {
		background: var(--bg);
		border: 1px solid var(--border);
	}

	.secondary {
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text-muted);
	}

	.secondary:hover {
		border-color: var(--accent);
		color: var(--text);
	}

	.primary {
		background: var(--accent);
	}

	.primary:hover {
		background: var(--accent-light);
	}

	.primary:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		color: var(--duplicate);
		font-size: 13px;
	}

	/* Workspace list */
	.ws-empty {
		text-align: center;
		padding: 24px;
		color: var(--text-muted);
		font-size: 13px;
	}

	.ws-list {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.ws-item {
		display: flex;
		align-items: center;
		gap: 8px;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		transition: border-color 0.15s;
	}

	.ws-item:hover {
		border-color: var(--accent);
	}

	.ws-item.active {
		border-color: var(--accent-light);
		background: rgba(69, 160, 165, 0.08);
	}

	.ws-item-main {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 10px 14px;
		text-align: left;
		background: none;
		border: none;
		cursor: pointer;
	}

	.ws-item-label {
		font-size: 14px;
		font-weight: 500;
	}

	.ws-item-path {
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ws-item-tags {
		display: flex;
		gap: 4px;
		flex-wrap: wrap;
	}

	.ws-item-stats {
		display: flex;
		gap: 6px;
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		margin-top: 2px;
	}

	.ws-item-stats .highlight {
		color: var(--duplicate);
		font-weight: 600;
	}

	.ws-item-stats .saved {
		color: var(--success);
		font-weight: 600;
	}

	.ws-delete-btn {
		flex-shrink: 0;
		width: 32px;
		height: 32px;
		border-radius: 4px;
		background: none;
		border: none;
		font-size: 18px;
		color: var(--text-muted);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		margin-right: 8px;
	}

	.ws-delete-btn:hover {
		background: rgba(239, 83, 80, 0.15);
		color: var(--duplicate);
	}
</style>
