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
