<script lang="ts">
	import type { Snippet } from 'svelte';
	import { UiButton, UiSegmentedControl, UiStat } from '$lib/components/ui';

	type View = 'files' | 'stats';

	interface ShellStat {
		label: string;
		value: string | number;
		tone?: 'default' | 'success' | 'error' | 'info' | 'warning';
	}

	interface Props {
		currentView: View;
		hasWorkspace: boolean;
		scanning: boolean;
		stats: ShellStat[];
		onViewChange: (view: View) => void;
		onScan: () => void;
		workspaceControl?: Snippet;
		children?: Snippet;
	}

	let {
		currentView,
		hasWorkspace,
		scanning,
		stats,
		onViewChange,
		onScan,
		workspaceControl,
		children
	}: Props = $props();
</script>

<div class="flex h-screen min-h-0 flex-col bg-base-100 text-base-content" data-theme="night">
	<header class="flex min-h-12 shrink-0 items-center gap-3 border-b border-base-300 bg-base-200 px-3">
		<h1 class="shrink-0 text-sm font-bold">dedup</h1>

		<div class="min-w-0 max-w-72 flex-1">{@render workspaceControl?.()}</div>

		{#if hasWorkspace}
			<div class="shrink-0">
				<UiSegmentedControl
					ariaLabel="Primary view"
					value={currentView}
					options={[
						{ value: 'files', label: 'Files' },
						{ value: 'stats', label: 'Stats' }
					]}
					onChange={onViewChange}
				/>
			</div>
		{/if}

		<div class="min-w-0 flex-1"></div>

		{#if stats.length > 0}
			<div class="hidden min-w-0 shrink grid-cols-3 gap-2 lg:grid">
				{#each stats as stat}
					<UiStat label={stat.label} value={stat.value} tone={stat.tone} />
				{/each}
			</div>
		{/if}

		<UiButton
			class="shrink-0"
			variant="primary"
			disabled={!hasWorkspace || scanning}
			loading={scanning}
			onclick={onScan}
		>
			Scan
		</UiButton>
	</header>

	<main class="flex min-h-0 flex-1 overflow-hidden">
		{@render children?.()}
	</main>
</div>
