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
		<div
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
		</div>
	</div>
{/if}
