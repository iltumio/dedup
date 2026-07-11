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

<div class="min-w-0 rounded-box border border-base-300 bg-base-200 px-3 py-2">
	<div class="text-[11px] uppercase text-base-content/50">{label}</div>
	<div class={`font-path mt-1 truncate text-sm font-semibold ${toneClass[tone]}`}>{value}</div>
</div>
