<script lang="ts">
	import type { ScanRuleAction } from '$lib/api/tauri';
	import { UiButton, UiField } from '$lib/components/ui';

	interface Props {
		label: string;
		pattern: string;
		action: ScanRuleAction;
		disabled?: boolean;
		error?: string | null;
		onLabelChange: (value: string) => void;
		onPatternChange: (value: string) => void;
		onActionChange: (value: ScanRuleAction) => void;
		onAdd: () => void;
	}

	let {
		label,
		pattern,
		action,
		disabled = false,
		error = null,
		onLabelChange,
		onPatternChange,
		onActionChange,
		onAdd
	}: Props = $props();

	function inputValue(event: Event) {
		return (event.currentTarget as HTMLInputElement).value;
	}

	function actionValue(event: Event) {
		return (event.currentTarget as HTMLSelectElement).value as ScanRuleAction;
	}
</script>

<div class="flex flex-col gap-3">
	<div class="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_8rem_auto]">
		<UiField label="Rule label">
			<input
				class="input input-bordered input-sm w-full"
				type="text"
				value={label}
				placeholder="Rule label"
				disabled={disabled}
				oninput={(event) => onLabelChange(inputValue(event))}
			/>
		</UiField>

		<UiField label="Regex">
			<input
				class="font-path input input-bordered input-sm w-full"
				type="text"
				value={pattern}
				placeholder="(^|/)dist$"
				disabled={disabled}
				oninput={(event) => onPatternChange(inputValue(event))}
			/>
		</UiField>

		<UiField label="Action">
			<select
				class="select select-bordered select-sm w-full"
				value={action}
				disabled={disabled}
				onchange={(event) => onActionChange(actionValue(event))}
			>
				<option value="ignore">Ignore</option>
				<option value="archive">Archive</option>
			</select>
		</UiField>

		<div class="flex items-end">
			<UiButton class="w-full" variant="secondary" disabled={disabled} onclick={onAdd}>Add Rule</UiButton>
		</div>
	</div>

	{#if error}
		<div class="alert alert-error py-2 text-sm" role="alert">
			<span>{error}</span>
		</div>
	{/if}
</div>
