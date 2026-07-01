<script lang="ts">
	import { listDir, formatSize, type DirEntry } from '$lib/api/tauri';
	import TreeNode from './TreeNode.svelte';

	interface Props {
		entry: DirEntry;
		parentPath: string;
		selectedPath: string | null;
		onSelect: (path: string, entry: DirEntry) => void;
		onScanInto: (targetPath: string) => void;
	}

	let { entry, parentPath, selectedPath, onSelect, onScanInto }: Props = $props();

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

	function handleScanInto(e: MouseEvent) {
		e.stopPropagation();
		onScanInto(fullPath);
	}
</script>

<li class="list-none">
	<div class="group flex items-center gap-1">
		<button
			class={`min-w-0 flex flex-1 items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-base-200 ${isSelected ? 'bg-primary text-primary-content' : ''}`}
			type="button"
			onclick={toggle}
		>
			<span class="shrink-0 text-xs">{entry.is_dir ? (expanded ? '▾' : '▸') : '·'}</span>
			<span class="min-w-0 flex-1 truncate">{entry.name}</span>
			{#if !entry.is_dir}
				<span class="font-path shrink-0 text-[11px] opacity-60">{formatSize(entry.size)}</span>
			{/if}
			{#if loading}
				<span class="loading loading-spinner loading-xs"></span>
			{/if}
		</button>
		{#if entry.is_dir}
			<button
				class="btn btn-ghost btn-xs opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 focus:opacity-100"
				type="button"
				onclick={handleScanInto}
				title="Scan into this directory"
				aria-label={`Scan into ${entry.name}`}
			>
				+
			</button>
		{/if}
	</div>

	{#if expanded && children.length > 0}
		<ul class="pl-4">
			{#each children as child (child.name)}
				<TreeNode entry={child} parentPath={fullPath} {selectedPath} {onSelect} {onScanInto} />
			{/each}
		</ul>
	{/if}
</li>
