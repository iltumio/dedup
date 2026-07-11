<script lang="ts">
	import { goto } from '$app/navigation';
	import { UiButton, UiEmptyState, UiField } from '$lib/components/ui';
	import CustomScanRuleEditor from '$lib/components/scan/CustomScanRuleEditor.svelte';
	import CustomScanRuleList from '$lib/components/scan/CustomScanRuleList.svelte';
	import ScanPresetList from '$lib/components/scan/ScanPresetList.svelte';
	import ScanProgressPanel from '$lib/components/scan/ScanProgressPanel.svelte';
	import { pickDirectory } from '$lib/api/tauri';
	import { app } from '$lib/state/app.svelte';

	let presets = $derived([
		{
			id: 'git',
			label: 'Archive .git directories',
			description: 'Store repository metadata as bundled content.',
			checked: app.bundleGitDirs
		},
		{
			id: 'rust',
			label: 'Ignore Rust target directories',
			description: 'Skip Cargo build output directories named target.',
			checked: app.ignoreRustTarget
		},
		{
			id: 'node',
			label: 'Ignore node_modules directories',
			description: 'Skip installed JavaScript dependency trees.',
			checked: app.ignoreNodeModules
		},
		{
			id: 'python',
			label: 'Ignore Python virtual environments',
			description: 'Skip directories named .venv or venv.',
			checked: app.ignorePythonVenv
		}
	]);

	let rulesDisabled = $derived(app.scanning || app.savingCustomRules);
	let startDisabled = $derived(app.scanning || app.savingCustomRules || !app.scanSource.trim());

	function inputValue(event: Event) {
		return (event.currentTarget as HTMLInputElement).value;
	}

	async function browseSource() {
		if (app.scanning) return;
		try {
			const dir = await pickDirectory('Select source directory');
			if (dir) app.scanSource = dir;
		} catch (e) {
			app.scanError = String(e);
		}
	}

	async function handleCloseOrCancel() {
		if (app.scanning) {
			await app.requestCancel();
			return;
		}
		goto('/');
	}

	async function startScan() {
		const finished = await app.runScan();
		if (finished) goto('/');
	}
</script>

<div class="flex h-screen min-h-0 flex-col bg-base-100 text-base-content" data-theme="night">
	<header class="flex min-h-12 shrink-0 items-center gap-3 border-b border-base-300 bg-base-200 px-3">
		<button
			class="btn btn-ghost btn-sm shrink-0"
			type="button"
			disabled={app.scanning}
			onclick={() => goto('/')}
			aria-label="Back to files"
		>
			← Back
		</button>
		<div class="min-w-0">
			<h1 class="truncate text-sm font-bold">Scan Directory</h1>
			<p class="truncate text-xs text-base-content/60">
				Choose a source, target path, and per-scan rules.
			</p>
		</div>
	</header>

	<main class="min-h-0 flex-1 overflow-y-auto">
		{#if !app.hasWorkspace}
			<UiEmptyState title="No workspace" message="Create or import a workspace before scanning.">
				{#snippet actions()}
					<UiButton variant="primary" onclick={() => goto('/')}>Back to workspaces</UiButton>
				{/snippet}
			</UiEmptyState>
		{:else}
			<div class="mx-auto flex min-w-0 max-w-4xl flex-col gap-5 p-4">
				<section class="grid min-w-0 gap-3">
					<h3 class="text-xs font-semibold uppercase text-base-content/50">Location</h3>
					<div class="grid gap-3 md:grid-cols-2">
						<UiField label="Source directory">
							<div class="flex gap-2">
								<input
									class="font-path input input-bordered input-sm min-w-0 flex-1"
									type="text"
									value={app.scanSource}
									placeholder="/path/to/directory"
									disabled={app.scanning}
									oninput={(event) => (app.scanSource = inputValue(event))}
								/>
								<UiButton variant="secondary" disabled={app.scanning} onclick={browseSource}>
									Browse
								</UiButton>
							</div>
						</UiField>
						<UiField label="Place content under (virtual path)" hint='Use "/" for root, or e.g. "/photos/vacation" to nest'>
							<input
								class="font-path input input-bordered input-sm w-full"
								type="text"
								value={app.targetPath}
								placeholder="/"
								disabled={app.scanning}
								oninput={(event) => (app.targetPath = inputValue(event))}
							/>
						</UiField>
					</div>
				</section>

				<section class="grid min-w-0 gap-3">
					<h3 class="text-xs font-semibold uppercase text-base-content/50">Presets</h3>
					<ScanPresetList {presets} disabled={app.scanning} onToggle={app.handlePresetChange} />
				</section>

				<section class="grid min-w-0 gap-3">
					<h3 class="text-xs font-semibold uppercase text-base-content/50">Saved custom rules</h3>
					<CustomScanRuleList
						rules={app.customScanRules}
						activeRuleIds={app.activeCustomRuleIds}
						disabled={rulesDisabled}
						onToggle={app.toggleCustomRule}
						onRemove={app.removeCustomRule}
					/>
				</section>

				<section class="grid min-w-0 gap-3">
					<h3 class="text-xs font-semibold uppercase text-base-content/50">Add custom rule</h3>
					<CustomScanRuleEditor
						label={app.newRuleLabel}
						pattern={app.newRulePattern}
						action={app.newRuleAction}
						disabled={rulesDisabled}
						error={app.customRulesError}
						onLabelChange={(value) => (app.newRuleLabel = value)}
						onPatternChange={(value) => (app.newRulePattern = value)}
						onActionChange={(value) => (app.newRuleAction = value)}
						onAdd={app.addCustomRule}
					/>
				</section>

				{#if app.scanning}
					<section class="grid min-w-0 gap-3">
						<h3 class="text-xs font-semibold uppercase text-base-content/50">Progress</h3>
						<ScanProgressPanel progress={app.progress} starting={app.scanning && !app.progress} />
					</section>
				{/if}

				{#if app.scanError}
					<div class="alert alert-error py-2 text-sm" role="alert">
						<span>{app.scanError}</span>
					</div>
				{/if}
			</div>
		{/if}
	</main>

	{#if app.hasWorkspace}
		<footer class="flex flex-wrap items-center justify-end gap-2 border-t border-base-300 bg-base-200 px-4 py-3">
			<UiButton variant="ghost" disabled={app.cancelling} onclick={handleCloseOrCancel}>
				{app.scanning ? (app.cancelling ? 'Cancelling...' : 'Cancel Scan') : 'Cancel'}
			</UiButton>
			<UiButton variant="primary" loading={app.scanning} disabled={startDisabled} onclick={startScan}>
				{app.scanning ? 'Scanning...' : 'Start Scan'}
			</UiButton>
		</footer>
	{/if}
</div>
