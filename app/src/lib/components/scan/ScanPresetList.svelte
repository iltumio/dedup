<script lang="ts">
	interface ScanPreset {
		id: string;
		label: string;
		description: string;
		checked: boolean;
	}

	interface Props {
		presets: ScanPreset[];
		disabled?: boolean;
		onToggle: (id: string, checked: boolean) => void;
	}

	let { presets, disabled = false, onToggle }: Props = $props();

	function checkedValue(event: Event) {
		return (event.currentTarget as HTMLInputElement).checked;
	}
</script>

<div class="grid gap-2">
	{#each presets as preset (preset.id)}
		<label class="flex items-start gap-3 rounded-box border border-base-300 bg-base-100 px-3 py-2">
			<input
				class="toggle toggle-primary toggle-sm mt-0.5 shrink-0"
				type="checkbox"
				checked={preset.checked}
				{disabled}
				onchange={(event) => onToggle(preset.id, checkedValue(event))}
			/>
			<span class="min-w-0">
				<span class="block text-sm font-medium">{preset.label}</span>
				<span class="block text-xs leading-5 text-base-content/60">{preset.description}</span>
			</span>
		</label>
	{/each}
</div>
