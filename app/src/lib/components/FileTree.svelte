<script lang="ts">
	import { listDir, type DirEntry } from '$lib/api/tauri';
	import TreeNode from './TreeNode.svelte';

	interface Props {
		selectedPath: string | null;
		onSelect: (path: string, entry: DirEntry) => void;
		onScanInto: (targetPath: string) => void;
	}

	let { selectedPath, onSelect, onScanInto }: Props = $props();

	let rootEntries = $state<DirEntry[]>([]);
	let error = $state<string | null>(null);

	async function loadRoot() {
		try {
			rootEntries = await listDir('/');
			error = null;
		} catch (e) {
			error = String(e);
			rootEntries = [];
		}
	}

	$effect(() => {
		loadRoot();
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<header class="flex h-10 shrink-0 items-center justify-between border-b border-base-300 px-3">
		<span class="text-sm font-semibold">Files</span>
		<div class="flex gap-1">
			<button
				class="btn btn-ghost btn-xs"
				type="button"
				onclick={() => onScanInto('/')}
				title="Scan into root"
				aria-label="Scan into root"
			>
				+
			</button>
			<button
				class="btn btn-ghost btn-xs"
				type="button"
				onclick={loadRoot}
				title="Refresh files"
			>
				Refresh
			</button>
		</div>
	</header>

	{#if error}
		<div class="alert alert-error m-3 py-2 text-sm">{error}</div>
	{:else if rootEntries.length === 0}
		<div class="p-3">
			<div class="rounded-box border border-base-300 bg-base-200 p-4">
				<p class="text-sm text-base-content/60">No files yet.</p>
				<button
					class="btn btn-primary btn-sm mt-3"
					type="button"
					onclick={() => onScanInto('/')}
				>
					Scan a directory
				</button>
			</div>
		</div>
	{:else}
		<ul class="min-h-0 flex-1 overflow-y-auto p-2">
			{#each rootEntries as entry (entry.name)}
				<TreeNode {entry} parentPath="/" {selectedPath} {onSelect} {onScanInto} />
			{/each}
		</ul>
	{/if}
</div>
