<script lang="ts">
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import { scanDirectory, findAllDuplicates, formatSize, type DirEntry, type ScanStats } from '$lib/api/tauri';

	let selectedPath = $state<string | null>(null);
	let selectedEntry = $state<DirEntry | null>(null);
	let scanSource = $state('');
	let storePath = $state('.store');
	let scanning = $state(false);
	let scanResult = $state<ScanStats | null>(null);
	let scanError = $state<string | null>(null);
	let showScanDialog = $state(false);

	function handleSelect(path: string, entry: DirEntry) {
		selectedPath = path;
		selectedEntry = entry;
	}

	async function handleScan() {
		if (!scanSource.trim()) return;
		scanning = true;
		scanError = null;
		scanResult = null;

		try {
			scanResult = await scanDirectory(scanSource, storePath);
			showScanDialog = false;
		} catch (e) {
			scanError = String(e);
		}
		scanning = false;
	}
</script>

<div class="app">
	<header class="toolbar">
		<h1 class="logo">dedup</h1>
		<button class="scan-btn" onclick={() => (showScanDialog = !showScanDialog)}>
			Scan Directory
		</button>
		{#if scanResult}
			<div class="scan-stats">
				<span>{scanResult.total_files} files</span>
				<span class="sep">·</span>
				<span>{scanResult.unique_blobs} unique</span>
				<span class="sep">·</span>
				<span class="highlight">{scanResult.duplicate_files} duplicates</span>
				<span class="sep">·</span>
				<span>saved {((1 - scanResult.total_stored_bytes / scanResult.total_original_bytes) * 100).toFixed(1)}%</span>
			</div>
		{/if}
	</header>

	{#if showScanDialog}
		<div class="scan-dialog">
			<div class="dialog-content">
				<h2>Scan Directory</h2>
				<label>
					<span>Source directory</span>
					<input
						type="text"
						bind:value={scanSource}
						placeholder="/path/to/directory"
					/>
				</label>
				<label>
					<span>Store location</span>
					<input type="text" bind:value={storePath} />
				</label>
				{#if scanError}
					<div class="error">{scanError}</div>
				{/if}
				<div class="dialog-actions">
					<button class="cancel" onclick={() => (showScanDialog = false)}>Cancel</button>
					<button class="primary" onclick={handleScan} disabled={scanning}>
						{scanning ? 'Scanning...' : 'Start Scan'}
					</button>
				</div>
			</div>
		</div>
	{/if}

	<main class="content">
		<aside class="sidebar">
			<FileTree {selectedPath} onSelect={handleSelect} />
		</aside>
		<section class="details">
			{#if selectedPath && selectedEntry}
				<FileDetails path={selectedPath} entry={selectedEntry} />
			{:else}
				<div class="placeholder">
					<p>Select a file to view details</p>
				</div>
			{/if}
		</section>
	</main>
</div>

<style>
	.app {
		display: flex;
		flex-direction: column;
		height: 100vh;
	}

	.toolbar {
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 8px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.logo {
		font-size: 16px;
		font-weight: 700;
		letter-spacing: -0.5px;
	}

	.scan-btn {
		padding: 6px 14px;
		background: var(--accent);
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		transition: background 0.15s;
	}

	.scan-btn:hover {
		background: var(--accent-light);
	}

	.scan-stats {
		display: flex;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.scan-stats .highlight {
		color: var(--duplicate);
		font-weight: 600;
	}

	.sep {
		opacity: 0.4;
	}

	.content {
		display: flex;
		flex: 1;
		overflow: hidden;
	}

	.sidebar {
		width: 320px;
		border-right: 1px solid var(--border);
		overflow: hidden;
	}

	.details {
		flex: 1;
		overflow: hidden;
	}

	.placeholder {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--text-muted);
		font-size: 14px;
	}

	.scan-dialog {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}

	.dialog-content {
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 24px;
		width: 400px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.dialog-content h2 {
		font-size: 16px;
		font-weight: 600;
	}

	.dialog-content label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 13px;
	}

	.dialog-content label span {
		color: var(--text-muted);
	}

	.dialog-content input {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 8px 12px;
		font-size: 13px;
		font-family: var(--font-mono);
	}

	.dialog-content input:focus {
		outline: none;
		border-color: var(--accent-light);
	}

	.dialog-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}

	.dialog-actions button {
		padding: 8px 16px;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
	}

	.cancel {
		background: var(--bg);
		border: 1px solid var(--border);
	}

	.primary {
		background: var(--accent);
	}

	.primary:hover {
		background: var(--accent-light);
	}

	.primary:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		color: var(--duplicate);
		font-size: 13px;
	}
</style>
