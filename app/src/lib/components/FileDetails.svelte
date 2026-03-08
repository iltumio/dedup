<script lang="ts">
	import {
		getFileMetadata,
		findDuplicates,
		readFile,
		openFile,
		formatSize,
		formatTimestamp,
		type FileMetadata,
		type DirEntry
	} from '$lib/api/tauri';

	interface Props {
		path: string;
		entry: DirEntry;
	}

	let { path, entry }: Props = $props();

	let metadata = $state<FileMetadata | null>(null);
	let duplicates = $state<string[]>([]);
	let cidString = $state('');
	let loading = $state(false);
	let previewUrl = $state<string | null>(null);

	const IMAGE_EXTENSIONS = new Set([
		'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico', 'svg', 'avif', 'tiff', 'tif'
	]);

	function getExtension(name: string): string {
		const dot = name.lastIndexOf('.');
		if (dot === -1) return '';
		return name.slice(dot + 1).toLowerCase();
	}

	function getMimeType(ext: string): string {
		const map: Record<string, string> = {
			png: 'image/png',
			jpg: 'image/jpeg',
			jpeg: 'image/jpeg',
			gif: 'image/gif',
			webp: 'image/webp',
			bmp: 'image/bmp',
			ico: 'image/x-icon',
			svg: 'image/svg+xml',
			avif: 'image/avif',
			tiff: 'image/tiff',
			tif: 'image/tiff'
		};
		return map[ext] ?? 'application/octet-stream';
	}

	let isImage = $derived(IMAGE_EXTENSIONS.has(getExtension(entry.name)));

	let opening = $state(false);

	async function handleOpen() {
		opening = true;
		try {
			await openFile(path);
		} catch (e) {
			console.error('Failed to open file:', e);
		}
		opening = false;
	}

	$effect(() => {
		loadDetails(path);
		return () => {
			if (previewUrl) {
				URL.revokeObjectURL(previewUrl);
				previewUrl = null;
			}
		};
	});

	async function loadDetails(p: string) {
		if (entry.is_dir) {
			metadata = null;
			duplicates = [];
			previewUrl = null;
			return;
		}

		loading = true;
		try {
			metadata = await getFileMetadata(p);
			duplicates = await findDuplicates(p);

			// Convert CID bytes to hex for display
			if (metadata?.cid) {
				cidString = metadata.cid.map((b) => b.toString(16).padStart(2, '0')).join('');
				if (cidString.length > 24) {
					cidString = cidString.slice(0, 12) + '...' + cidString.slice(-12);
				}
			}

			// Load image preview
			const ext = getExtension(entry.name);
			if (IMAGE_EXTENSIONS.has(ext)) {
				try {
					const bytes = await readFile(p);
					const blob = new Blob([new Uint8Array(bytes)], { type: getMimeType(ext) });
					if (previewUrl) URL.revokeObjectURL(previewUrl);
					previewUrl = URL.createObjectURL(blob);
				} catch (e) {
					console.error('Failed to load image preview:', e);
					previewUrl = null;
				}
			} else {
				previewUrl = null;
			}
		} catch (e) {
			console.error('Failed to load details:', e);
		}
		loading = false;
	}
</script>

<div class="file-details">
	<div class="header">
		<span class="title">Details</span>
	</div>

	<div class="content">
		<div class="path-display">
			<span class="icon">{entry.is_dir ? '📁' : '📄'}</span>
			<span class="path">{path}</span>
		</div>

		{#if !entry.is_dir}
			<button class="open-btn" onclick={handleOpen} disabled={opening}>
				{opening ? 'Opening...' : 'Open'}
			</button>
		{/if}

		{#if isImage && previewUrl}
			<div class="image-preview">
				<img src={previewUrl} alt={entry.name} />
			</div>
		{:else if isImage && loading}
			<div class="image-preview placeholder-img">
				<span>Loading preview...</span>
			</div>
		{/if}

		{#if entry.is_dir}
			<div class="info-row">
				<span class="label">Type</span>
				<span class="value">Directory</span>
			</div>
		{:else if loading}
			<div class="loading">Loading...</div>
		{:else if metadata}
			<div class="info-grid">
				<div class="info-row">
					<span class="label">Size</span>
					<span class="value">{formatSize(metadata.original_size)}</span>
				</div>
				<div class="info-row">
					<span class="label">Stored</span>
					<span class="value">{formatSize(metadata.compressed_size)}</span>
				</div>
				{#if metadata.original_size > 0}
					<div class="info-row">
						<span class="label">Ratio</span>
						<span class="value">
							{((metadata.compressed_size / metadata.original_size) * 100).toFixed(1)}%
						</span>
					</div>
				{/if}
				<div class="info-row">
					<span class="label">Modified</span>
					<span class="value">{formatTimestamp(metadata.modified)}</span>
				</div>
				<div class="info-row">
					<span class="label">CID</span>
					<span class="value mono">{cidString}</span>
				</div>
			</div>

			{#if duplicates.length > 1}
				<div class="duplicates">
					<div class="dup-header">
						<span class="dup-icon">⚠</span>
						<span>{duplicates.length} copies of this file</span>
					</div>
					<ul class="dup-list">
						{#each duplicates as dup}
							<li class:current={dup === path}>{dup}</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	</div>
</div>

<style>
	.file-details {
		height: 100%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.header {
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
	}

	.title {
		font-weight: 600;
		font-size: 14px;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
	}

	.path-display {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 16px;
		padding-bottom: 12px;
		border-bottom: 1px solid var(--border);
	}

	.path-display .icon {
		font-size: 20px;
	}

	.path-display .path {
		font-family: var(--font-mono);
		font-size: 13px;
		word-break: break-all;
	}

	.info-grid {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.info-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-size: 13px;
	}

	.label {
		color: var(--text-muted);
	}

	.value {
		font-family: var(--font-mono);
		font-size: 12px;
	}

	.mono {
		font-family: var(--font-mono);
	}

	.loading {
		color: var(--text-muted);
		font-size: 13px;
	}

	.duplicates {
		margin-top: 16px;
		padding: 12px;
		background: rgba(239, 83, 80, 0.1);
		border: 1px solid rgba(239, 83, 80, 0.3);
		border-radius: 6px;
	}

	.dup-header {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		font-weight: 600;
		color: var(--duplicate);
		margin-bottom: 8px;
	}

	.dup-list {
		list-style: none;
		font-size: 12px;
		font-family: var(--font-mono);
	}

	.dup-list li {
		padding: 3px 0;
		color: var(--text-muted);
	}

	.dup-list li.current {
		color: var(--text);
		font-weight: 600;
	}

	.image-preview {
		margin-bottom: 16px;
		padding-bottom: 12px;
		border-bottom: 1px solid var(--border);
		display: flex;
		justify-content: center;
		align-items: center;
		background: var(--bg);
		border-radius: 8px;
		overflow: hidden;
		min-height: 80px;
	}

	.image-preview img {
		max-width: 100%;
		max-height: 400px;
		object-fit: contain;
		display: block;
	}

	.image-preview.placeholder-img {
		height: 120px;
		color: var(--text-muted);
		font-size: 12px;
	}

	.open-btn {
		width: 100%;
		padding: 8px 14px;
		margin-bottom: 16px;
		background: var(--accent);
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		border: none;
		color: var(--text);
		transition: background 0.15s;
	}

	.open-btn:hover {
		background: var(--accent-light);
	}

	.open-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
