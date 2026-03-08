<script lang="ts">
	import { listDir, formatSize, type DirEntry } from '$lib/api/tauri';
	import TreeNode from './TreeNode.svelte';

	interface Props {
		entry: DirEntry;
		parentPath: string;
		selectedPath: string | null;
		onSelect: (path: string, entry: DirEntry) => void;
	}

	let { entry, parentPath, selectedPath, onSelect }: Props = $props();

	let expanded = $state(false);
	let children = $state<DirEntry[]>([]);
	let loading = $state(false);

	let fullPath = $derived(
		parentPath === '/' ? `/${entry.name}` : `${parentPath}/${entry.name}`
	);
	let isSelected = $derived(selectedPath === fullPath);

	async function toggle() {
		if (!entry.is_dir) {
			onSelect(fullPath, entry);
			return;
		}

		if (!expanded) {
			loading = true;
			try {
				children = await listDir(fullPath);
			} catch (e) {
				console.error('Failed to list dir:', e);
			}
			loading = false;
		}
		expanded = !expanded;
		onSelect(fullPath, entry);
	}
</script>

<li class="tree-node">
	<button
		class="node-button"
		class:selected={isSelected}
		class:directory={entry.is_dir}
		onclick={toggle}
	>
		<span class="icon">
			{#if entry.is_dir}
				{expanded ? '📂' : '📁'}
			{:else}
				📄
			{/if}
		</span>
		<span class="name">{entry.name}</span>
		{#if !entry.is_dir}
			<span class="size">{formatSize(entry.size)}</span>
		{/if}
		{#if loading}
			<span class="loading">...</span>
		{/if}
	</button>

	{#if expanded && children.length > 0}
		<ul class="children">
			{#each children as child (child.name)}
				<TreeNode
					entry={child}
					parentPath={fullPath}
					{selectedPath}
					{onSelect}
				/>
			{/each}
		</ul>
	{/if}
</li>

<style>
	.tree-node {
		list-style: none;
	}

	.node-button {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 4px 8px;
		border-radius: 4px;
		font-size: 13px;
		text-align: left;
		transition: background 0.1s;
	}

	.node-button:hover {
		background: var(--bg-hover);
	}

	.node-button.selected {
		background: var(--accent);
	}

	.icon {
		flex-shrink: 0;
		font-size: 14px;
	}

	.name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.size {
		flex-shrink: 0;
		color: var(--text-muted);
		font-size: 11px;
		font-family: var(--font-mono);
	}

	.loading {
		color: var(--text-muted);
		font-size: 11px;
	}

	.children {
		padding-left: 16px;
	}
</style>
