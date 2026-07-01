<script lang="ts">
	import type { CustomScanRule, ScanProgress, ScanRuleAction } from '$lib/api/tauri';
	import { UiButton, UiDialog, UiField } from '$lib/components/ui';
	import CustomScanRuleEditor from './CustomScanRuleEditor.svelte';
	import CustomScanRuleList from './CustomScanRuleList.svelte';
	import ScanPresetList from './ScanPresetList.svelte';
	import ScanProgressPanel from './ScanProgressPanel.svelte';

	interface Props {
		open: boolean;
		scanning: boolean;
		cancelling: boolean;
		savingCustomRules: boolean;
		source: string;
		targetPath: string;
		bundleGitDirs: boolean;
		ignoreRustTarget: boolean;
		ignoreNodeModules: boolean;
		ignorePythonVenv: boolean;
		customRules: CustomScanRule[];
		activeCustomRuleIds: string[];
		newRuleLabel: string;
		newRulePattern: string;
		newRuleAction: ScanRuleAction;
		customRulesError: string | null;
		scanError: string | null;
		progress: ScanProgress | null;
		onCloseOrCancel: () => void;
		onScan: () => void;
		onSourceChange: (value: string) => void;
		onTargetPathChange: (value: string) => void;
		onPresetChange: (id: string, checked: boolean) => void;
		onToggleCustomRule: (id: string, checked: boolean) => void;
		onRemoveCustomRule: (id: string) => void;
		onNewRuleLabelChange: (value: string) => void;
		onNewRulePatternChange: (value: string) => void;
		onNewRuleActionChange: (value: ScanRuleAction) => void;
		onAddCustomRule: () => void;
	}

	let {
		open,
		scanning,
		cancelling,
		savingCustomRules,
		source,
		targetPath,
		bundleGitDirs,
		ignoreRustTarget,
		ignoreNodeModules,
		ignorePythonVenv,
		customRules,
		activeCustomRuleIds,
		newRuleLabel,
		newRulePattern,
		newRuleAction,
		customRulesError,
		scanError,
		progress,
		onCloseOrCancel,
		onScan,
		onSourceChange,
		onTargetPathChange,
		onPresetChange,
		onToggleCustomRule,
		onRemoveCustomRule,
		onNewRuleLabelChange,
		onNewRulePatternChange,
		onNewRuleActionChange,
		onAddCustomRule
	}: Props = $props();

	let presets = $derived([
		{
			id: 'git',
			label: 'Archive .git directories',
			description: 'Store repository metadata as bundled content.',
			checked: bundleGitDirs
		},
		{
			id: 'rust',
			label: 'Ignore Rust target directories',
			description: 'Skip Cargo build output directories named target.',
			checked: ignoreRustTarget
		},
		{
			id: 'node',
			label: 'Ignore node_modules directories',
			description: 'Skip installed JavaScript dependency trees.',
			checked: ignoreNodeModules
		},
		{
			id: 'python',
			label: 'Ignore Python virtual environments',
			description: 'Skip directories named .venv or venv.',
			checked: ignorePythonVenv
		}
	]);

	let rulesDisabled = $derived(scanning || savingCustomRules);
	let startDisabled = $derived(scanning || savingCustomRules || !source.trim());

	function inputValue(event: Event) {
		return (event.currentTarget as HTMLInputElement).value;
	}
</script>

<UiDialog
	{open}
	title="Scan Directory"
	description="Choose a source, target path, and per-scan rules."
	wide
	onClose={onCloseOrCancel}
>
	<div class="flex flex-col gap-5">
		<section class="grid gap-3">
			<h3 class="text-xs font-semibold uppercase text-base-content/50">Location</h3>
			<div class="grid gap-3 md:grid-cols-2">
				<UiField label="Source directory">
					<input
						class="font-path input input-bordered input-sm w-full"
						type="text"
						value={source}
						placeholder="/path/to/directory"
						disabled={scanning}
						oninput={(event) => onSourceChange(inputValue(event))}
					/>
				</UiField>
				<UiField label="Place content under (virtual path)" hint='Use "/" for root, or e.g. "/photos/vacation" to nest'>
					<input
						class="font-path input input-bordered input-sm w-full"
						type="text"
						value={targetPath}
						placeholder="/"
						disabled={scanning}
						oninput={(event) => onTargetPathChange(inputValue(event))}
					/>
				</UiField>
			</div>
		</section>

		<section class="grid gap-3">
			<h3 class="text-xs font-semibold uppercase text-base-content/50">Presets</h3>
			<ScanPresetList {presets} disabled={scanning} onToggle={onPresetChange} />
		</section>

		<section class="grid gap-3">
			<h3 class="text-xs font-semibold uppercase text-base-content/50">Saved custom rules</h3>
			<CustomScanRuleList
				rules={customRules}
				activeRuleIds={activeCustomRuleIds}
				disabled={rulesDisabled}
				onToggle={onToggleCustomRule}
				onRemove={onRemoveCustomRule}
			/>
		</section>

		<section class="grid gap-3">
			<h3 class="text-xs font-semibold uppercase text-base-content/50">Add custom rule</h3>
			<CustomScanRuleEditor
				label={newRuleLabel}
				pattern={newRulePattern}
				action={newRuleAction}
				disabled={rulesDisabled}
				error={customRulesError}
				onLabelChange={onNewRuleLabelChange}
				onPatternChange={onNewRulePatternChange}
				onActionChange={onNewRuleActionChange}
				onAdd={onAddCustomRule}
			/>
		</section>

		{#if scanning}
			<section class="grid gap-3">
				<h3 class="text-xs font-semibold uppercase text-base-content/50">Progress</h3>
				<ScanProgressPanel {progress} starting={scanning && !progress} />
			</section>
		{/if}

		{#if scanError}
			<div class="alert alert-error py-2 text-sm" role="alert">
				<span>{scanError}</span>
			</div>
		{/if}
	</div>

	{#snippet actions()}
		<UiButton variant="ghost" disabled={cancelling} onclick={onCloseOrCancel}>
			{scanning ? (cancelling ? 'Cancelling...' : 'Cancel Scan') : 'Cancel'}
		</UiButton>
		<UiButton variant="primary" loading={scanning} disabled={startDisabled} onclick={onScan}>
			{scanning ? 'Scanning...' : 'Start Scan'}
		</UiButton>
	{/snippet}
</UiDialog>
