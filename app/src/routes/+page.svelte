<script lang="ts">
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import StatsPage from '$lib/components/StatsPage.svelte';
	import AppShell from '$lib/components/AppShell.svelte';
	import { UiButton, UiEmptyState } from '$lib/components/ui';
	import WorkspaceManagerDialog from '$lib/components/workspace/WorkspaceManagerDialog.svelte';
	import ScanDialog from '$lib/components/scan/ScanDialog.svelte';
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

	function handlePresetChange(id: string, checked: boolean) {
		if (id === 'git') bundleGitDirs = checked;
		if (id === 'rust') ignoreRustTarget = checked;
		if (id === 'node') ignoreNodeModules = checked;
		if (id === 'python') ignorePythonVenv = checked;
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
		<button class="btn btn-neutral btn-sm min-w-0 max-w-72 w-full justify-start" type="button" onclick={openWorkspaceManager}>
			{#if activeWorkspace}
				<span class="truncate">{activeWorkspace.label}</span>
			{:else}
				<span class="text-base-content/50">No workspace</span>
			{/if}
		</button>
	{/snippet}

	<!-- Scan Dialog -->
	<ScanDialog
		open={showScanDialog}
		{scanning}
		{cancelling}
		{savingCustomRules}
		source={scanSource}
		{targetPath}
		{bundleGitDirs}
		{ignoreRustTarget}
		{ignoreNodeModules}
		{ignorePythonVenv}
		customRules={customScanRules}
		{activeCustomRuleIds}
		{newRuleLabel}
		{newRulePattern}
		{newRuleAction}
		{customRulesError}
		{scanError}
		{progress}
		onCloseOrCancel={handleCancelScan}
		onScan={handleScan}
		onSourceChange={(value) => (scanSource = value)}
		onTargetPathChange={(value) => (targetPath = value)}
		onPresetChange={handlePresetChange}
		onToggleCustomRule={toggleCustomRule}
		onRemoveCustomRule={handleRemoveCustomRule}
		onNewRuleLabelChange={(value) => (newRuleLabel = value)}
		onNewRulePatternChange={(value) => (newRulePattern = value)}
		onNewRuleActionChange={(value) => (newRuleAction = value)}
		onAddCustomRule={handleAddCustomRule}
	/>

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

	<div class="min-h-0 flex-1 overflow-hidden">
		{#if !hasWorkspace}
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
					{#key treeRefreshKey}
						<FileTree {selectedPath} onSelect={handleSelect} onScanInto={openScanDialog} />
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
