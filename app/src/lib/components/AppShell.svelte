<script lang="ts">
  import UiSelect from "./ui/UiSelect.svelte";
  import type { Snippet } from "svelte";
  import Icon from "./ui/Icon.svelte";
  import { app } from "$lib/state/app.svelte";
  export type View = "files" | "duplicates" | "activity" | "stats";
  let {
    currentView,
    hasWorkspace,
    scanning,
    onViewChange,
    onScan,
    workspaceControl,
    children,
  }: {
    currentView: View;
    hasWorkspace: boolean;
    scanning: boolean;
    onViewChange: (view: View) => void;
    onScan: () => void;
    workspaceControl?: Snippet;
    children?: Snippet;
  } = $props();
  const views: { id: View; label: string; icon: string }[] = [
    { id: "files", label: "Files", icon: "folder" },
    { id: "duplicates", label: "Duplicates", icon: "copies" },
    { id: "activity", label: "Activity", icon: "activity" },
    { id: "stats", label: "Overview", icon: "chart" },
  ];
</script>

<div class="app-shell">
  <aside class="app-sidebar">
    <a class="brand" href="/" aria-label="dedup home"
      ><span class="brand-mark"><Icon name="copies" /></span>dedup<span
        class="brand-dot">.</span
      ></a
    >
    <div class="sidebar-label">YOUR ARCHIVE</div>
    {@render workspaceControl?.()}
    <nav aria-label="Main navigation" class="app-nav">
      {#each views as view}
        <button
          type="button"
          class:active={currentView === view.id}
          aria-current={currentView === view.id ? "page" : undefined}
          disabled={!hasWorkspace || (scanning && view.id !== "activity")}
          onclick={() => onViewChange(view.id)}
          ><Icon
            name={view.icon}
          />{view.label}{#if view.id === "activity" && scanning}<span
              class="live-dot"
            ></span>{/if}</button
        >
      {/each}
    </nav>
    <div class="sidebar-bottom">
      <p>One content.<br />Every file, preserved.</p>
      <div class="theme-control">
        Appearance<UiSelect
          label="Appearance"
          value={app.theme}
          onValueChange={(value) =>
            app.setTheme(value as "system" | "light" | "dark")}
          options={[
            { value: "system", label: "System" },
            { value: "light", label: "Light" },
            { value: "dark", label: "Dark" },
          ]}
        />
      </div>
    </div>
  </aside>
  <div class="app-content">
    <header class="app-toolbar">
      <div>
        <span class="eyebrow"
          >{app.activeWorkspace?.label ?? "GET STARTED"}</span
        >
        <h1>{views.find((v) => v.id === currentView)?.label ?? "Files"}</h1>
      </div>
      <button
        type="button"
        class="btn btn-primary"
        disabled={!hasWorkspace || scanning}
        onclick={onScan}><Icon name="plus" size={18} />Add folder</button
      >
    </header>
    <main class="app-main">{@render children?.()}</main>
  </div>
</div>
