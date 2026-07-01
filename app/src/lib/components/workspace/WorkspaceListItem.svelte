<script lang="ts">
	import { formatSize, type Workspace } from '$lib/api/tauri';
	import { UiBadge, UiButton } from '$lib/components/ui';

	interface Props {
		workspace: Workspace;
		active: boolean;
		onSelect: (id: string) => void;
		onDelete: (id: string) => void;
	}

	let { workspace, active, onSelect, onDelete }: Props = $props();

	let savedBytes = $derived(workspace.stats.total_original_bytes - workspace.stats.total_stored_bytes);
</script>

<li class={`rounded-box border ${active ? 'border-primary bg-primary/10' : 'border-base-300 bg-base-100'}`}>
	<div class="flex items-center gap-2 p-3">
		<button class="min-w-0 flex-1 text-left" type="button" onclick={() => onSelect(workspace.id)}>
			<div class="flex min-w-0 items-center gap-2">
				<span class="truncate text-sm font-semibold">{workspace.label}</span>
				{#each workspace.tags as tag}
					<UiBadge text={tag} />
				{/each}
			</div>
			<div class="font-path mt-1 truncate text-xs text-base-content/50">{workspace.store_path}</div>
			<div class="font-path mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-base-content/60">
				<span>{workspace.stats.total_files} files</span>
				<span class="text-error">{workspace.stats.duplicate_files} dups</span>
				<span class="text-success">saved {formatSize(savedBytes)}</span>
				<span>{workspace.stats.scans_count} scans</span>
			</div>
		</button>
		<UiButton
			variant="destructive"
			size="xs"
			title="Delete workspace"
			ariaLabel={`Delete ${workspace.label}`}
			onclick={() => onDelete(workspace.id)}
		>
			Delete
		</UiButton>
	</div>
</li>
