<script lang="ts">
	import {
		getExtensionStats,
		formatSize,
		type ExtensionStats
	} from '$lib/api/tauri';

	let stats = $state<ExtensionStats[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let sortBy = $state<'saved' | 'dup_pct' | 'dup_count' | 'files'>('saved');

	$effect(() => {
		loadStats();
	});

	async function loadStats() {
		loading = true;
		error = null;
		try {
			stats = await getExtensionStats();
		} catch (e) {
			error = String(e);
		}
		loading = false;
	}

	let sortedBySaved = $derived(
		[...stats].sort((a, b) => b.bytes_saved - a.bytes_saved)
	);

	let sortedByDupPct = $derived(
		[...stats]
			.filter((s) => s.total_files >= 2)
			.sort((a, b) => b.duplicate_pct - a.duplicate_pct)
	);

	let sortedByDupCount = $derived(
		[...stats].sort((a, b) => b.duplicate_files - a.duplicate_files)
	);

	let sortedByFiles = $derived(
		[...stats].sort((a, b) => b.total_files - a.total_files)
	);

	let currentList = $derived(
		sortBy === 'saved'
			? sortedBySaved
			: sortBy === 'dup_pct'
				? sortedByDupPct
				: sortBy === 'dup_count'
					? sortedByDupCount
					: sortedByFiles
	);

	let totalSaved = $derived(stats.reduce((s, e) => s + e.bytes_saved, 0));
	let totalDups = $derived(stats.reduce((s, e) => s + e.duplicate_files, 0));
	let totalFiles = $derived(stats.reduce((s, e) => s + e.total_files, 0));

	function barWidth(value: number, max: number): string {
		if (max === 0) return '0%';
		return `${Math.min((value / max) * 100, 100)}%`;
	}

	let maxSaved = $derived(
		stats.length > 0 ? Math.max(...stats.map((s) => s.bytes_saved)) : 1
	);
	let maxDupPct = $derived(100);
	let maxDupCount = $derived(
		stats.length > 0 ? Math.max(...stats.map((s) => s.duplicate_files)) : 1
	);
	let maxFiles = $derived(
		stats.length > 0 ? Math.max(...stats.map((s) => s.total_files)) : 1
	);

	let currentMax = $derived(
		sortBy === 'saved'
			? maxSaved
			: sortBy === 'dup_pct'
				? maxDupPct
				: sortBy === 'dup_count'
					? maxDupCount
					: maxFiles
	);

	function barValue(item: ExtensionStats): number {
		if (sortBy === 'saved') return item.bytes_saved;
		if (sortBy === 'dup_pct') return item.duplicate_pct;
		if (sortBy === 'dup_count') return item.duplicate_files;
		return item.total_files;
	}

	function formatValue(item: ExtensionStats): string {
		if (sortBy === 'saved') return formatSize(item.bytes_saved);
		if (sortBy === 'dup_pct') return `${item.duplicate_pct}%`;
		if (sortBy === 'dup_count') return `${item.duplicate_files}`;
		return `${item.total_files}`;
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<header class="flex h-10 shrink-0 items-center justify-between border-b border-base-300 px-4">
		<h2 class="text-sm font-semibold">Extension Analytics</h2>
		<button class="btn btn-ghost btn-xs" type="button" onclick={loadStats} disabled={loading}>
			{#if loading}
				<span class="loading loading-spinner loading-xs"></span>
				Loading...
			{:else}
				Refresh
			{/if}
		</button>
	</header>

	<div class="min-h-0 flex-1 overflow-y-auto p-4">
		{#if error}
			<div class="alert alert-error">
				<span class="break-all text-sm">{error}</span>
			</div>
		{:else if loading}
			<div
				class="rounded-box flex items-center justify-center gap-2 border border-base-300 bg-base-200 p-10 text-sm text-base-content/60"
			>
				<span class="loading loading-spinner loading-sm"></span>
				Loading stats...
			</div>
		{:else if stats.length === 0}
			<div
				class="rounded-box border border-base-300 bg-base-200 p-4 text-sm text-base-content/60"
			>
				No files in store yet. Scan a directory first.
			</div>
		{:else}
			<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="font-path text-lg font-bold">{totalFiles}</div>
					<div class="mt-1 text-xs text-base-content/50">Total Files</div>
				</div>
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="font-path text-lg font-bold text-error">{totalDups}</div>
					<div class="mt-1 text-xs text-base-content/50">Duplicate Files</div>
				</div>
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="font-path text-lg font-bold text-success">{formatSize(totalSaved)}</div>
					<div class="mt-1 text-xs text-base-content/50">Space Saved</div>
				</div>
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="font-path text-lg font-bold">{stats.length}</div>
					<div class="mt-1 text-xs text-base-content/50">Extensions</div>
				</div>
			</div>

			<div class="mt-4 flex flex-wrap gap-2 border-b border-base-300 pb-3">
				<button
					class={sortBy === 'saved' ? 'btn btn-primary btn-xs' : 'btn btn-ghost btn-xs'}
					type="button"
					onclick={() => (sortBy = 'saved')}
				>
					Most Space Saved
				</button>
				<button
					class={sortBy === 'dup_pct' ? 'btn btn-primary btn-xs' : 'btn btn-ghost btn-xs'}
					type="button"
					onclick={() => (sortBy = 'dup_pct')}
				>
					Highest Dup %
				</button>
				<button
					class={sortBy === 'dup_count' ? 'btn btn-primary btn-xs' : 'btn btn-ghost btn-xs'}
					type="button"
					onclick={() => (sortBy = 'dup_count')}
				>
					Most Duplicates
				</button>
				<button
					class={sortBy === 'files' ? 'btn btn-primary btn-xs' : 'btn btn-ghost btn-xs'}
					type="button"
					onclick={() => (sortBy = 'files')}
				>
					Most Files
				</button>
			</div>

			<div class="mt-4 space-y-2">
				{#each currentList as item, i (item.extension)}
					<div class="rounded-box border border-base-300 bg-base-200 p-3">
						<div
							class="grid gap-2 md:grid-cols-[3rem_minmax(6rem,12rem)_minmax(10rem,1fr)_auto] md:items-center"
						>
							<span class="font-path text-xs font-semibold text-base-content/50">#{i + 1}</span>
							<span class="font-path min-w-0 break-all text-sm font-semibold">
								.{item.extension}
							</span>
							<div class="h-2 overflow-hidden rounded-full bg-base-300">
								{#if sortBy === 'saved'}
									<div
										class="h-full rounded-full bg-success transition-[width]"
										style="width: {barWidth(barValue(item), currentMax)}"
									></div>
								{:else if sortBy === 'dup_pct' || sortBy === 'dup_count'}
									<div
										class="h-full rounded-full bg-error transition-[width]"
										style="width: {barWidth(barValue(item), currentMax)}"
									></div>
								{:else}
									<div
										class="h-full rounded-full bg-info transition-[width]"
										style="width: {barWidth(barValue(item), currentMax)}"
									></div>
								{/if}
							</div>
							<span class="font-path whitespace-nowrap text-right text-sm font-semibold">
								{formatValue(item)}
							</span>
							<div
								class="font-path flex min-w-0 flex-wrap gap-x-2 gap-y-1 text-xs text-base-content/60 md:col-start-2 md:col-end-5"
							>
								<span>{item.total_files} files</span>
								<span class="opacity-40">·</span>
								<span class="font-semibold text-error">
									{item.duplicate_files} dups ({item.duplicate_pct}%)
								</span>
								<span class="opacity-40">·</span>
								<span class="font-semibold text-success">
									{formatSize(item.bytes_saved)} saved
								</span>
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
