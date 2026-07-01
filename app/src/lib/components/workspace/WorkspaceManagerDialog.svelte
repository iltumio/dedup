<script lang="ts">
	import type { WorkspacesConfig } from '$lib/api/tauri';
	import { UiButton, UiDialog, UiEmptyState } from '$lib/components/ui';
	import WorkspaceForm from './WorkspaceForm.svelte';
	import WorkspaceListItem from './WorkspaceListItem.svelte';

	type Mode = 'list' | 'create' | 'import';

	interface Props {
		open: boolean;
		config: WorkspacesConfig;
		mode: Mode;
		error: string | null;
		importing: boolean;
		newLabel: string;
		newTags: string;
		newStorePath: string;
		importLabel: string;
		importStorePath: string;
		onClose: () => void;
		onModeChange: (mode: Mode) => void;
		onSwitch: (id: string) => void;
		onDelete: (id: string) => void;
		onExport: () => void;
		onImportConfig: () => void;
		onCreate: () => void;
		onImportStore: () => void;
		onNewLabelChange: (value: string) => void;
		onNewTagsChange: (value: string) => void;
		onNewStorePathChange: (value: string) => void;
		onImportLabelChange: (value: string) => void;
		onImportStorePathChange: (value: string) => void;
	}

	let {
		open,
		config,
		mode,
		error,
		importing,
		newLabel,
		newTags,
		newStorePath,
		importLabel,
		importStorePath,
		onClose,
		onModeChange,
		onSwitch,
		onDelete,
		onExport,
		onImportConfig,
		onCreate,
		onImportStore,
		onNewLabelChange,
		onNewTagsChange,
		onNewStorePathChange,
		onImportLabelChange,
		onImportStorePathChange
	}: Props = $props();
</script>

<UiDialog {open} title="Workspaces" wide {onClose}>
	{#if mode === 'create'}
		<WorkspaceForm
			mode="create"
			label={newLabel}
			tags={newTags}
			storePath={newStorePath}
			loading={false}
			{error}
			onBack={() => onModeChange('list')}
			onSubmit={onCreate}
			onLabelChange={onNewLabelChange}
			onTagsChange={onNewTagsChange}
			onStorePathChange={onNewStorePathChange}
		/>
	{:else if mode === 'import'}
		<WorkspaceForm
			mode="import"
			label={importLabel}
			storePath={importStorePath}
			loading={importing}
			{error}
			onBack={() => onModeChange('list')}
			onSubmit={onImportStore}
			onLabelChange={onImportLabelChange}
			onStorePathChange={onImportStorePathChange}
		/>
	{:else}
		<div class="flex flex-col gap-4">
			{#if config.workspaces.length === 0}
				<UiEmptyState title="No workspaces yet" message="Create one to get started." />
			{:else}
				<ul class="flex list-none flex-col gap-2 p-0">
					{#each config.workspaces as workspace (workspace.id)}
						<WorkspaceListItem
							{workspace}
							active={workspace.id === config.active_workspace_id}
							onSelect={onSwitch}
							onDelete={onDelete}
						/>
					{/each}
				</ul>
			{/if}

			{#if error}
				<div class="alert alert-error py-2 text-sm" role="alert">
					<span>{error}</span>
				</div>
			{/if}

			<div class="flex flex-wrap items-center gap-2">
				<UiButton variant="secondary" onclick={onExport}>Export</UiButton>
				<UiButton variant="secondary" onclick={onImportConfig}>Import Config</UiButton>
				<div class="grow"></div>
				<UiButton variant="ghost" onclick={onClose}>Close</UiButton>
				<UiButton variant="primary" onclick={() => onModeChange('import')}>Import Existing</UiButton>
				<UiButton variant="primary" onclick={() => onModeChange('create')}>New Workspace</UiButton>
			</div>
		</div>
	{/if}
</UiDialog>
