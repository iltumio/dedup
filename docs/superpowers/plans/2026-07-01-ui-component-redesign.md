# UI Component Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the Svelte/Tauri app as a compact DaisyUI Night operations console with reusable UI primitives and extracted scan/workspace components.

**Architecture:** Keep backend and API contracts unchanged. Install Tailwind CSS v4 and DaisyUI v5, then migrate the app from large local CSS blocks to small Svelte components composed by `+page.svelte`. `+page.svelte` remains the state/API orchestration layer while UI components own markup, styling, and presentation structure.

**Tech Stack:** Svelte 5, SvelteKit, Tauri API, TypeScript, Tailwind CSS v4, DaisyUI v5, npm.

---

## File Structure

Create shared primitives:

- `app/src/lib/components/ui/UiButton.svelte` — DaisyUI button wrapper for variants, size, loading, disabled, icon/title handling.
- `app/src/lib/components/ui/UiDialog.svelte` — accessible modal frame with title/body/actions slots.
- `app/src/lib/components/ui/UiField.svelte` — label, hint, error, and form-control slot wrapper.
- `app/src/lib/components/ui/UiSegmentedControl.svelte` — compact two-option Files/Stats control.
- `app/src/lib/components/ui/UiStat.svelte` — compact metric display.
- `app/src/lib/components/ui/UiBadge.svelte` — semantic badge wrapper.
- `app/src/lib/components/ui/UiEmptyState.svelte` — consistent empty state.
- `app/src/lib/components/ui/index.ts` — re-export UI primitives.

Create shell and workflow components:

- `app/src/lib/components/AppShell.svelte` — top bar, workspace trigger, view tabs, aggregate stats, scan action, main content slot.
- `app/src/lib/components/workspace/WorkspaceManagerDialog.svelte` — workspace list/create/import/config dialog state machine.
- `app/src/lib/components/workspace/WorkspaceForm.svelte` — create/import workspace form.
- `app/src/lib/components/workspace/WorkspaceListItem.svelte` — dense workspace row.
- `app/src/lib/components/scan/ScanDialog.svelte` — scan modal workflow.
- `app/src/lib/components/scan/ScanPresetList.svelte` — built-in scan preset toggles.
- `app/src/lib/components/scan/CustomScanRuleList.svelte` — saved rule list with per-scan enablement.
- `app/src/lib/components/scan/CustomScanRuleEditor.svelte` — add custom rule form.
- `app/src/lib/components/scan/ScanProgressPanel.svelte` — progress and cancel state.

Modify existing files:

- `app/package.json` and `app/package-lock.json` — add Tailwind/DaisyUI packages.
- `app/vite.config.ts` — add Tailwind Vite plugin before SvelteKit.
- `app/src/app.css` — replace custom CSS tokens with Tailwind/DaisyUI imports and app-specific global sizing.
- `app/src/routes/+page.svelte` — compose new components, keep state/API logic.
- `app/src/lib/components/FileTree.svelte` — restyle with Tailwind/DaisyUI classes.
- `app/src/lib/components/TreeNode.svelte` — restyle and improve accessible scan-into button.
- `app/src/lib/components/FileDetails.svelte` — restyle inspector view.
- `app/src/lib/components/StatsPage.svelte` — restyle stats view using compact console vocabulary.

---

### Task 1: Tailwind And DaisyUI Setup

**Files:**
- Modify: `app/package.json`
- Modify: `app/package-lock.json`
- Modify: `app/vite.config.ts`
- Modify: `app/src/app.css`

- [ ] **Step 1: Install frontend styling dependencies**

Run:

```bash
npm --prefix app install -D tailwindcss@latest @tailwindcss/vite@latest daisyui@latest
```

Expected: `app/package.json` gains `tailwindcss`, `@tailwindcss/vite`, and `daisyui` in `devDependencies`; `app/package-lock.json` updates.

- [ ] **Step 2: Configure Vite for Tailwind**

Replace `app/vite.config.ts` with:

```ts
import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true
	}
});
```

- [ ] **Step 3: Replace global CSS with Tailwind/DaisyUI base**

Replace `app/src/app.css` with:

```css
@import "tailwindcss";

@plugin "daisyui" {
	themes: night --default;
}

:root {
	font-family:
		Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
	font-synthesis: none;
	text-rendering: optimizeLegibility;
	-webkit-font-smoothing: antialiased;
	-moz-osx-font-smoothing: grayscale;
}

html,
body,
#svelte {
	height: 100%;
}

body {
	margin: 0;
	overflow: hidden;
}

button,
input,
select,
textarea {
	font: inherit;
}

.font-path {
	font-family:
		"JetBrains Mono", "Fira Code", "Cascadia Code", "SFMono-Regular", Consolas, monospace;
}
```

- [ ] **Step 4: Run frontend checks**

Run:

```bash
npm --prefix app run check
```

Expected: `svelte-check found 0 errors and 0 warnings`. The existing npm warning about `scripts-prepend-node-path` may still appear.

Run:

```bash
npm --prefix app run build
```

Expected: Vite builds successfully and DaisyUI classes compile.

- [ ] **Step 5: Commit**

```bash
git add app/package.json app/package-lock.json app/vite.config.ts app/src/app.css
git commit -m "chore: add Tailwind and DaisyUI"
```

---

### Task 2: Shared UI Primitives

**Files:**
- Create: `app/src/lib/components/ui/UiButton.svelte`
- Create: `app/src/lib/components/ui/UiDialog.svelte`
- Create: `app/src/lib/components/ui/UiField.svelte`
- Create: `app/src/lib/components/ui/UiSegmentedControl.svelte`
- Create: `app/src/lib/components/ui/UiStat.svelte`
- Create: `app/src/lib/components/ui/UiBadge.svelte`
- Create: `app/src/lib/components/ui/UiEmptyState.svelte`
- Create: `app/src/lib/components/ui/index.ts`

- [ ] **Step 1: Create `UiButton`**

