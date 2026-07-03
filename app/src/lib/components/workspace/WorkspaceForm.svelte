<script lang="ts">
	import { UiButton, UiField } from '$lib/components/ui';

	type Mode = 'create' | 'import';

	interface Props {
		mode: Mode;
		label: string;
		tags?: string;
		storePath: string;
		loading?: boolean;
		error?: string | null;
		onLabelChange: (value: string) => void;
		onTagsChange?: (value: string) => void;
		onStorePathChange: (value: string) => void;
		onBrowseFolder?: () => void;
		onBrowseFile?: () => void;
		onSubmit: () => void;
		onBack: () => void;
	}

	let {
		mode,
		label,
		tags = '',
		storePath,
		loading = false,
		error = null,
		onLabelChange,
		onTagsChange,
		onStorePathChange,
		onBrowseFolder,
		onBrowseFile,
		onSubmit,
		onBack
	}: Props = $props();

	let title = $derived(mode === 'create' ? 'New Workspace' : 'Import Existing Store');
	let submitLabel = $derived(mode === 'create' ? 'Create' : loading ? 'Importing...' : 'Import');
	let labelPlaceholder = $derived(mode === 'create' ? 'My Photos' : 'My Imported Store');
	let storePlaceholder = $derived(
		mode === 'create' ? '/path/to/workspace/.store' : '/path/to/.store or /path/to/.store/metadata.redb'
	);
	let storeHint = $derived(
		mode === 'create'
			? 'Directory where blobs and metadata will be stored'
			: 'Path to an existing .store directory or its metadata.redb file'
	);

	function inputValue(event: Event) {
		return (event.currentTarget as HTMLInputElement).value;
	}

	function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		onSubmit();
	}
</script>

<form class="flex flex-col gap-4" onsubmit={handleSubmit}>
	<h3 class="text-sm font-semibold">{title}</h3>

	<div class="grid gap-3">
		<UiField label="Label">
			<input
				class="input input-bordered input-sm w-full"
				type="text"
				value={label}
				placeholder={labelPlaceholder}
				disabled={loading}
				oninput={(event) => onLabelChange(inputValue(event))}
			/>
		</UiField>

		{#if mode === 'create'}
			<UiField label="Tags (comma-separated)">
				<input
					class="input input-bordered input-sm w-full"
					type="text"
					value={tags}
					placeholder="photos, backup"
					disabled={loading}
					oninput={(event) => onTagsChange?.(inputValue(event))}
				/>
			</UiField>
		{/if}

		<UiField label="Store path" hint={storeHint}>
			<div class="flex gap-2">
				<input
					class="input input-bordered input-sm min-w-0 flex-1 font-path"
					type="text"
					value={storePath}
					placeholder={storePlaceholder}
					disabled={loading}
					oninput={(event) => onStorePathChange(inputValue(event))}
				/>
				{#if mode === 'create'}
					{#if onBrowseFolder}
						<UiButton variant="secondary" disabled={loading} onclick={onBrowseFolder}>Browse</UiButton>
					{/if}
				{:else}
					{#if onBrowseFolder}
						<UiButton variant="secondary" disabled={loading} onclick={onBrowseFolder}>Folder</UiButton>
					{/if}
					{#if onBrowseFile}
						<UiButton variant="secondary" disabled={loading} onclick={onBrowseFile}>File</UiButton>
					{/if}
				{/if}
			</div>
		</UiField>
	</div>

	{#if error}
		<div class="alert alert-error py-2 text-sm" role="alert">
			<span>{error}</span>
		</div>
	{/if}

	<div class="flex flex-wrap items-center justify-end gap-2">
		<UiButton variant="ghost" disabled={loading} onclick={onBack}>Back</UiButton>
		<UiButton variant="primary" type="submit" loading={loading} disabled={!label.trim() || !storePath.trim()}>
			{submitLabel}
		</UiButton>
	</div>
</form>
