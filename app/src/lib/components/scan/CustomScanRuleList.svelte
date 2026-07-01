<script lang="ts">
	import type { CustomScanRule } from '$lib/api/tauri';
	import { UiBadge, UiButton, UiEmptyState } from '$lib/components/ui';

	interface Props {
		rules: CustomScanRule[];
		activeRuleIds: string[];
		disabled?: boolean;
		onToggle: (id: string, checked: boolean) => void;
		onRemove: (id: string) => void;
	}

	let { rules, activeRuleIds, disabled = false, onToggle, onRemove }: Props = $props();

	const componentId = $props.id();

	function checkedValue(event: Event) {
		return (event.currentTarget as HTMLInputElement).checked;
	}
</script>

{#if rules.length === 0}
	<div class="rounded-box border border-base-300 bg-base-100">
		<UiEmptyState title="No saved custom rules" message="Add a rule below to reuse it in future scans." />
	</div>
{:else}
	<ul class="flex list-none flex-col gap-2 p-0">
		{#each rules as rule (rule.id)}
			<li class="rounded-box border border-base-300 bg-base-100 p-3">
				<div class="flex items-start gap-3">
					<input
						id={`${componentId}-${rule.id}`}
						class="checkbox checkbox-primary checkbox-sm mt-0.5 shrink-0"
						type="checkbox"
						checked={activeRuleIds.includes(rule.id)}
						{disabled}
						onchange={(event) => onToggle(rule.id, checkedValue(event))}
					/>
					<label class="min-w-0 flex-1" for={`${componentId}-${rule.id}`}>
						<span class="flex min-w-0 flex-wrap items-center gap-2">
							<span class="truncate text-sm font-medium">{rule.label}</span>
							{#if rule.action === 'ignore'}
								<UiBadge tone="warning" text="ignore" />
							{:else}
								<UiBadge tone="info" text="archive" />
							{/if}
						</span>
						<span class="font-path mt-1 block truncate text-xs text-base-content/55" title={rule.pattern}>
							{rule.pattern}
						</span>
					</label>
					<UiButton
						variant="ghost"
						size="xs"
						title={`Remove ${rule.label}`}
						ariaLabel={`Remove ${rule.label}`}
						disabled={disabled}
						onclick={() => onRemove(rule.id)}
					>
						Remove
					</UiButton>
				</div>
			</li>
		{/each}
	</ul>
{/if}