Create `app/src/lib/components/ui/UiButton.svelte`:

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	type Variant = 'primary' | 'secondary' | 'ghost' | 'destructive';
	type Size = 'xs' | 'sm' | 'md';

	interface Props {
		variant?: Variant;
		size?: Size;
		type?: 'button' | 'submit' | 'reset';
		disabled?: boolean;
		loading?: boolean;
		title?: string;
		ariaLabel?: string;
		class?: string;
		onclick?: (event: MouseEvent) => void;
		children?: Snippet;
	}

	let {
		variant = 'secondary',
		size = 'sm',
		type = 'button',
		disabled = false,
		loading = false,
		title,
		ariaLabel,
		class: className = '',
		onclick,
		children
	}: Props = $props();

	const variantClass: Record<Variant, string> = {
		primary: 'btn-primary',
		secondary: 'btn-neutral',
		ghost: 'btn-ghost',
		destructive: 'btn-error'
	};
</script>

<button
	{type}
	{title}
	aria-label={ariaLabel}
	class={`btn btn-${size} ${variantClass[variant]} ${loading ? 'btn-disabled' : ''} ${className}`}
	disabled={disabled || loading}
	{onclick}
>
	{#if loading}
		<span class="loading loading-spinner loading-xs" aria-hidden="true"></span>
	{/if}
	{@render children?.()}
</button>
```

- [ ] **Step 2: Create `UiDialog`**

Create `app/src/lib/components/ui/UiDialog.svelte`:

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		title: string;
		description?: string;
		wide?: boolean;
		onClose: () => void;
		children?: Snippet;
		actions?: Snippet;
	}

	let { open, title, description, wide = false, onClose, children, actions }: Props = $props();
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/65 p-3">
		<section
			class={`card max-h-[calc(100vh-1.5rem)] w-full overflow-hidden border border-base-300 bg-base-200 shadow-xl ${wide ? 'max-w-4xl' : 'max-w-2xl'}`}
			role="dialog"
			aria-modal="true"
			aria-labelledby="dialog-title"
			aria-describedby={description ? 'dialog-description' : undefined}
		>
			<header class="flex items-start justify-between gap-4 border-b border-base-300 px-4 py-3">
				<div class="min-w-0">
					<h2 id="dialog-title" class="truncate text-sm font-semibold">{title}</h2>
					{#if description}
						<p id="dialog-description" class="mt-1 text-xs text-base-content/60">{description}</p>
					{/if}
				</div>
				<button class="btn btn-ghost btn-xs" type="button" aria-label="Close dialog" onclick={onClose}>
					x
				</button>
			</header>

			<div class="min-h-0 overflow-y-auto px-4 py-4">
				{@render children?.()}
			</div>

			{#if actions}
				<footer class="flex flex-wrap items-center justify-end gap-2 border-t border-base-300 px-4 py-3">
					{@render actions()}
				</footer>
			{/if}
		</section>
	</div>
{/if}
```

- [ ] **Step 3: Create field, segmented control, stat, badge, and empty state primitives**

Create `app/src/lib/components/ui/UiField.svelte`:

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		hint?: string;
		error?: string | null;
		children?: Snippet;
	}

	let { label, hint, error = null, children }: Props = $props();
</script>

