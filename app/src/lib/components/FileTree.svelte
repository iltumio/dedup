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

<div class="file-tree">
	<div class="header">
		<span class="title">Files</span>
		<div class="header-actions">
			<button class="action-btn" onclick={() => onScanInto('/')} title="Scan into root">+</button>
			<button class="action-btn" onclick={loadRoot} title="Refresh">⟳</button>
		</div>
	</div>

	{#if error}
		<div class="error">{error}</div>
	{:else if rootEntries.length === 0}
		<div class="empty">
			<p>No files yet.</p>
			<button class="empty-scan-btn" onclick={() => onScanInto('/')}>Scan a directory</button>
		</div>
	{:else}
		<ul class="tree-root">
			{#each rootEntries as entry (entry.name)}
				<TreeNode
					{entry}
					parentPath="/"
					{selectedPath}
					{onSelect}
					{onScanInto}
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

	.header-actions {
		display: flex;
		gap: 4px;
	}

	.action-btn {
		font-size: 16px;
		padding: 4px 6px;
		border-radius: 4px;
		line-height: 1;
	}

	.action-btn:hover {
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
		display: flex;
		flex-direction: column;
		gap: 12px;
		align-items: flex-start;
	}

	.empty-scan-btn {
		padding: 6px 12px;
		background: var(--accent);
		border-radius: 6px;
		font-size: 12px;
		font-weight: 500;
	}

	.empty-scan-btn:hover {
		background: var(--accent-light);
	}
</style>
