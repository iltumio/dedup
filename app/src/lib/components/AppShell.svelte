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
	<header class="flex h-12 shrink-0 items-center gap-3 border-b border-base-300 bg-base-200 px-3">
		<h1 class="text-sm font-bold">dedup</h1>

		<div class="min-w-0">{@render workspaceControl?.()}</div>

		{#if hasWorkspace}
			<UiSegmentedControl
				ariaLabel="Primary view"
				value={currentView}
				options={[
					{ value: 'files', label: 'Files' },
					{ value: 'stats', label: 'Stats' }
				]}
				onChange={onViewChange}
			/>
		{/if}

		{#if stats.length > 0}
			<div class="ml-auto hidden min-w-0 grid-cols-3 gap-2 lg:grid">
				{#each stats as stat}
					<UiStat label={stat.label} value={stat.value} tone={stat.tone} />
				{/each}
			</div>
		{:else}
			<div class="ml-auto"></div>
		{/if}

		<UiButton variant="primary" disabled={!hasWorkspace || scanning} loading={scanning} onclick={onScan}>
			Scan
		</UiButton>
	</header>

	<main class="min-h-0 flex-1 overflow-hidden">
		{@render children?.()}
	</main>
</div>
