<script lang="ts">
  import { Select } from "bits-ui";
  import Icon from "./Icon.svelte";

  let {
    value = $bindable(""),
    options,
    label,
    placeholder = "Choose an option",
    disabled = false,
    class: className = "",
    onValueChange,
  }: {
    value?: string;
    options: { value: string; label: string; disabled?: boolean }[];
    label: string;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
    onValueChange?: (value: string) => void;
  } = $props();

  const componentId = $props.id();
  const contentId = `${componentId}-options`;
  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  // Keep popups inside the native dialog's top layer when used in a modal.
  const portalTarget = $derived(trigger?.closest("dialog") ?? undefined);
</script>

<Select.Root
  type="single"
  bind:value
  bind:open
  {disabled}
  {onValueChange}
  items={options}
  allowDeselect={false}
>
  <Select.Trigger
    bind:ref={trigger}
    class={`ui-select ${className}`}
    aria-label={label}
    role="combobox"
    aria-controls={open ? contentId : undefined}
  >
    <span class="ui-select-value"
      >{options.find((option) => option.value === value)?.label ??
        placeholder}</span
    >
    <span class="ui-select-chevron"><Icon name="chevron" size={15} /></span>
  </Select.Trigger>
  <Select.Portal to={portalTarget}>
    <Select.Content
      id={contentId}
      class="ui-select-menu"
      sideOffset={6}
      align="start"
      collisionPadding={8}
      aria-label={label}
    >
      {#each options as option (option.value)}
        <Select.Item
          class="ui-select-option"
          value={option.value}
          label={option.label}
          disabled={option.disabled}
        >
          {#snippet children({ selected })}
            <span>{option.label}</span>
            <span class="ui-select-check"
              >{#if selected}<Icon name="check" size={16} />{/if}</span
            >
          {/snippet}
        </Select.Item>
      {/each}
    </Select.Content>
  </Select.Portal>
</Select.Root>
