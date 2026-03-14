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

<div class="stats-page">
	<div class="stats-header">
		<h2>Extension Analytics</h2>
		<button class="refresh-btn" onclick={loadStats} disabled={loading}>
			{loading ? 'Loading...' : 'Refresh'}
		</button>
	</div>

	{#if error}
		<div class="error">{error}</div>
	{:else if loading}
		<div class="loading">Loading stats...</div>
	{:else if stats.length === 0}
		<div class="empty">No files in store yet. Scan a directory first.</div>
	{:else}
		<!-- Summary cards -->
		<div class="summary-cards">
			<div class="card">
				<span class="card-value">{totalFiles}</span>
				<span class="card-label">Total Files</span>
			</div>
			<div class="card">
				<span class="card-value highlight">{totalDups}</span>
				<span class="card-label">Duplicate Files</span>
			</div>
			<div class="card">
				<span class="card-value saved">{formatSize(totalSaved)}</span>
				<span class="card-label">Space Saved</span>
			</div>
			<div class="card">
				<span class="card-value">{stats.length}</span>
				<span class="card-label">Extensions</span>
			</div>
		</div>

		<!-- Sort tabs -->
		<div class="sort-tabs">
			<button class:active={sortBy === 'saved'} onclick={() => (sortBy = 'saved')}>
				Most Space Saved
			</button>
			<button class:active={sortBy === 'dup_pct'} onclick={() => (sortBy = 'dup_pct')}>
				Highest Dup %
			</button>
			<button class:active={sortBy === 'dup_count'} onclick={() => (sortBy = 'dup_count')}>
				Most Duplicates
			</button>
			<button class:active={sortBy === 'files'} onclick={() => (sortBy = 'files')}>
				Most Files
			</button>
		</div>

		<!-- Ranking table -->
		<div class="ranking">
			{#each currentList as item, i (item.extension)}
				<div class="rank-row">
					<span class="rank-num">#{i + 1}</span>
					<span class="rank-ext">.{item.extension}</span>
					<div class="rank-bar-container">
						<div
							class="rank-bar"
							class:bar-saved={sortBy === 'saved'}
							class:bar-dup={sortBy === 'dup_pct' || sortBy === 'dup_count'}
							class:bar-files={sortBy === 'files'}
							style="width: {barWidth(barValue(item), currentMax)}"
						></div>
					</div>
					<span class="rank-value">{formatValue(item)}</span>
					<div class="rank-detail">
						<span>{item.total_files} files</span>
						<span class="sep">·</span>
						<span class="highlight">{item.duplicate_files} dups ({item.duplicate_pct}%)</span>
						<span class="sep">·</span>
						<span class="saved">{formatSize(item.bytes_saved)} saved</span>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.stats-page {
		height: 100%;
		overflow-y: auto;
		padding: 20px 24px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.stats-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.stats-header h2 {
		font-size: 16px;
		font-weight: 600;
	}

	.refresh-btn {
		padding: 5px 12px;
		font-size: 12px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 5px;
		cursor: pointer;
		color: var(--text-muted);
	}

	.refresh-btn:hover {
		border-color: var(--accent);
		color: var(--text);
	}

	.error {
		color: var(--duplicate);
		font-size: 13px;
	}

	.loading,
	.empty {
		color: var(--text-muted);
		font-size: 13px;
		text-align: center;
		padding: 40px 0;
	}

	/* Summary cards */
	.summary-cards {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 10px;
	}

	.card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 14px 8px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 8px;
	}

	.card-value {
		font-size: 18px;
		font-weight: 700;
		font-family: var(--font-mono);
	}

	.card-label {
		font-size: 11px;
		color: var(--text-muted);
	}

	/* Sort tabs */
	.sort-tabs {
		display: flex;
		gap: 4px;
		border-bottom: 1px solid var(--border);
		padding-bottom: 8px;
	}

	.sort-tabs button {
		padding: 5px 12px;
		font-size: 12px;
		border-radius: 4px;
		background: none;
		border: 1px solid transparent;
		color: var(--text-muted);
		cursor: pointer;
		transition: all 0.15s;
	}

	.sort-tabs button:hover {
		color: var(--text);
	}

	.sort-tabs button.active {
		background: var(--accent);
		color: var(--text);
		font-weight: 500;
	}

	/* Ranking */
	.ranking {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.rank-row {
		display: grid;
		grid-template-columns: 32px 64px 1fr auto;
		grid-template-rows: auto auto;
		gap: 0 10px;
		align-items: center;
		padding: 8px 10px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
	}

	.rank-num {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		grid-row: 1 / 3;
	}

	.rank-ext {
		font-size: 14px;
		font-weight: 600;
		font-family: var(--font-mono);
	}

	.rank-bar-container {
		height: 6px;
		background: var(--border);
		border-radius: 3px;
		overflow: hidden;
	}

	.rank-bar {
		height: 100%;
		border-radius: 3px;
		transition: width 0.3s;
	}

	.rank-bar.bar-saved {
		background: var(--success);
	}

	.rank-bar.bar-dup {
		background: var(--duplicate);
	}

	.rank-bar.bar-files {
		background: var(--accent-light);
	}

	.rank-value {
		font-size: 13px;
		font-weight: 600;
		font-family: var(--font-mono);
		text-align: right;
		white-space: nowrap;
	}

	.rank-detail {
		grid-column: 2 / -1;
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		display: flex;
		gap: 6px;
	}

	.highlight {
		color: var(--duplicate);
		font-weight: 600;
	}

	.saved {
		color: var(--success);
		font-weight: 600;
	}

	.sep {
		opacity: 0.4;
	}
</style>
