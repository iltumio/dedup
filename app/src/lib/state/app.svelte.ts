import {
	scanDirectory,
	cancelScan,
	onScanProgress,
	formatSize,
	listWorkspaces,
	listCustomScanRules,
	saveCustomScanRules,
	type ScanStats,
	type ScanProgress,
	type WorkspacesConfig,
	type CustomScanRule,
	type ScanRule,
	type ScanRuleAction
} from '$lib/api/tauri';
import type { UnlistenFn } from '@tauri-apps/api/event';

interface ShellStat {
	label: string;
	value: string | number;
	tone?: 'default' | 'success' | 'error' | 'info' | 'warning';
}

class AppState {
	// ── Workspaces ──
	workspacesConfig = $state<WorkspacesConfig>({
		workspaces: [],
		active_workspace_id: null,
		custom_scan_rules: []
	});
	treeRefreshKey = $state(0);

	// ── Custom scan rules ──
	customScanRules = $state<CustomScanRule[]>([]);
	activeCustomRuleIds = $state<string[]>([]);
	customRulesError = $state<string | null>(null);
	savingCustomRules = $state(false);
	newRuleLabel = $state('');
	newRulePattern = $state('');
	newRuleAction = $state<ScanRuleAction>('ignore');

	// ── Scan form ──
	scanSource = $state('');
	targetPath = $state('/');
	bundleGitDirs = $state(false);
	ignoreRustTarget = $state(false);
	ignoreNodeModules = $state(false);
	ignorePythonVenv = $state(false);
	scanning = $state(false);
	cancelling = $state(false);
	scanResult = $state<ScanStats | null>(null);
	scanError = $state<string | null>(null);
	progress = $state<ScanProgress | null>(null);

	// ── Derived ──
	activeWorkspace = $derived(
		this.workspacesConfig.workspaces.find(
			(w) => w.id === this.workspacesConfig.active_workspace_id
		) ?? null
	);
	hasWorkspace = $derived(this.activeWorkspace !== null);

	aggFiles = $derived(
		this.workspacesConfig.workspaces.reduce((s, w) => s + w.stats.total_files, 0)
	);
	aggDuplicates = $derived(
		this.workspacesConfig.workspaces.reduce((s, w) => s + w.stats.duplicate_files, 0)
	);
	aggOriginal = $derived(
		this.workspacesConfig.workspaces.reduce((s, w) => s + w.stats.total_original_bytes, 0)
	);
	aggStored = $derived(
		this.workspacesConfig.workspaces.reduce((s, w) => s + w.stats.total_stored_bytes, 0)
	);
	aggSavedBytes = $derived(this.aggOriginal - this.aggStored);
	aggSavedPct = $derived(
		this.aggOriginal > 0 ? ((1 - this.aggStored / this.aggOriginal) * 100).toFixed(1) : '0.0'
	);
	shellStats = $derived<ShellStat[]>(
		this.aggFiles > 0
			? [
					{ label: 'Files', value: this.aggFiles },
					{ label: 'Duplicates', value: this.aggDuplicates, tone: 'error' },
					{
						label: 'Saved',
						value: `${formatSize(this.aggSavedBytes)} (${this.aggSavedPct}%)`,
						tone: 'success'
					}
				]
			: []
	);

	// ── Loading ──
	loadWorkspaces = async () => {
		try {
			this.workspacesConfig = await listWorkspaces();
			this.treeRefreshKey++;
		} catch (e) {
			console.error('Failed to load workspaces:', e);
		}
	};

	loadCustomScanRules = async () => {
		try {
			this.customScanRules = await listCustomScanRules();
			this.activeCustomRuleIds = this.customScanRules
				.filter((rule) => rule.enabled)
				.map((rule) => rule.id);
			this.customRulesError = null;
		} catch (e) {
			this.customRulesError = String(e);
		}
	};

	syncCustomScanRulesFromConfig = (nextRules: CustomScanRule[]) => {
		this.customScanRules = nextRules;
		this.activeCustomRuleIds = this.customScanRules
			.filter((rule) => rule.enabled)
			.map((rule) => rule.id);
	};

	// ── Scan form actions ──

	prepareScan = (presetTarget?: string) => {
		this.targetPath = presetTarget ?? '/';
		this.bundleGitDirs = false;
		this.ignoreRustTarget = false;
		this.ignoreNodeModules = false;
		this.ignorePythonVenv = false;
		this.activeCustomRuleIds = this.customScanRules
			.filter((rule) => rule.enabled)
			.map((rule) => rule.id);
		this.scanError = null;
		this.customRulesError = null;
		this.progress = null;
	};

	handlePresetChange = (id: string, checked: boolean) => {
		if (id === 'git') this.bundleGitDirs = checked;
		if (id === 'rust') this.ignoreRustTarget = checked;
		if (id === 'node') this.ignoreNodeModules = checked;
		if (id === 'python') this.ignorePythonVenv = checked;
	};

