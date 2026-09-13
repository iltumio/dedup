<script lang="ts">
  import type { Snippet } from "svelte";
  let {
    label,
    hint,
    error = null,
    children,
  }: {
    label: string;
    hint?: string;
    error?: string | null;
    children?: Snippet;
  } = $props();
  const id = $props.id();
  let field: HTMLDivElement;
  let controlId = $state("");
  $effect(() => {
    const described = [hint ? `${id}-hint` : "", error ? `${id}-error` : ""]
      .filter(Boolean)
      .join(" ");
    const control = field?.querySelector(
      'input:not([type="hidden"]), select, textarea, button[data-select-trigger]',
    );
    if (control) {
      if (!control.id) control.id = id;
      controlId = control.id;
      control.setAttribute("aria-describedby", described);
      control.setAttribute("aria-invalid", String(Boolean(error)));
    }
  });
</script>

<div class="field" bind:this={field}>
  <label for={controlId || id}>{label}</label
  >{@render children?.()}{#if hint}<span id={`${id}-hint`} class="field-hint"
      >{hint}</span
    >{/if}{#if error}<span id={`${id}-error`} class="text-sm text-error"
      >{error}</span
    >{/if}
</div>
