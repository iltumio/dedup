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

	const sizeClass: Record<Size, string> = {
		xs: 'btn-xs',
		sm: 'btn-sm',
		md: 'btn-md'
	};
</script>

<button
	{type}
	{title}
	aria-label={ariaLabel}
	class={`btn ${sizeClass[size]} ${variantClass[variant]} ${loading ? 'btn-disabled' : ''} ${className}`}
	disabled={disabled || loading}
	{onclick}
>
	{#if loading}
		<span class="loading loading-spinner loading-xs" aria-hidden="true"></span>
	{/if}
	{@render children?.()}
</button>
