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

<div class="join" role="group" aria-label={ariaLabel}>
	{#each options as option}
		<button
			type="button"
			aria-pressed={value === option.value}
			class={`btn btn-xs join-item ${value === option.value ? 'btn-primary' : 'btn-neutral'}`}
			onclick={() => onChange(option.value)}
		>
			{option.label}
		</button>
	{/each}
</div>