<label class="form-control w-full gap-1">
	<span class="label py-0">
		<span class="label-text text-xs font-medium text-base-content/70">{label}</span>
	</span>
	{@render children?.()}
	{#if hint}
		<span class="text-[11px] leading-4 text-base-content/50">{hint}</span>
	{/if}
	{#if error}
		<span class="text-xs text-error">{error}</span>
	{/if}
</label>
```

Create `app/src/lib/components/ui/UiSegmentedControl.svelte`:

```svelte
<script lang="ts" generics="T extends string">
	interface Option<T> {
		value: T;
		label: string;
	}

	interface Props<T> {
		value: T;
		options: Option<T>[];
		onChange: (value: T) => void;
		ariaLabel: string;
	}

	let { value, options, onChange, ariaLabel }: Props<T> = $props();
</script>

<div class="join" role="tablist" aria-label={ariaLabel}>
	{#each options as option}
		<button
			type="button"
			role="tab"
			aria-selected={value === option.value}
			class={`btn btn-xs join-item ${value === option.value ? 'btn-primary' : 'btn-neutral'}`}
			onclick={() => onChange(option.value)}
		>
			{option.label}
		</button>
	{/each}
</div>
```

Create `app/src/lib/components/ui/UiStat.svelte`:

```svelte
<script lang="ts">
	interface Props {
		label: string;
		value: string | number;
		tone?: 'default' | 'success' | 'error' | 'info' | 'warning';
	}

	let { label, value, tone = 'default' }: Props = $props();

	const toneClass = {
		default: 'text-base-content',
		success: 'text-success',
		error: 'text-error',
		info: 'text-info',
		warning: 'text-warning'
	};
</script>

<div class="rounded-box border border-base-300 bg-base-200 px-3 py-2">
	<div class="text-[11px] uppercase text-base-content/50">{label}</div>
	<div class={`font-path mt-1 text-sm font-semibold ${toneClass[tone]}`}>{value}</div>
</div>
```

Create `app/src/lib/components/ui/UiBadge.svelte`:

```svelte
<script lang="ts">
	interface Props {
		tone?: 'neutral' | 'primary' | 'success' | 'error' | 'warning' | 'info';
		text: string;
		class?: string;
	}

	let { tone = 'neutral', text, class: className = '' }: Props = $props();
</script>

<span class={`badge badge-${tone} badge-sm max-w-full truncate ${className}`}>{text}</span>
```

Create `app/src/lib/components/ui/UiEmptyState.svelte`:

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		message?: string;
		actions?: Snippet;
	}

	let { title, message, actions }: Props = $props();
</script>

<div class="flex h-full min-h-40 flex-col items-center justify-center gap-3 p-6 text-center">
	<div>
		<h2 class="text-base font-semibold">{title}</h2>
		{#if message}
			<p class="mt-1 max-w-md text-sm text-base-content/60">{message}</p>
		{/if}
	</div>
	{#if actions}
		<div class="flex items-center gap-2">{@render actions()}</div>
	{/if}
</div>
```

- [ ] **Step 4: Export primitives**

Create `app/src/lib/components/ui/index.ts`:

```ts
export { default as UiBadge } from './UiBadge.svelte';
export { default as UiButton } from './UiButton.svelte';
export { default as UiDialog } from './UiDialog.svelte';
export { default as UiEmptyState } from './UiEmptyState.svelte';
export { default as UiField } from './UiField.svelte';
export { default as UiSegmentedControl } from './UiSegmentedControl.svelte';
export { default as UiStat } from './UiStat.svelte';
```

- [ ] **Step 5: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no Svelte or TypeScript errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/components/ui
git commit -m "feat: add shared UI primitives"
```

---

### Task 3: App Shell Component

**Files:**
- Create: `app/src/lib/components/AppShell.svelte`
- Modify: `app/src/routes/+page.svelte`

- [ ] **Step 1: Create `AppShell`**

Create `app/src/lib/components/AppShell.svelte`:

```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { UiButton, UiSegmentedControl, UiStat } from '$lib/components/ui';

	type View = 'files' | 'stats';

	interface ShellStat {
		label: string;
		value: string | number;
		tone?: 'default' | 'success' | 'error' | 'info' | 'warning';
	}

	interface Props {
		currentView: View;
		hasWorkspace: boolean;
		scanning: boolean;
		stats: ShellStat[];
		onViewChange: (view: View) => void;
		onScan: () => void;
		workspaceControl?: Snippet;
		children?: Snippet;
	}

	let {
		currentView,
		hasWorkspace,
		scanning,
		stats,
		onViewChange,
		onScan,
		workspaceControl,
		children
	}: Props = $props();
</script>

<div class="flex h-screen min-h-0 flex-col bg-base-100 text-base-content" data-theme="night">
	<header class="flex h-12 shrink-0 items-center gap-3 border-b border-base-300 bg-base-200 px-3">
		<h1 class="text-sm font-bold">dedup</h1>

		<div class="min-w-0">{@render workspaceControl?.()}</div>

		{#if hasWorkspace}
			<UiSegmentedControl
				ariaLabel="Primary view"
				value={currentView}
				options={[
					{ value: 'files', label: 'Files' },
					{ value: 'stats', label: 'Stats' }
				]}
				onChange={onViewChange}
			/>
		{/if}

		{#if stats.length > 0}
			<div class="ml-auto hidden min-w-0 grid-cols-3 gap-2 lg:grid">
				{#each stats as stat}
					<UiStat label={stat.label} value={stat.value} tone={stat.tone} />
				{/each}
			</div>
		{:else}
			<div class="ml-auto"></div>
		{/if}

		<UiButton variant="primary" disabled={!hasWorkspace || scanning} loading={scanning} onclick={onScan}>
			Scan
		</UiButton>
	</header>

	<main class="min-h-0 flex-1 overflow-hidden">
		{@render children?.()}
	</main>
</div>
```

- [ ] **Step 2: Wire `AppShell` around the current page content**

In `app/src/routes/+page.svelte`, import `AppShell`, then wrap the existing content. Keep existing dialog markup for now; this task only changes the outer shell. The `shellStats` derived value should be:

```ts
let shellStats = $derived(
	aggFiles > 0
		? [
				{ label: 'Files', value: aggFiles },
				{ label: 'Duplicates', value: aggDuplicates, tone: 'error' as const },
				{ label: 'Saved', value: `${formatSize(aggSavedBytes)} (${aggSavedPct}%)`, tone: 'success' as const }
			]
		: []
);
```

Use this workspace control inside the `AppShell` call:

```svelte
{#snippet workspaceControl()}
	<button class="btn btn-neutral btn-sm max-w-72 justify-start" type="button" onclick={openWorkspaceManager}>
		{#if activeWorkspace}
			<span class="truncate">{activeWorkspace.label}</span>
		{:else}
			<span class="text-base-content/50">No workspace</span>
		{/if}
	</button>
{/snippet}
```

Use `AppShell` like:

```svelte
<AppShell
	{currentView}
	{hasWorkspace}
	{scanning}
	stats={shellStats}
	onViewChange={(view) => (currentView = view)}
	onScan={() => openScanDialog()}
>
	{#snippet workspaceControl()}
		<!-- workspace button snippet from above -->
	{/snippet}

	<!-- existing dialogs and main content move inside here temporarily -->
</AppShell>
```

- [ ] **Step 3: Remove obsolete top toolbar CSS only after the shell renders**

Delete the old `.toolbar`, `.logo`, `.workspace-btn`, `.scan-btn`, `.scan-stats`, and `.view-toggle` styles from `+page.svelte` after the same behavior is visible in `AppShell`.

- [ ] **Step 4: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no Svelte errors. The top bar should render from `AppShell`.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/components/AppShell.svelte app/src/routes/+page.svelte
git commit -m "feat: add compact app shell"
```

---

### Task 4: Workspace Components

**Files:**
- Create: `app/src/lib/components/workspace/WorkspaceListItem.svelte`
- Create: `app/src/lib/components/workspace/WorkspaceForm.svelte`
- Create: `app/src/lib/components/workspace/WorkspaceManagerDialog.svelte`
- Modify: `app/src/routes/+page.svelte`

- [ ] **Step 1: Create `WorkspaceListItem`**

Create `app/src/lib/components/workspace/WorkspaceListItem.svelte`:

```svelte
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
```

- [ ] **Step 2: Create `WorkspaceForm`**

Create `app/src/lib/components/workspace/WorkspaceForm.svelte`:

```svelte
<script lang="ts">
	import { UiButton, UiField } from '$lib/components/ui';

	interface Props {
		mode: 'create' | 'import';
		label: string;
		tags?: string;
		storePath: string;
		loading?: boolean;
		error?: string | null;
		onLabelChange: (value: string) => void;
		onTagsChange?: (value: string) => void;
		onStorePathChange: (value: string) => void;
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
		onSubmit,
		onBack
	}: Props = $props();
</script>

<div class="space-y-4">
	<UiField label="Label">
		<input
			class="input input-sm input-bordered w-full"
			type="text"
			value={label}
			placeholder={mode === 'create' ? 'My Photos' : 'My Imported Store'}
			disabled={loading}
			oninput={(event) => onLabelChange(event.currentTarget.value)}
		/>
	</UiField>

	{#if mode === 'create'}
		<UiField label="Tags" hint="Comma-separated labels for filtering workspaces">
			<input
				class="input input-sm input-bordered w-full"
				type="text"
				value={tags}
				placeholder="photos, backup"
				oninput={(event) => onTagsChange?.(event.currentTarget.value)}
			/>
		</UiField>
	{/if}

	<UiField
		label="Store path"
		hint={mode === 'create'
			? 'Directory where blobs and metadata will be stored'
			: 'Path to an existing .store directory or metadata.redb file'}
	>
		<input
			class="font-path input input-sm input-bordered w-full"
			type="text"
			value={storePath}
			placeholder={mode === 'create' ? '/path/to/workspace/.store' : '/path/to/.store'}
			disabled={loading}
			oninput={(event) => onStorePathChange(event.currentTarget.value)}
		/>
	</UiField>

	{#if error}
		<div class="alert alert-error py-2 text-sm">{error}</div>
	{/if}

	<div class="flex justify-end gap-2">
		<UiButton variant="ghost" onclick={onBack} disabled={loading}>Back</UiButton>
		<UiButton variant="primary" onclick={onSubmit} loading={loading}>
			{mode === 'create' ? 'Create' : 'Import'}
		</UiButton>
	</div>
</div>
```

- [ ] **Step 3: Create `WorkspaceManagerDialog`**

Create `app/src/lib/components/workspace/WorkspaceManagerDialog.svelte`:

```svelte
<script lang="ts">
	import type { Workspace, WorkspacesConfig } from '$lib/api/tauri';
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

	let props: Props = $props();
</script>

<UiDialog open={props.open} title="Workspaces" wide onClose={props.onClose}>
	{#if props.mode === 'create'}
		<WorkspaceForm
			mode="create"
			label={props.newLabel}
			tags={props.newTags}
			storePath={props.newStorePath}
			error={props.error}
			onLabelChange={props.onNewLabelChange}
			onTagsChange={props.onNewTagsChange}
			onStorePathChange={props.onNewStorePathChange}
			onSubmit={props.onCreate}
			onBack={() => props.onModeChange('list')}
		/>
	{:else if props.mode === 'import'}
		<WorkspaceForm
			mode="import"
			label={props.importLabel}
			storePath={props.importStorePath}
			loading={props.importing}
			error={props.error}
			onLabelChange={props.onImportLabelChange}
			onStorePathChange={props.onImportStorePathChange}
			onSubmit={props.onImportStore}
			onBack={() => props.onModeChange('list')}
		/>
	{:else}
		<div class="space-y-4">
			{#if props.config.workspaces.length === 0}
				<UiEmptyState title="No workspaces" message="Create or import a workspace to start scanning." />
			{:else}
				<ul class="space-y-2">
					{#each props.config.workspaces as workspace (workspace.id)}
						<WorkspaceListItem
							{workspace}
							active={workspace.id === props.config.active_workspace_id}
							onSelect={props.onSwitch}
							onDelete={props.onDelete}
						/>
					{/each}
				</ul>
			{/if}

			{#if props.error}
				<div class="alert alert-error py-2 text-sm">{props.error}</div>
			{/if}

			<div class="flex flex-wrap justify-end gap-2">
				<UiButton variant="ghost" onclick={props.onExport}>Export</UiButton>
				<UiButton variant="ghost" onclick={props.onImportConfig}>Import Config</UiButton>
				<UiButton variant="secondary" onclick={() => props.onModeChange('import')}>Import Existing</UiButton>
				<UiButton variant="primary" onclick={() => props.onModeChange('create')}>New Workspace</UiButton>
			</div>
		</div>
	{/if}
</UiDialog>
```

- [ ] **Step 4: Replace workspace dialog markup in `+page.svelte`**

Replace the `showCreateWorkspace` and `showImportWorkspace` booleans with:

```ts
let workspaceDialogMode = $state<'list' | 'create' | 'import'>('list');
```

Update:

```ts
function openWorkspaceManager() {
	wsError = null;
	workspaceDialogMode = 'list';
	showWorkspaceDialog = true;
}

function openCreateWorkspace() {
	newWsLabel = '';
	newWsTags = '';
	newWsStorePath = '';
	wsError = null;
	workspaceDialogMode = 'create';
}

function openImportWorkspace() {
	importWsStorePath = '';
	importWsLabel = '';
	wsError = null;
	workspaceDialogMode = 'import';
}
```

Use the component:

```svelte
<WorkspaceManagerDialog
	open={showWorkspaceDialog}
	config={workspacesConfig}
	mode={workspaceDialogMode}
	error={wsError}
	importing={importingWs}
	newLabel={newWsLabel}
	newTags={newWsTags}
	newStorePath={newWsStorePath}
	importLabel={importWsLabel}
	importStorePath={importWsStorePath}
	onClose={() => (showWorkspaceDialog = false)}
	onModeChange={(mode) => (workspaceDialogMode = mode)}
	onSwitch={handleSwitchWorkspace}
	onDelete={handleDeleteWorkspace}
	onExport={handleExportWorkspaces}
	onImportConfig={handleImportWorkspaces}
	onCreate={handleCreateWorkspace}
	onImportStore={handleImportWorkspace}
	onNewLabelChange={(value) => (newWsLabel = value)}
	onNewTagsChange={(value) => (newWsTags = value)}
	onNewStorePathChange={(value) => (newWsStorePath = value)}
	onImportLabelChange={(value) => (importWsLabel = value)}
	onImportStorePathChange={(value) => (importWsStorePath = value)}
/>
```

- [ ] **Step 5: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no Svelte errors and workspace manager flows still compile.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/components/workspace app/src/routes/+page.svelte
git commit -m "feat: extract workspace UI"
```

---

### Task 5: Scan Dialog Components

**Files:**
- Create: `app/src/lib/components/scan/ScanPresetList.svelte`
- Create: `app/src/lib/components/scan/CustomScanRuleList.svelte`
- Create: `app/src/lib/components/scan/CustomScanRuleEditor.svelte`
- Create: `app/src/lib/components/scan/ScanProgressPanel.svelte`
- Create: `app/src/lib/components/scan/ScanDialog.svelte`
- Modify: `app/src/routes/+page.svelte`

- [ ] **Step 1: Create scan preset and custom rule components**

Create `app/src/lib/components/scan/ScanPresetList.svelte`:

```svelte
<script lang="ts">
	interface Preset {
		id: string;
		label: string;
		description: string;
		checked: boolean;
	}

	interface Props {
		presets: Preset[];
		disabled?: boolean;
		onToggle: (id: string, checked: boolean) => void;
	}

	let { presets, disabled = false, onToggle }: Props = $props();
</script>

<div class="grid gap-2 sm:grid-cols-2">
	{#each presets as preset}
		<label class="rounded-box flex items-start gap-3 border border-base-300 bg-base-100 p-3">
			<input
				class="toggle toggle-primary toggle-sm mt-0.5"
				type="checkbox"
				checked={preset.checked}
				{disabled}
				onchange={(event) => onToggle(preset.id, event.currentTarget.checked)}
			/>
			<span class="min-w-0">
				<span class="block text-sm font-medium">{preset.label}</span>
				<span class="block text-xs text-base-content/55">{preset.description}</span>
			</span>
		</label>
	{/each}
</div>
```

Create `app/src/lib/components/scan/CustomScanRuleList.svelte`:

```svelte
<script lang="ts">
	import type { CustomScanRule } from '$lib/api/tauri';
	import { UiBadge, UiButton, UiEmptyState } from '$lib/components/ui';

	interface Props {
		rules: CustomScanRule[];
		activeRuleIds: string[];
		disabled?: boolean;
		onToggle: (id: string, checked: boolean) => void;
		onRemove: (id: string) => void;
	}

	let { rules, activeRuleIds, disabled = false, onToggle, onRemove }: Props = $props();
	let activeIds = $derived(new Set(activeRuleIds));
</script>

{#if rules.length === 0}
	<UiEmptyState title="No saved custom rules" message="Add a regex rule below to reuse it in future scans." />
{:else}
	<ul class="space-y-2">
		{#each rules as rule (rule.id)}
			<li class="rounded-box border border-base-300 bg-base-100 p-3">
				<div class="flex items-start gap-3">
					<input
						class="checkbox checkbox-primary checkbox-sm mt-1"
						type="checkbox"
						checked={activeIds.has(rule.id)}
						{disabled}
						onchange={(event) => onToggle(rule.id, event.currentTarget.checked)}
					/>
					<div class="min-w-0 flex-1">
						<div class="flex min-w-0 items-center gap-2">
							<span class="truncate text-sm font-medium">{rule.label}</span>
							<UiBadge text={rule.action} tone={rule.action === 'archive' ? 'info' : 'warning'} />
						</div>
						<div class="font-path mt-1 truncate text-xs text-base-content/55" title={rule.pattern}>
							{rule.pattern}
						</div>
					</div>
					<UiButton
						variant="ghost"
						size="xs"
						disabled={disabled}
						title="Remove custom rule"
						ariaLabel={`Remove ${rule.label}`}
						onclick={() => onRemove(rule.id)}
					>
						Remove
					</UiButton>
				</div>
			</li>
		{/each}
	</ul>
{/if}
```

- [ ] **Step 2: Create custom rule editor and progress panel**

Create `app/src/lib/components/scan/CustomScanRuleEditor.svelte`:

```svelte
<script lang="ts">
	import type { ScanRuleAction } from '$lib/api/tauri';
	import { UiButton, UiField } from '$lib/components/ui';

	interface Props {
		label: string;
		pattern: string;
		action: ScanRuleAction;
		disabled?: boolean;
		error?: string | null;
		onLabelChange: (value: string) => void;
		onPatternChange: (value: string) => void;
		onActionChange: (value: ScanRuleAction) => void;
		onAdd: () => void;
	}

	let {
		label,
		pattern,
		action,
		disabled = false,
		error = null,
		onLabelChange,
		onPatternChange,
		onActionChange,
		onAdd
	}: Props = $props();
</script>

<div class="rounded-box border border-base-300 bg-base-100 p-3">
	<div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)_9rem_auto]">
		<UiField label="Rule label">
			<input
				class="input input-sm input-bordered w-full"
				value={label}
				placeholder="Build output"
				disabled={disabled}
				oninput={(event) => onLabelChange(event.currentTarget.value)}
			/>
		</UiField>
		<UiField label="Full path regex">
			<input
				class="font-path input input-sm input-bordered w-full"
				value={pattern}
				placeholder="(^|/)dist$"
				disabled={disabled}
				oninput={(event) => onPatternChange(event.currentTarget.value)}
			/>
		</UiField>
		<UiField label="Action">
			<select
				class="select select-sm select-bordered w-full"
				value={action}
				disabled={disabled}
				onchange={(event) => onActionChange(event.currentTarget.value as ScanRuleAction)}
			>
				<option value="ignore">Ignore</option>
				<option value="archive">Archive</option>
			</select>
		</UiField>
		<div class="flex items-end">
			<UiButton variant="secondary" class="w-full" disabled={disabled} onclick={onAdd}>Add</UiButton>
		</div>
	</div>
	{#if error}
		<div class="alert alert-error mt-3 py-2 text-sm">{error}</div>
	{/if}
</div>
```

Create `app/src/lib/components/scan/ScanProgressPanel.svelte`:

```svelte
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

<div class="rounded-box border border-base-300 bg-base-100 p-3">
	<div class="mb-3 h-1.5 overflow-hidden rounded-full bg-base-300">
		<div class={`h-full bg-primary ${progress ? 'w-full' : 'w-1/3 animate-pulse'}`}></div>
	</div>
	{#if progress}
		<div class="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
			<UiStat label="Files" value={progress.files_processed} />
			<UiStat label="Processed" value={formatSize(progress.bytes_processed)} />
			<UiStat label="Stored" value={formatSize(progress.bytes_stored)} />
			<UiStat label="Duplicates" value={progress.duplicates_found} tone="error" />
			<UiStat label="Saved" value={formatSize(savedBytes)} tone="success" />
		</div>
		<div class="font-path mt-3 truncate text-xs text-base-content/55" title={progress.current_file}>
			{progress.current_file}
		</div>
	{:else if starting}
		<div class="font-path text-xs text-base-content/55">Starting scan...</div>
	{/if}
</div>
```

- [ ] **Step 3: Create `ScanDialog`**

Create `app/src/lib/components/scan/ScanDialog.svelte`:

```svelte
<script lang="ts">
	import type { CustomScanRule, ScanProgress, ScanRuleAction } from '$lib/api/tauri';
	import { UiButton, UiDialog, UiField } from '$lib/components/ui';
	import CustomScanRuleEditor from './CustomScanRuleEditor.svelte';
	import CustomScanRuleList from './CustomScanRuleList.svelte';
	import ScanPresetList from './ScanPresetList.svelte';
	import ScanProgressPanel from './ScanProgressPanel.svelte';

	interface Props {
		open: boolean;
		scanning: boolean;
		cancelling: boolean;
		savingCustomRules: boolean;
		source: string;
		targetPath: string;
		bundleGitDirs: boolean;
		ignoreRustTarget: boolean;
		ignoreNodeModules: boolean;
		ignorePythonVenv: boolean;
		customRules: CustomScanRule[];
		activeCustomRuleIds: string[];
		newRuleLabel: string;
		newRulePattern: string;
		newRuleAction: ScanRuleAction;
		customRulesError: string | null;
		scanError: string | null;
		progress: ScanProgress | null;
		onCloseOrCancel: () => void;
		onScan: () => void;
		onSourceChange: (value: string) => void;
		onTargetPathChange: (value: string) => void;
		onPresetChange: (id: string, checked: boolean) => void;
		onToggleCustomRule: (id: string, checked: boolean) => void;
		onRemoveCustomRule: (id: string) => void;
		onNewRuleLabelChange: (value: string) => void;
		onNewRulePatternChange: (value: string) => void;
		onNewRuleActionChange: (value: ScanRuleAction) => void;
		onAddCustomRule: () => void;
	}

	let props: Props = $props();

	let presetRows = $derived([
		{
			id: 'git',
			label: 'Archive .git directories',
			description: 'Store each Git directory as one archive file.',
			checked: props.bundleGitDirs
		},
		{
			id: 'rust',
			label: 'Ignore Rust target',
			description: 'Skip target directories generated by Cargo.',
			checked: props.ignoreRustTarget
		},
		{
			id: 'node',
			label: 'Ignore node_modules',
			description: 'Skip dependency trees from Node projects.',
			checked: props.ignoreNodeModules
		},
		{
			id: 'python',
			label: 'Ignore Python virtual envs',
			description: 'Skip .venv and venv directories.',
			checked: props.ignorePythonVenv
		}
	]);
</script>

<UiDialog
	open={props.open}
	title="Scan Directory"
	description="Choose a source, target path, and per-scan rules."
	wide
	onClose={props.onCloseOrCancel}
>
	<div class="space-y-5">
		<section class="grid gap-3 md:grid-cols-2">
			<UiField label="Source directory">
				<input
					class="font-path input input-sm input-bordered w-full"
					type="text"
					value={props.source}
					placeholder="/path/to/directory"
					disabled={props.scanning}
					oninput={(event) => props.onSourceChange(event.currentTarget.value)}
				/>
			</UiField>
			<UiField label="Virtual target path" hint='Use "/" for root, or "/photos/vacation" to nest'>
				<input
					class="font-path input input-sm input-bordered w-full"
					type="text"
					value={props.targetPath}
					placeholder="/"
					disabled={props.scanning}
					oninput={(event) => props.onTargetPathChange(event.currentTarget.value)}
				/>
			</UiField>
		</section>

		<section class="space-y-2">
			<h3 class="text-xs font-semibold uppercase text-base-content/60">Presets</h3>
			<ScanPresetList presets={presetRows} disabled={props.scanning} onToggle={props.onPresetChange} />
		</section>

		<section class="space-y-2">
			<h3 class="text-xs font-semibold uppercase text-base-content/60">Saved custom rules</h3>
			<CustomScanRuleList
				rules={props.customRules}
				activeRuleIds={props.activeCustomRuleIds}
				disabled={props.scanning || props.savingCustomRules}
				onToggle={props.onToggleCustomRule}
				onRemove={props.onRemoveCustomRule}
			/>
		</section>

		<section class="space-y-2">
			<h3 class="text-xs font-semibold uppercase text-base-content/60">Add custom rule</h3>
			<CustomScanRuleEditor
				label={props.newRuleLabel}
				pattern={props.newRulePattern}
				action={props.newRuleAction}
				disabled={props.scanning || props.savingCustomRules}
				error={props.customRulesError}
				onLabelChange={props.onNewRuleLabelChange}
				onPatternChange={props.onNewRulePatternChange}
				onActionChange={props.onNewRuleActionChange}
				onAdd={props.onAddCustomRule}
			/>
		</section>

		{#if props.scanning}
			<ScanProgressPanel progress={props.progress} starting={props.scanning} />
		{/if}

		{#if props.scanError}
			<div class="alert alert-error py-2 text-sm">{props.scanError}</div>
		{/if}

		<div class="flex justify-end gap-2">
			<UiButton variant="ghost" onclick={props.onCloseOrCancel} disabled={props.cancelling}>
				{props.scanning ? (props.cancelling ? 'Cancelling...' : 'Cancel Scan') : 'Cancel'}
			</UiButton>
			<UiButton
				variant="primary"
				loading={props.scanning}
				disabled={props.scanning || props.savingCustomRules || !props.source.trim()}
				onclick={props.onScan}
			>
				Start Scan
			</UiButton>
		</div>
	</div>
</UiDialog>
```

- [ ] **Step 4: Replace scan dialog markup in `+page.svelte`**

Add:

```ts
function handlePresetChange(id: string, checked: boolean) {
	if (id === 'git') bundleGitDirs = checked;
	if (id === 'rust') ignoreRustTarget = checked;
	if (id === 'node') ignoreNodeModules = checked;
	if (id === 'python') ignorePythonVenv = checked;
}
```

Use:

```svelte
<ScanDialog
	open={showScanDialog}
	{scanning}
	{cancelling}
	{savingCustomRules}
	source={scanSource}
	{targetPath}
	{bundleGitDirs}
	{ignoreRustTarget}
	{ignoreNodeModules}
	{ignorePythonVenv}
	customRules={customScanRules}
	{activeCustomRuleIds}
	{newRuleLabel}
	{newRulePattern}
	{newRuleAction}
	{customRulesError}
	{scanError}
	{progress}
	onCloseOrCancel={handleCancelScan}
	onScan={handleScan}
	onSourceChange={(value) => (scanSource = value)}
	onTargetPathChange={(value) => (targetPath = value)}
	onPresetChange={handlePresetChange}
	onToggleCustomRule={toggleCustomRule}
	onRemoveCustomRule={handleRemoveCustomRule}
	onNewRuleLabelChange={(value) => (newRuleLabel = value)}
	onNewRulePatternChange={(value) => (newRulePattern = value)}
	onNewRuleActionChange={(value) => (newRuleAction = value)}
	onAddCustomRule={handleAddCustomRule}
/>
```

Delete the old inline scan dialog markup and scan-dialog-specific CSS after the component renders.

- [ ] **Step 5: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no Svelte errors. Scan dialog props compile, and cancel remains wired to `handleCancelScan`.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/components/scan app/src/routes/+page.svelte
git commit -m "feat: extract scan workflow UI"
```

---

### Task 6: Refactor Main Page Layout

**Files:**
- Modify: `app/src/routes/+page.svelte`

- [ ] **Step 1: Simplify the content area**

Replace the old `<main class="content">` block in `+page.svelte` with this structure inside `AppShell`:

```svelte
{#if !hasWorkspace}
	<UiEmptyState title="Welcome to dedup" message="Create or import a workspace to start scanning.">
		{#snippet actions()}
			<UiButton variant="primary" onclick={openWorkspaceManager}>Manage Workspaces</UiButton>
		{/snippet}
	</UiEmptyState>
{:else if currentView === 'stats'}
	<div class="h-full overflow-auto p-3">
		<StatsPage />
	</div>
{:else}
	<div class="grid h-full min-h-0 grid-cols-1 lg:grid-cols-[20rem_minmax(0,1fr)]">
		<aside class="min-h-0 border-r border-base-300 bg-base-100">
			{#key treeRefreshKey}
				<FileTree {selectedPath} onSelect={handleSelect} onScanInto={openScanDialog} />
			{/key}
		</aside>
		<section class="min-h-0 overflow-hidden bg-base-100">
			{#if selectedPath && selectedEntry}
				<FileDetails path={selectedPath} entry={selectedEntry} />
			{:else}
				<UiEmptyState title="Select a file" message="Choose a stored path to inspect metadata and duplicate locations." />
			{/if}
		</section>
	</div>
{/if}
```

Add imports:

```ts
import { UiButton, UiEmptyState } from '$lib/components/ui';
```

- [ ] **Step 2: Remove obsolete page styles**

Delete the old `<style>` block from `+page.svelte` after all referenced local class names are gone. Keep no component-specific CSS in `+page.svelte`.

- [ ] **Step 3: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no unused class dependencies or Svelte errors.

- [ ] **Step 4: Commit**

```bash
git add app/src/routes/+page.svelte
git commit -m "refactor: simplify main page layout"
```

---

### Task 7: Restyle File Browser Components

**Files:**
- Modify: `app/src/lib/components/FileTree.svelte`
- Modify: `app/src/lib/components/TreeNode.svelte`

- [ ] **Step 1: Convert `FileTree` to Tailwind/DaisyUI**

Replace the markup after the script in `FileTree.svelte` with:

```svelte
<div class="flex h-full min-h-0 flex-col">
	<header class="flex h-10 shrink-0 items-center justify-between border-b border-base-300 px-3">
		<span class="text-sm font-semibold">Files</span>
		<div class="flex gap-1">
			<button class="btn btn-ghost btn-xs" type="button" onclick={() => onScanInto('/')} title="Scan into root">
				+
			</button>
			<button class="btn btn-ghost btn-xs" type="button" onclick={loadRoot} title="Refresh files">
				Refresh
			</button>
		</div>
	</header>

	{#if error}
		<div class="alert alert-error m-3 py-2 text-sm">{error}</div>
	{:else if rootEntries.length === 0}
		<div class="p-3">
			<div class="rounded-box border border-base-300 bg-base-200 p-4">
				<p class="text-sm text-base-content/60">No files yet.</p>
				<button class="btn btn-primary btn-sm mt-3" type="button" onclick={() => onScanInto('/')}>
					Scan a directory
				</button>
			</div>
		</div>
	{:else}
		<ul class="min-h-0 flex-1 overflow-y-auto p-2">
			{#each rootEntries as entry (entry.name)}
				<TreeNode {entry} parentPath="/" {selectedPath} {onSelect} {onScanInto} />
			{/each}
		</ul>
	{/if}
</div>
```

Remove the old `<style>` block.

- [ ] **Step 2: Convert `TreeNode` to Tailwind/DaisyUI**

Replace the markup after the script in `TreeNode.svelte` with:

```svelte
<li class="list-none">
	<div class="group flex items-center gap-1">
		<button
			class={`min-w-0 flex flex-1 items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-base-200 ${isSelected ? 'bg-primary text-primary-content' : ''}`}
			type="button"
			onclick={toggle}
		>
			<span class="shrink-0 text-xs">{entry.is_dir ? (expanded ? '▾' : '▸') : '·'}</span>
			<span class="min-w-0 flex-1 truncate">{entry.name}</span>
			{#if !entry.is_dir}
				<span class="font-path shrink-0 text-[11px] opacity-60">{formatSize(entry.size)}</span>
			{/if}
			{#if loading}
				<span class="loading loading-spinner loading-xs"></span>
			{/if}
		</button>
		{#if entry.is_dir}
			<button
				class="btn btn-ghost btn-xs invisible group-hover:visible"
				type="button"
				onclick={handleScanInto}
				title="Scan into this directory"
				aria-label={`Scan into ${entry.name}`}
			>
				+
			</button>
		{/if}
	</div>

	{#if expanded && children.length > 0}
		<ul class="pl-4">
			{#each children as child (child.name)}
				<TreeNode
					entry={child}
					parentPath={fullPath}
					{selectedPath}
					{onSelect}
					{onScanInto}
				/>
			{/each}
		</ul>
	{/if}
</li>
```

Remove the old `<style>` block.

- [ ] **Step 3: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/components/FileTree.svelte app/src/lib/components/TreeNode.svelte
git commit -m "style: redesign file browser"
```

---

### Task 8: Restyle File Details And Stats

**Files:**
- Modify: `app/src/lib/components/FileDetails.svelte`
- Modify: `app/src/lib/components/StatsPage.svelte`

- [ ] **Step 1: Restyle `FileDetails` without changing data loading**

Keep the existing `<script>` behavior. Replace the top-level markup with Tailwind/DaisyUI structure:

```svelte
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
			<button class="btn btn-primary btn-sm mt-3 w-full" type="button" onclick={handleOpen} disabled={opening}>
				{opening ? 'Opening...' : 'Open'}
			</button>
		{/if}

		{#if isImage && previewUrl}
			<div class="rounded-box mt-4 flex min-h-28 items-center justify-center overflow-hidden border border-base-300 bg-base-200">
				<img class="max-h-96 max-w-full object-contain" src={previewUrl} alt={entry.name} />
			</div>
		{:else if isImage && loading}
			<div class="rounded-box mt-4 flex h-28 items-center justify-center border border-base-300 bg-base-200 text-sm text-base-content/60">
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
					<div class="font-path mt-1 text-sm font-semibold">{formatSize(metadata.original_size)}</div>
				</div>
				<div class="rounded-box border border-base-300 bg-base-200 p-3">
					<div class="text-xs text-base-content/50">Stored</div>
					<div class="font-path mt-1 text-sm font-semibold">{formatSize(metadata.compressed_size)}</div>
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
					<div class="font-path mt-1 text-sm font-semibold">{formatTimestamp(metadata.modified)}</div>
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
							<li class={dup === path ? 'font-semibold text-error-content' : 'opacity-80'}>{dup}</li>
						{/each}
					</ul>
				</div>
			{/if}
		{/if}
	</div>
</div>
```

Remove the old `<style>` block.

- [ ] **Step 2: Restyle `StatsPage` incrementally**

Keep the existing stats loading/data logic. Replace card/table classes with DaisyUI/Tailwind utilities:

- Top-level wrapper: `class="space-y-4"`
- Summary grid: `class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4"`
- Summary cards: `class="rounded-box border border-base-300 bg-base-200 p-3"`
- Tables/lists: `class="rounded-box overflow-hidden border border-base-300 bg-base-200"`
- Error state: `class="alert alert-error"`
- Loading state: `class="loading loading-spinner loading-md"`

Preserve all current data fields and labels while removing the old `<style>` block.

- [ ] **Step 3: Run checks**

Run:

```bash
npm --prefix app run check
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/components/FileDetails.svelte app/src/lib/components/StatsPage.svelte
git commit -m "style: redesign details and stats"
```

---

### Task 9: Final Polish, Responsive Pass, And Verification

**Files:**
- Modify any frontend file touched by Tasks 1-8 only if needed for responsive or type fixes.

- [ ] **Step 1: Run full frontend verification**

Run:

```bash
npm --prefix app run check
npm --prefix app run build
```

Expected:

- `svelte-check found 0 errors and 0 warnings`
- Vite build succeeds.

- [ ] **Step 2: Run Rust verification if TypeScript API contracts changed**

If `app/src/lib/api/tauri.ts`, `app/src-tauri`, or Rust crates were changed during implementation, run:

```bash
cargo check
cargo test -p dedup-core
cargo test -p dedup-app
```

Expected: all pass.

- [ ] **Step 3: Run local app for visual verification**

Start the dev server:

```bash
npm --prefix app run dev
```

Expected: Vite serves the app on `http://localhost:1420`.

Use browser verification to inspect:

- Desktop viewport around `1440x900`.
- Narrow viewport around `390x844`.
- Top bar does not overlap at either size.
- Workspace dialog list/create/import states fit within the viewport.
- Scan dialog source/target, presets, saved rules, custom editor, progress panel, and cancel button remain visible and usable.
- Long paths and regex patterns truncate or wrap intentionally.
- DaisyUI Night theme is active.

- [ ] **Step 4: Fix visual or accessibility defects**

For any defect found in Step 3, patch the smallest responsible component. Examples:

```svelte
class="min-w-0 truncate"
```

for a single-line label, or:

```svelte
class="break-all"
```

for path/regex text that must wrap.

Re-run:

```bash
npm --prefix app run check
npm --prefix app run build
```

Expected: both pass after every fix.

- [ ] **Step 5: Commit final polish**

```bash
git add app
git commit -m "fix: polish responsive UI redesign"
```

---

## Completion Criteria

- Tailwind CSS v4 and DaisyUI v5 are installed and active.
- DaisyUI Night is the default theme.
- `+page.svelte` is primarily orchestration and composition.
- App shell, workspace dialog, scan dialog, file tree, file details, and stats use extracted components or Tailwind/DaisyUI styling.
- Scan-rule behavior and ordering are unchanged.
- Scan cancellation remains wired and visible during active scans.
- Workspace import/export behavior remains wired.
- `npm --prefix app run check` passes.
- `npm --prefix app run build` passes.
- Browser verification confirms desktop and narrow layouts have no incoherent overlap.
