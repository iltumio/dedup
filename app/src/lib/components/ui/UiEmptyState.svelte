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
