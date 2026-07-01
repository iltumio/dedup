<script lang="ts">
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import StatsPage from '$lib/components/StatsPage.svelte';
	import AppShell from '$lib/components/AppShell.svelte';
	import WorkspaceManagerDialog from '$lib/components/workspace/WorkspaceManagerDialog.svelte';
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
	let workspaceDialogMode = $state<'list' | 'create' | 'import'>('list');
	let newWsLabel = $state('');
	let newWsTags = $state('');
	let newWsStorePath = $state('');
	let wsError = $state<string | null>(null);
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
	let shellStats = $derived(
		aggFiles > 0
			? [
					{ label: 'Files', value: aggFiles },
					{ label: 'Duplicates', value: aggDuplicates, tone: 'error' as const },
					{ label: 'Saved', value: `${formatSize(aggSavedBytes)} (${aggSavedPct}%)`, tone: 'success' as const }
				]
			: []
	);

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
			workspacesConfig = await listWorkspaces();
			workspaceDialogMode = 'list';
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
		workspaceDialogMode = 'import';
	}

	async function handleImportWorkspace() {
		if (!importWsStorePath.trim() || !importWsLabel.trim()) return;
		wsError = null;
		importingWs = true;
		try {
			await importWorkspace(importWsStorePath, importWsLabel);
			workspacesConfig = await listWorkspaces();
			workspaceDialogMode = 'list';
			treeRefreshKey++;
		} catch (e) {
			wsError = String(e);
		} finally {
			importingWs = false;
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

<AppShell
	{currentView}
	{hasWorkspace}
	{scanning}
	stats={shellStats}
	onViewChange={(view) => (currentView = view)}
	onScan={() => openScanDialog()}
>
	{#snippet workspaceControl()}
		<button class="btn btn-neutral btn-sm max-w-72 justify-start" type="button" onclick={openWorkspaceManager}>
			{#if activeWorkspace}
				<span class="truncate">{activeWorkspace.label}</span>
			{:else}
				<span class="text-base-content/50">No workspace</span>
			{/if}
		</button>
	{/snippet}

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

	<WorkspaceManagerDialog
		open={showWorkspaceDialog}
		config={workspacesConfig}
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
	/>

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
</AppShell>

<style>
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
</style>