	buildScanRules = (): ScanRule[] => {
		const rules: ScanRule[] = [];
		if (this.bundleGitDirs) {
			rules.push({ pattern: '(^|/)\\.git$', action: 'archive' });
		}
		if (this.ignoreRustTarget) {
			rules.push({ pattern: '(^|/)target$', action: 'ignore' });
		}
		if (this.ignoreNodeModules) {
			rules.push({ pattern: '(^|/)node_modules$', action: 'ignore' });
		}
		if (this.ignorePythonVenv) {
			rules.push({ pattern: '(^|/)(\\.venv|venv)$', action: 'ignore' });
		}
		const activeCustomRules = new Set(this.activeCustomRuleIds);
		for (const rule of this.customScanRules) {
			if (activeCustomRules.has(rule.id)) {
				rules.push({ pattern: rule.pattern, action: rule.action });
			}
		}
		return rules;
	};

	toggleCustomRule = (ruleId: string, checked: boolean) => {
		if (this.savingCustomRules) return;
		if (checked) {
			this.activeCustomRuleIds = Array.from(new Set([...this.activeCustomRuleIds, ruleId]));
		} else {
			this.activeCustomRuleIds = this.activeCustomRuleIds.filter((id) => id !== ruleId);
		}
	};

	addCustomRule = async () => {
		if (this.savingCustomRules || !this.newRuleLabel.trim() || !this.newRulePattern.trim()) return;
		this.customRulesError = null;
		const newRuleId = `rule_${Date.now().toString(16)}`;
		const previousRules = this.customScanRules;
		const previousActiveRuleIds = this.activeCustomRuleIds;
		const activeBeforeSave = new Set(this.activeCustomRuleIds);
		const nextRules = [
			...this.customScanRules,
			{
				id: newRuleId,
				label: this.newRuleLabel.trim(),
				pattern: this.newRulePattern.trim(),
				action: this.newRuleAction,
				enabled: true
			}
		];
		this.savingCustomRules = true;
		try {
			this.customScanRules = await saveCustomScanRules(nextRules);
			const savedRuleIds = new Set(this.customScanRules.map((rule) => rule.id));
			this.activeCustomRuleIds = Array.from(new Set([...activeBeforeSave, newRuleId])).filter(
				(id) => savedRuleIds.has(id)
			);
			this.newRuleLabel = '';
			this.newRulePattern = '';
			this.newRuleAction = 'ignore';
		} catch (e) {
			this.customScanRules = previousRules;
			this.activeCustomRuleIds = previousActiveRuleIds;
			this.customRulesError = String(e);
		} finally {
			this.savingCustomRules = false;
		}
	};

	removeCustomRule = async (ruleId: string) => {
		if (this.savingCustomRules) return;
		this.customRulesError = null;
		const previousRules = this.customScanRules;
		const previousActiveRuleIds = this.activeCustomRuleIds;
		const nextRules = this.customScanRules.filter((rule) => rule.id !== ruleId);
		this.customScanRules = nextRules;
		this.activeCustomRuleIds = this.activeCustomRuleIds.filter((id) => id !== ruleId);
		this.savingCustomRules = true;
		try {
			this.customScanRules = await saveCustomScanRules(nextRules);
			const savedRuleIds = new Set(this.customScanRules.map((rule) => rule.id));
			this.activeCustomRuleIds = this.activeCustomRuleIds.filter((id) => savedRuleIds.has(id));
		} catch (e) {
			this.customScanRules = previousRules;
			this.activeCustomRuleIds = previousActiveRuleIds;
			this.customRulesError = String(e);
		} finally {
			this.savingCustomRules = false;
		}
	};

	// Resolves true when the caller should leave the scan page (finished or
	// cancelled), false when it failed and the error must stay visible.
	runScan = async (): Promise<boolean> => {
		if (!this.scanSource.trim() || this.savingCustomRules) return false;
		this.scanning = true;
		this.cancelling = false;
		this.scanError = null;
		this.progress = null;

		let unlisten: UnlistenFn | null = null;

		try {
			unlisten = await onScanProgress((p) => {
				this.progress = p;
			});

			this.scanResult = await scanDirectory(
				this.scanSource,
				this.targetPath,
				false,
				this.buildScanRules()
			);
			this.treeRefreshKey++;
			this.workspacesConfig = await listWorkspaces();
			return true;
		} catch (e) {
			const message = String(e);
			if (this.cancelling && message.includes('scan cancelled')) {
				this.progress = null;
				return true;
			}
			this.scanError = message;
			return false;
		} finally {
			unlisten?.();
			this.scanning = false;
			this.cancelling = false;
		}
	};

	/** Request cancellation of an in-progress scan. */
	requestCancel = async () => {
		if (!this.scanning) return;
		this.cancelling = true;
		this.scanError = null;
		try {
			await cancelScan();
		} catch (e) {
			this.scanError = String(e);
			this.cancelling = false;
		}
	};
}

export const app = new AppState();
