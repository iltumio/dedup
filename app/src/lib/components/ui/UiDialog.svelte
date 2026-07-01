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

	let dialog: HTMLDialogElement;
	let previousActiveElement: HTMLElement | null = null;

	const componentId = $props.id();
	const titleId = `${componentId}-title`;
	const descriptionId = `${componentId}-description`;

	$effect(() => {
		if (!dialog) {
			return;
		}

		if (open && !dialog.open) {
			previousActiveElement =
				document.activeElement instanceof HTMLElement ? document.activeElement : null;
			dialog.showModal();
		} else if (!open && dialog.open) {
			dialog.close();
		}
	});

	function handleCancel(event: Event) {
		event.preventDefault();
		onClose();
	}

	function restoreFocus() {
		if (previousActiveElement && document.contains(previousActiveElement)) {
			previousActiveElement.focus();
		}

		previousActiveElement = null;
	}
</script>

<dialog
	bind:this={dialog}
	class="modal bg-transparent p-0 text-base-content"
	aria-modal="true"
	aria-labelledby={titleId}
	aria-describedby={description ? descriptionId : undefined}
	oncancel={handleCancel}
	onclose={restoreFocus}
>
	<div
		class={`modal-box max-h-[calc(100vh-1.5rem)] w-[calc(100vw-1.5rem)] overflow-hidden border border-base-300 bg-base-200 p-0 shadow-xl ${wide ? 'max-w-4xl' : 'max-w-2xl'}`}
	>
		<header class="flex items-start justify-between gap-4 border-b border-base-300 px-4 py-3">
			<div class="min-w-0">
				<h2 id={titleId} class="truncate text-sm font-semibold">{title}</h2>
				{#if description}
					<p id={descriptionId} class="mt-1 text-xs text-base-content/60">{description}</p>
				{/if}
			</div>
			<button
				class="btn btn-ghost btn-xs shrink-0"
				type="button"
				aria-label="Close dialog"
				onclick={onClose}
			>
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
	</div>
</dialog>

<style>
	dialog::backdrop {
		background: rgb(0 0 0 / 65%);
	}
</style>
