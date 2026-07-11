<script lang="ts">
	type Tone = 'neutral' | 'primary' | 'success' | 'error' | 'warning' | 'info';

	interface Props {
		tone?: Tone;
		text: string;
		class?: string;
	}

	let { tone = 'neutral', text, class: className = '' }: Props = $props();

	const toneClass: Record<Tone, string> = {
		neutral: 'badge-neutral',
		primary: 'badge-primary',
		success: 'badge-success',
		error: 'badge-error',
		warning: 'badge-warning',
		info: 'badge-info'
	};
</script>

<span class={`badge ${toneClass[tone]} badge-sm max-w-full truncate ${className}`}>{text}</span>
