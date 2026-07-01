<script lang="ts">
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import StatsPage from '$lib/components/StatsPage.svelte';
	import {
		scanDirectory,
		cancelScan,
		onScanProgress,
		formatSize,
		listWorkspaces,
		listCustomScanRules,
		saveCustomScanRules,
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
		type WorkspacesConfig,
		type CustomScanRule,
		type ScanRule
	} from '$lib/api/tauri';
	import type { UnlistenFn } from '@tauri-apps/api/event';

	// ── File browser state ──
	let selectedPath = $state<string | null>(null);
	let selectedEntry = $state<DirEntry | null>(null);
	let scanSource = $state('');
	let targetPath = $state('/');
	let bundleGitDirs = $state(false);
	let ignoreRustTarget = $state(false);
	let ignoreNodeModules = $state(false);
	let ignorePythonVenv = $state(false);
	let customScanRules = $state<CustomScanRule[]>([]);
	let activeCustomRuleIds = $state<string[]>([]);
	let customRulesError = $state<string | null>(null);
	let savingCustomRules = $state(false);
	let newRuleLabel = $state('');
	let newRulePattern = $state('');
	let newRuleAction = $state<'ignore' | 'archive'>('ignore');
	let scanning = $state(false);
	let cancelling = $state(false);
	let scanResult = $state<ScanStats | null>(null);
	let scanError = $state<string | null>(null);
	let showScanDialog = $state(false);
	let progress = $state<ScanProgress | null>(null);
	let treeRefreshKey = $state(0);
	let currentView = $state<'files' | 'stats'>('files');

	// ── Workspace state ──
	let workspacesConfig = $state<WorkspacesConfig>({
		workspaces: [],
		active_workspace_id: null,
		custom_scan_rules: []
	});
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
		loadCustomScanRules();
	});

	async function loadWorkspaces() {
		try {
			workspacesConfig = await listWorkspaces();
			treeRefreshKey++;
		} catch (e) {
			console.error('Failed to load workspaces:', e);
		}
	}

	async function loadCustomScanRules() {
		try {
			customScanRules = await listCustomScanRules();
			activeCustomRuleIds = customScanRules.filter((rule) => rule.enabled).map((rule) => rule.id);
			customRulesError = null;
		} catch (e) {
			customRulesError = String(e);
		}
	}

	function syncCustomScanRulesFromConfig(nextRules: CustomScanRule[]) {
		const previousRuleIds = new Set(customScanRules.map((rule) => rule.id));
		const nextRuleIds = new Set(nextRules.map((rule) => rule.id));
		customScanRules = nextRules;

		if (!showScanDialog) {
			activeCustomRuleIds = customScanRules.filter((rule) => rule.enabled).map((rule) => rule.id);
			return;
		}

		const preservedActiveIds = activeCustomRuleIds.filter((id) => nextRuleIds.has(id));
		const newlyEnabledIds = customScanRules
			.filter((rule) => rule.enabled && !previousRuleIds.has(rule.id))
			.map((rule) => rule.id);
		activeCustomRuleIds = Array.from(new Set([...preservedActiveIds, ...newlyEnabledIds]));
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
				wsError = null;
				try {
					const json = await file.text();
					workspacesConfig = await importWorkspaces(json);
					syncCustomScanRulesFromConfig(workspacesConfig.custom_scan_rules);
					treeRefreshKey++;
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
		bundleGitDirs = false;
		ignoreRustTarget = false;
		ignoreNodeModules = false;
		ignorePythonVenv = false;
		activeCustomRuleIds = customScanRules.filter((rule) => rule.enabled).map((rule) => rule.id);
		scanError = null;
		customRulesError = null;
		progress = null;
		showScanDialog = true;
	}

	function buildScanRules(): ScanRule[] {
		const rules: ScanRule[] = [];
		if (bundleGitDirs) {
			rules.push({ pattern: '(^|/)\\.git$', action: 'archive' });
		}
		if (ignoreRustTarget) {
			rules.push({ pattern: '(^|/)target$', action: 'ignore' });
		}
		if (ignoreNodeModules) {
			rules.push({ pattern: '(^|/)node_modules$', action: 'ignore' });
		}
		if (ignorePythonVenv) {
			rules.push({ pattern: '(^|/)(\\.venv|venv)$', action: 'ignore' });
		}
		const activeCustomRules = new Set(activeCustomRuleIds);
		for (const rule of customScanRules) {
			if (activeCustomRules.has(rule.id)) {
				rules.push({ pattern: rule.pattern, action: rule.action });
			}
		}
		return rules;
	}

	function toggleCustomRule(ruleId: string, checked: boolean) {
		if (savingCustomRules) return;
		if (checked) {
			activeCustomRuleIds = Array.from(new Set([...activeCustomRuleIds, ruleId]));
		} else {
			activeCustomRuleIds = activeCustomRuleIds.filter((id) => id !== ruleId);
		}
	}

	async function handleAddCustomRule() {
		if (savingCustomRules || !newRuleLabel.trim() || !newRulePattern.trim()) return;
		customRulesError = null;
		const newRuleId = `rule_${Date.now().toString(16)}`;
		const previousRules = customScanRules;
		const previousActiveRuleIds = activeCustomRuleIds;
		const activeBeforeSave = new Set(activeCustomRuleIds);
		const nextRules = [
			...customScanRules,
			{
				id: newRuleId,
				label: newRuleLabel.trim(),
				pattern: newRulePattern.trim(),
				action: newRuleAction,
				enabled: true
			}
		];
		savingCustomRules = true;
		try {
			customScanRules = await saveCustomScanRules(nextRules);
			const savedRuleIds = new Set(customScanRules.map((rule) => rule.id));
			activeCustomRuleIds = Array.from(new Set([...activeBeforeSave, newRuleId])).filter((id) =>
				savedRuleIds.has(id)
			);
			newRuleLabel = '';
			newRulePattern = '';
			newRuleAction = 'ignore';
		} catch (e) {
			customScanRules = previousRules;
			activeCustomRuleIds = previousActiveRuleIds;
			customRulesError = String(e);
		} finally {
			savingCustomRules = false;
		}
	}

	async function handleRemoveCustomRule(ruleId: string) {
		if (savingCustomRules) return;
		customRulesError = null;
		const previousRules = customScanRules;
		const previousActiveRuleIds = activeCustomRuleIds;
		const nextRules = customScanRules.filter((rule) => rule.id !== ruleId);
		customScanRules = nextRules;
		activeCustomRuleIds = activeCustomRuleIds.filter((id) => id !== ruleId);
		savingCustomRules = true;
		try {
			customScanRules = await saveCustomScanRules(nextRules);
			const savedRuleIds = new Set(customScanRules.map((rule) => rule.id));
			activeCustomRuleIds = activeCustomRuleIds.filter((id) => savedRuleIds.has(id));
		} catch (e) {
			customScanRules = previousRules;
			activeCustomRuleIds = previousActiveRuleIds;
			customRulesError = String(e);
		} finally {
			savingCustomRules = false;
		}
	}

	async function handleScan() {
		if (!scanSource.trim() || savingCustomRules) return;
		scanning = true;
		cancelling = false;
		scanError = null;
		progress = null;

		let unlisten: UnlistenFn | null = null;

		try {
			unlisten = await onScanProgress((p) => {
				progress = p;
			});

			scanResult = await scanDirectory(scanSource, targetPath, false, buildScanRules());
			showScanDialog = false;
			treeRefreshKey++;
			// Refresh workspace stats
			workspacesConfig = await listWorkspaces();
		} catch (e) {
			const message = String(e);
			if (cancelling && message.includes('scan cancelled')) {
				showScanDialog = false;
				progress = null;
			} else {
				scanError = message;
			}
		} finally {
			unlisten?.();
			scanning = false;
			cancelling = false;
		}
	}

	async function handleCancelScan() {
		if (!scanning) {
			showScanDialog = false;
			return;
		}

		cancelling = true;
		scanError = null;

		try {
			await cancelScan();
		} catch (e) {
			scanError = String(e);
			cancelling = false;
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

		<button class="scan-btn" onclick={() => openScanDialog()} disabled={!hasWorkspace || scanning}>
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
		<div class="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="scan-dialog-title">
			<div class="dialog-content">
				<h2 id="scan-dialog-title">Scan Directory</h2>
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
				<div class="scan-rules">
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={bundleGitDirs} disabled={scanning} />
						<span>Archive .git directories</span>
					</label>
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={ignoreRustTarget} disabled={scanning} />
						<span>Ignore Rust target directories</span>
					</label>
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={ignoreNodeModules} disabled={scanning} />
						<span>Ignore node_modules directories</span>
					</label>
					<label class="checkbox-row">
						<input type="checkbox" bind:checked={ignorePythonVenv} disabled={scanning} />
						<span>Ignore Python virtual environments</span>
					</label>
				</div>

				{#if customScanRules.length > 0}
					<div class="scan-rules">
						{#each customScanRules as rule (rule.id)}
							<div class="custom-rule-row">
								<label class="checkbox-row custom-rule-label">
									<input
										type="checkbox"
										checked={activeCustomRuleIds.includes(rule.id)}
										onchange={(event) =>
											toggleCustomRule(rule.id, event.currentTarget.checked)}
										disabled={scanning || savingCustomRules}
									/>
									<span>{rule.label}</span>
								</label>
								<button
									class="rule-remove"
									onclick={() => handleRemoveCustomRule(rule.id)}
									disabled={scanning || savingCustomRules}
								>
									Remove
								</button>
							</div>
						{/each}
					</div>
				{/if}

				<div class="custom-rule-editor">
					<input
						type="text"
						bind:value={newRuleLabel}
						aria-label="Custom rule label"
						placeholder="Rule label"
						disabled={scanning || savingCustomRules}
					/>
					<input
						type="text"
						bind:value={newRulePattern}
						aria-label="Custom rule regex"
						placeholder="Regex, e.g. (^|/)dist$"
						disabled={scanning || savingCustomRules}
					/>
					<select
						bind:value={newRuleAction}
						aria-label="Custom rule action"
						disabled={scanning || savingCustomRules}
					>
						<option value="ignore">Ignore</option>
						<option value="archive">Archive</option>
					</select>
					<button
						class="secondary"
						onclick={handleAddCustomRule}
						disabled={scanning || savingCustomRules}
					>
						Add Rule
					</button>
				</div>

				{#if customRulesError}
					<div class="error">{customRulesError}</div>
				{/if}

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
					<button class="cancel" onclick={handleCancelScan} disabled={cancelling}>
						{scanning ? (cancelling ? 'Cancelling...' : 'Cancel Scan') : 'Cancel'}
					</button>
					<button class="primary" onclick={handleScan} disabled={scanning || savingCustomRules}>
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
		background: var(--app-bg-secondary);
		border-bottom: 1px solid var(--app-border-color);
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
		border: 1px solid var(--app-border-color);
		border-radius: 6px;
		overflow: hidden;
	}

	.view-toggle button {
		padding: 5px 12px;
		font-size: 12px;
		background: var(--app-bg);
		border: none;
		border-right: 1px solid var(--app-border-color);
		color: var(--app-text-muted);
		cursor: pointer;
		transition: all 0.15s;
	}

	.view-toggle button:last-child {
		border-right: none;
	}

	.view-toggle button:hover {
		color: var(--app-text);
	}

	.view-toggle button.active {
		background: var(--app-accent);
		color: var(--app-text);
		font-weight: 500;
	}

	/* Workspace button */
	.workspace-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 12px;
		background: var(--app-bg);
		border: 1px solid var(--app-border-color);
		border-radius: 6px;
		font-size: 13px;
		cursor: pointer;
		transition: border-color 0.15s;
	}

	.workspace-btn:hover {
		border-color: var(--app-accent);
	}

	.ws-label {
		font-weight: 500;
	}

	.ws-none {
		color: var(--app-text-muted);
		font-style: italic;
	}

	.ws-tag {
		display: inline-block;
		padding: 1px 6px;
		background: var(--app-accent);
		border-radius: 3px;
		font-size: 10px;
		font-weight: 500;
		opacity: 0.8;
	}

	.scan-btn {
		padding: 6px 14px;
		background: var(--app-accent);
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		transition: background 0.15s;
	}

	.scan-btn:hover:not(:disabled) {
		background: var(--app-accent-light);
	}

	.scan-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.scan-stats {
		display: flex;
		gap: 6px;
		font-size: 12px;
		color: var(--app-text-muted);
		font-family: var(--app-font-mono);
	}

	.scan-stats .highlight {
		color: var(--app-duplicate);
		font-weight: 600;
	}

	.scan-stats .saved {
		color: var(--app-success);
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
		border-right: 1px solid var(--app-border-color);
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
		color: var(--app-text-muted);
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
		color: var(--app-text-muted);
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
		background: var(--app-bg-secondary);
		border: 1px solid var(--app-border-color);
		border-radius: 12px;
		padding: 24px;
		width: min(480px, calc(100vw - 32px));
		max-height: calc(100vh - 32px);
		overflow-y: auto;
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
		color: var(--app-text-muted);
	}

	.hint {
		font-size: 11px !important;
		opacity: 0.6;
	}

	.dialog-content input {
		background: var(--app-bg);
		border: 1px solid var(--app-border-color);
		border-radius: 6px;
		padding: 8px 12px;
		font-size: 13px;
		font-family: var(--app-font-mono);
	}

	.dialog-content input:focus {
		outline: none;
		border-color: var(--app-accent-light);
	}

	.dialog-content input:disabled {
		opacity: 0.5;
	}

	.checkbox-row {
		flex-direction: row !important;
		align-items: center;
		gap: 8px !important;
	}

	.checkbox-row input {
		width: 14px;
		height: 14px;
		padding: 0;
		margin: 0;
	}

	.checkbox-row span {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.scan-rules {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px 0;
	}

	.custom-rule-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.custom-rule-label {
		flex: 1;
		min-width: 0;
	}

	.rule-remove {
		margin-left: auto;
		padding: 4px 8px;
		background: var(--app-bg);
		border: 1px solid var(--app-border-color);
		border-radius: 6px;
		color: var(--app-text-muted);
		font-size: 12px;
		flex-shrink: 0;
	}

	.rule-remove:hover:not(:disabled) {
		border-color: var(--app-duplicate);
		color: var(--app-duplicate);
	}

	.custom-rule-editor {
		display: grid;
		grid-template-columns: 1fr 1fr 96px auto;
		gap: 8px;
		align-items: center;
	}

	.custom-rule-editor input {
		min-width: 0;
	}

	.custom-rule-editor select {
		background: var(--app-bg);
		border: 1px solid var(--app-border-color);
		border-radius: 6px;
		padding: 8px 10px;
		font-size: 13px;
	}

	.custom-rule-editor button {
		white-space: nowrap;
	}

	@media (max-width: 560px) {
		.custom-rule-editor {
			grid-template-columns: 1fr;
		}

		.custom-rule-editor button {
			width: 100%;
		}
	}

	/* Progress */
	.progress-section {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		background: var(--app-bg);
		border-radius: 8px;
		border: 1px solid var(--app-border-color);
	}

	.progress-bar-track {
		height: 6px;
		background: var(--app-border-color);
		border-radius: 3px;
		overflow: hidden;
	}

	.progress-bar-fill {
		height: 100%;
		background: var(--app-accent-light);
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
		color: var(--app-text-muted);
	}

	.stat-value {
		font-family: var(--app-font-mono);
		font-size: 11px;
	}

	.stat-value.highlight {
		color: var(--app-duplicate);
		font-weight: 600;
	}

	.stat-value.saved {
		color: var(--app-success);
		font-weight: 600;
	}

	.current-file {
		font-size: 11px;
		color: var(--app-text-muted);
		font-family: var(--app-font-mono);
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
		background: var(--app-bg);
		border: 1px solid var(--app-border-color);
	}

	.secondary {
		background: var(--app-bg);
		border: 1px solid var(--app-border-color);
		color: var(--app-text-muted);
	}

	.secondary:hover {
		border-color: var(--app-accent);
		color: var(--app-text);
	}

	.primary {
		background: var(--app-accent);
	}

	.primary:hover {
		background: var(--app-accent-light);
	}

	.primary:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		color: var(--app-duplicate);
		font-size: 13px;
	}

	/* Workspace list */
	.ws-empty {
		text-align: center;
		padding: 24px;
		color: var(--app-text-muted);
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
		border: 1px solid var(--app-border-color);
		border-radius: 8px;
		overflow: hidden;
		transition: border-color 0.15s;
	}

	.ws-item:hover {
		border-color: var(--app-accent);
	}

	.ws-item.active {
		border-color: var(--app-accent-light);
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
		font-family: var(--app-font-mono);
		color: var(--app-text-muted);
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
		color: var(--app-text-muted);
		font-family: var(--app-font-mono);
		margin-top: 2px;
	}

	.ws-item-stats .highlight {
		color: var(--app-duplicate);
		font-weight: 600;
	}

	.ws-item-stats .saved {
		color: var(--app-success);
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
		color: var(--app-text-muted);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		margin-right: 8px;
	}

	.ws-delete-btn:hover {
		background: rgba(239, 83, 80, 0.15);
		color: var(--app-duplicate);
	}
</style>
