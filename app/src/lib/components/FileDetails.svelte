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

<div class="flex h-full min-h-0 flex-col">
	<header class="flex h-10 shrink-0 items-center border-b border-base-300 px-4">
		<span class="text-sm font-semibold">Details</span>
	</header>

	<div class="min-h-0 flex-1 overflow-y-auto p-4">
		<div class="rounded-box border border-base-300 bg-base-200 p-3">
			<div class="flex min-w-0 items-center gap-2">
				<span class="text-sm">{entry.is_dir ? 'DIR' : 'FILE'}</span>
				<span class="font-path min-w-0 flex-1 break-all text-sm">{path}</span>
			</div>
		</div>

		{#if !entry.is_dir}
			<button
				class="btn btn-primary btn-sm mt-3 w-full"
				type="button"
				onclick={handleOpen}
				disabled={opening}
			>
				{opening ? 'Opening...' : 'Open'}
			</button>
		{/if}

		{#if isImage && previewUrl}
			<div
				class="rounded-box mt-4 flex min-h-28 items-center justify-center overflow-hidden border border-base-300 bg-base-200"
			>
				<img class="max-h-96 max-w-full object-contain" src={previewUrl} alt={entry.name} />
			</div>
		{:else if isImage && loading}
			<div
				class="rounded-box mt-4 flex h-28 items-center justify-center border border-base-300 bg-base-200 text-sm text-base-content/60"
			>
				Loading preview...
			</div>
		{/if}

		{#if entry.is_dir}
			<div class="mt-4 grid gap-2 sm:grid-cols-2">
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="text-xs text-base-content/50">Type</div>
					<div class="mt-1 text-sm font-semibold">Directory</div>
				</div>
			</div>
		{:else if loading}
			<div class="mt-4 text-sm text-base-content/60">Loading...</div>
		{:else if metadata}
			<div class="mt-4 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="text-xs text-base-content/50">Size</div>
					<div class="font-path mt-1 text-sm font-semibold">
						{formatSize(metadata.original_size)}
					</div>
				</div>
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="text-xs text-base-content/50">Stored</div>
					<div class="font-path mt-1 text-sm font-semibold">
						{formatSize(metadata.compressed_size)}
					</div>
				</div>
				{#if metadata.original_size > 0}
					<div class="rounded-box border border-base-300 bg-base-200 p-3">
						<div class="text-xs text-base-content/50">Ratio</div>
						<div class="font-path mt-1 text-sm font-semibold">
							{((metadata.compressed_size / metadata.original_size) * 100).toFixed(1)}%
						</div>
					</div>
				{/if}
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="text-xs text-base-content/50">Modified</div>
					<div class="font-path mt-1 text-sm font-semibold">
						{formatTimestamp(metadata.modified)}
					</div>
				</div>
				<div class="rounded-box border border-base-300 bg-base-200 p-3 sm:col-span-2">
					<div class="text-xs text-base-content/50">CID</div>
					<div class="font-path mt-1 break-all text-sm font-semibold">{cidString}</div>
				</div>
			</div>

			{#if duplicates.length > 1}
				<div class="alert alert-error mt-4 block">
					<div class="text-sm font-semibold">{duplicates.length} copies of this file</div>
					<ul class="font-path mt-2 space-y-1 text-xs">
						{#each duplicates as dup}
							<li class={dup === path ? 'font-semibold text-error-content' : 'opacity-80'}>
								{dup}
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	</div>
</div>
