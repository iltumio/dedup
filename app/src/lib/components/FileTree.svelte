<script lang="ts">
	import { listDir, type DirEntry } from '$lib/api/tauri';
	import TreeNode from './TreeNode.svelte';

	interface Props {
		selectedPath: string | null;
		onSelect: (path: string, entry: DirEntry) => void;
	}

	let { selectedPath, onSelect }: Props = $props();

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

<div class="file-tree">
	<div class="header">
		<span class="title">Files</span>
		<button class="refresh" onclick={loadRoot} title="Refresh">⟳</button>
	</div>

	{#if error}
		<div class="error">{error}</div>
	{:else if rootEntries.length === 0}
		<div class="empty">No files. Scan a directory first.</div>
	{:else}
		<ul class="tree-root">
			{#each rootEntries as entry (entry.name)}
				<TreeNode
					{entry}
					parentPath="/"
					{selectedPath}
					{onSelect}
				/>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.file-tree {
		height: 100%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
	}

	.title {
		font-weight: 600;
		font-size: 14px;
	}

	.refresh {
		font-size: 16px;
		padding: 4px;
		border-radius: 4px;
	}

	.refresh:hover {
		background: var(--bg-hover);
	}

	.tree-root {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}

	.error {
		padding: 16px;
		color: var(--duplicate);
		font-size: 13px;
	}

	.empty {
		padding: 16px;
		color: var(--text-muted);
		font-size: 13px;
	}
</style>
