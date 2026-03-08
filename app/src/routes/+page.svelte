<script lang="ts">
	import FileTree from '$lib/components/FileTree.svelte';
	import FileDetails from '$lib/components/FileDetails.svelte';
	import {
		scanDirectory,
		onScanProgress,
		formatSize,
		type DirEntry,
		type ScanStats,
		type ScanProgress
	} from '$lib/api/tauri';
	import type { UnlistenFn } from '@tauri-apps/api/event';

	let selectedPath = $state<string | null>(null);
	let selectedEntry = $state<DirEntry | null>(null);
	let scanSource = $state('');
	let storePath = $state('.store');
	let targetPath = $state('/');
	let scanning = $state(false);
	let scanResult = $state<ScanStats | null>(null);
	let scanError = $state<string | null>(null);
	let showScanDialog = $state(false);
	let progress = $state<ScanProgress | null>(null);
	let treeRefreshKey = $state(0);

	function handleSelect(path: string, entry: DirEntry) {
		selectedPath = path;
		selectedEntry = entry;
	}

	function openScanDialog(presetTarget?: string) {
		targetPath = presetTarget ?? '/';
		scanError = null;
		progress = null;
		showScanDialog = true;
	}

	async function handleScan() {
		if (!scanSource.trim()) return;
		scanning = true;
		scanError = null;
		progress = null;

		let unlisten: UnlistenFn | null = null;

		try {
			unlisten = await onScanProgress((p) => {
				progress = p;
			});

			scanResult = await scanDirectory(scanSource, storePath, targetPath);
			showScanDialog = false;
			treeRefreshKey++;
		} catch (e) {
			scanError = String(e);
		} finally {
			unlisten?.();
			scanning = false;
		}
	}

	let savedBytes = $derived(
		scanResult ? scanResult.total_original_bytes - scanResult.total_stored_bytes : 0
	);
	let savedPct = $derived(
		scanResult && scanResult.total_original_bytes > 0
			? ((1 - scanResult.total_stored_bytes / scanResult.total_original_bytes) * 100).toFixed(1)
			: '0.0'
	);
</script>

<div class="app">
	<header class="toolbar">
		<h1 class="logo">dedup</h1>
		<button class="scan-btn" onclick={() => openScanDialog()}>
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
				<span class="saved">saved {formatSize(savedBytes)} ({savedPct}%)</span>
			</div>
		{/if}
	</header>

	{#if showScanDialog}
		<div class="scan-dialog" role="dialog">
			<div class="dialog-content">
				<h2>Scan Directory</h2>
				<label>
					<span>Source directory</span>
					<input
						type="text"
						bind:value={scanSource}
						placeholder="/path/to/directory"
						disabled={scanning}
					/>
				</label>
				<label>
					<span>Place content under (virtual path)</span>
					<input
						type="text"
						bind:value={targetPath}
						placeholder="/"
						disabled={scanning}
					/>
					<span class="hint">Use "/" for root, or e.g. "/photos/vacation" to nest</span>
				</label>
				<label>
					<span>Store location</span>
					<input type="text" bind:value={storePath} disabled={scanning} />
				</label>

				{#if scanning && progress}
					<div class="progress-section">
						<div class="progress-bar-track">
							<div class="progress-bar-fill" style="width: 100%"></div>
							<!-- indeterminate since we don't know total files in advance -->
						</div>
						<div class="progress-stats">
							<div class="stat-row">
								<span class="stat-label">Files</span>
								<span class="stat-value">{progress.files_processed}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Processed</span>
								<span class="stat-value">{formatSize(progress.bytes_processed)}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Stored</span>
								<span class="stat-value">{formatSize(progress.bytes_stored)}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Duplicates</span>
								<span class="stat-value highlight">{progress.duplicates_found}</span>
							</div>
							{#if progress.bytes_processed > 0}
								<div class="stat-row">
									<span class="stat-label">Space saved</span>
									<span class="stat-value saved">
										{formatSize(progress.bytes_processed - progress.bytes_stored)}
									</span>
								</div>
							{/if}
						</div>
						<div class="current-file" title={progress.current_file}>
							{progress.current_file}
						</div>
					</div>
				{:else if scanning}
					<div class="progress-section">
						<div class="progress-bar-track">
							<div class="progress-bar-fill indeterminate"></div>
						</div>
						<div class="current-file">Starting scan...</div>
					</div>
				{/if}

				{#if scanError}
					<div class="error">{scanError}</div>
				{/if}

				<div class="dialog-actions">
					<button class="cancel" onclick={() => (showScanDialog = false)} disabled={scanning}>
						Cancel
					</button>
					<button class="primary" onclick={handleScan} disabled={scanning}>
						{scanning ? 'Scanning...' : 'Start Scan'}
					</button>
				</div>
			</div>
		</div>
	{/if}

	<main class="content">
		<aside class="sidebar">
			{#key treeRefreshKey}
				<FileTree {selectedPath} onSelect={handleSelect} onScanInto={openScanDialog} />
			{/key}
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

	.scan-stats .saved {
		color: var(--success);
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

	/* Dialog */
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
		width: 480px;
		display: flex;
		flex-direction: column;
		gap: 14px;
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

	.hint {
		font-size: 11px !important;
		opacity: 0.6;
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

	.dialog-content input:disabled {
		opacity: 0.5;
	}

	/* Progress */
	.progress-section {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		background: var(--bg);
		border-radius: 8px;
		border: 1px solid var(--border);
	}

	.progress-bar-track {
		height: 6px;
		background: var(--border);
		border-radius: 3px;
		overflow: hidden;
	}

	.progress-bar-fill {
		height: 100%;
		background: var(--accent-light);
		border-radius: 3px;
		transition: width 0.3s;
	}

	.progress-bar-fill.indeterminate {
		width: 30% !important;
		animation: indeterminate 1.5s infinite ease-in-out;
	}

	@keyframes indeterminate {
		0% { transform: translateX(-100%); }
		100% { transform: translateX(400%); }
	}

	.progress-stats {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 4px 16px;
	}

	.stat-row {
		display: flex;
		justify-content: space-between;
		font-size: 12px;
	}

	.stat-label {
		color: var(--text-muted);
	}

	.stat-value {
		font-family: var(--font-mono);
		font-size: 11px;
	}

	.stat-value.highlight {
		color: var(--duplicate);
		font-weight: 600;
	}

	.stat-value.saved {
		color: var(--success);
		font-weight: 600;
	}

	.current-file {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
