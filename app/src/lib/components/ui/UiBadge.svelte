<script lang="ts">
	interface Props {
		tone?: 'neutral' | 'primary' | 'success' | 'error' | 'warning' | 'info';
		text: string;
		class?: string;
	}

	let { tone = 'neutral', text, class: className = '' }: Props = $props();
</script>

<span class={`badge badge-${tone} badge-sm max-w-full truncate ${className}`}>{text}</span>
