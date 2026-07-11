<script lang="ts">
	import { formatSize, type ScanProgress } from '$lib/api/tauri';
	import { UiStat } from '$lib/components/ui';

	interface Props {
		progress: ScanProgress | null;
		starting: boolean;
	}

	let { progress, starting }: Props = $props();

	let savedBytes = $derived(progress ? progress.bytes_processed - progress.bytes_stored : 0);
</script>

{#if progress}
	<div class="min-w-0 rounded-box border border-base-300 bg-base-100 p-3">
		<div class="grid min-w-0 gap-2 sm:grid-cols-2 lg:grid-cols-5">
			<UiStat label="Files" value={progress.files_processed} />
			<UiStat label="Processed" value={formatSize(progress.bytes_processed)} />
			<UiStat label="Stored" value={formatSize(progress.bytes_stored)} />
			<UiStat label="Duplicates" value={progress.duplicates_found} tone="error" />
			<UiStat label="Saved" value={formatSize(savedBytes)} tone="success" />
		</div>
		<div class="font-path mt-3 min-w-0 truncate text-xs text-base-content/60" title={progress.current_file}>
			{progress.current_file}
		</div>
	</div>
{:else if starting}
	<div class="min-w-0 rounded-box border border-base-300 bg-base-100 p-3">
		<progress class="progress progress-primary w-full"></progress>
		<div class="font-path mt-3 text-xs text-base-content/60">Starting scan...</div>
	</div>
{/if}
