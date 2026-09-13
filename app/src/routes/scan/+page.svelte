<script lang="ts">
  import { goto } from "$app/navigation";
  import { UiButton, UiEmptyState, UiField } from "$lib/components/ui";
  import CustomScanRuleEditor from "$lib/components/scan/CustomScanRuleEditor.svelte";
  import CustomScanRuleList from "$lib/components/scan/CustomScanRuleList.svelte";
  import ScanPresetList from "$lib/components/scan/ScanPresetList.svelte";
  import ScanActivity from "$lib/components/scan/ScanActivity.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  let showActivity = $state(app.scanning);
  import { pickDirectory } from "$lib/api/tauri";
  import { app } from "$lib/state/app.svelte";

  let presets = $derived([
    {
      id: "git",
      label: "Archive .git directories",
      description: "Store repository metadata as bundled content.",
      checked: app.bundleGitDirs,
    },
    {
      id: "rust",
      label: "Ignore Rust target directories",
      description: "Skip Cargo build output directories named target.",
      checked: app.ignoreRustTarget,
    },
    {
      id: "node",
      label: "Ignore node_modules directories",
      description: "Skip installed JavaScript dependency trees.",
      checked: app.ignoreNodeModules,
    },
    {
      id: "python",
      label: "Ignore Python virtual environments",
      description: "Skip directories named .venv or venv.",
      checked: app.ignorePythonVenv,
    },
  ]);

  let rulesDisabled = $derived(app.scanning || app.savingCustomRules);
  let startDisabled = $derived(
    app.scanning || app.savingCustomRules || !app.scanSource.trim(),
  );

  function inputValue(event: Event) {
    return (event.currentTarget as HTMLInputElement).value;
  }

  async function browseSource() {
    if (app.scanning) return;
    try {
      const dir = await pickDirectory("Select source directory");
      if (dir) app.scanSource = dir;
    } catch (e) {
      app.scanError = String(e);
    }
  }

  async function handleCloseOrCancel() {
    if (app.scanning) {
      await app.requestCancel();
      return;
    }
    goto("/");
  }

  async function startScan() {
    if (startDisabled) return;
    showActivity = true;
    await app.runScan();
  }
</script>

<div class="scan-page">
  <header class="scan-header">
    <button
      class="btn btn-ghost"
      type="button"
      disabled={app.scanning}
      onclick={() => goto("/")}
      ><Icon name="back" size={18} />Back to archive</button
    ><span class="muted"
      >{app.activeWorkspace?.label ?? "No archive selected"}</span
    >
  </header>
  {#if showActivity}
    <ScanActivity
      onFiles={() => goto("/")}
      onDuplicates={() => goto("/?view=duplicates")}
      onNew={() => {
        app.prepareScan();
        showActivity = false;
      }}
    />
  {:else if !app.hasWorkspace}
    <UiEmptyState
      title="Choose an archive first"
      message="Create or open an archive before adding files."
      >{#snippet actions()}<UiButton variant="primary" onclick={() => goto("/")}
          >Choose archive</UiButton
        >{/snippet}</UiEmptyState
    >
  {:else}
    <main class="scan-form-scroll">
      <div class="scan-form">
        <div class="eyebrow">ADD TO YOUR ARCHIVE</div>
        <h1 class="page-title">A folder. One safe copy.</h1>
        <p class="muted">
          Choose a folder to archive. Identical content is stored only once.
        </p>
        <section class="source-card">
          <div class="source-icon"><Icon name="folder" size={28} /></div>
          <div class="grow min-w-0">
            <UiField label="Folder to add"
              ><input
                class="input w-full font-path"
                type="text"
                value={app.scanSource}
                placeholder="/home/you/Photos"
                oninput={(event) => (app.scanSource = inputValue(event))}
              /></UiField
            >
          </div>
          <UiButton onclick={browseSource}>Browse</UiButton>
        </section>
        <div class="notice">
          <Icon name="check" size={18} />
          <div>
            <strong>Original files stay where they are.</strong>
            <p>
              Content is copied into <b>{app.activeWorkspace?.label}</b>. This
              does not free space in your source folder.
            </p>
          </div>
        </div>
        <details class="disclosure">
          <summary
            ><span class="flex items-center gap-2"
              ><Icon name="settings" size={18} />Advanced options</span
            ><span>Location & exclusions</span></summary
          >
          <div class="detail-content space-y-6">
            <UiField
              label="Folder inside the archive"
              hint="Use / for the archive root, or a path such as /photos."
              ><input
                class="input w-full font-path"
                value={app.targetPath}
                oninput={(event) => (app.targetPath = inputValue(event))}
              /></UiField
            >
            <section>
              <h2 class="section-heading">
                Exclude or bundle technical folders
              </h2>
              <p class="muted text-sm mb-3">
                Optional. Only the selected rules will apply.
              </p>
              <ScanPresetList
                {presets}
                disabled={false}
                onToggle={app.handlePresetChange}
              />
            </section>
            {#if app.customScanRules.length}<section>
                <h2 class="section-heading">Saved rules</h2>
                <CustomScanRuleList
                  rules={app.customScanRules}
                  activeRuleIds={app.activeCustomRuleIds}
                  disabled={rulesDisabled}
                  onToggle={app.toggleCustomRule}
                  onRemove={app.removeCustomRule}
                />
              </section>{/if}
            <details class="disclosure">
              <summary>Create a reusable rule</summary>
              <div class="detail-content">
                <CustomScanRuleEditor
                  label={app.newRuleLabel}
                  pattern={app.newRulePattern}
                  action={app.newRuleAction}
                  disabled={rulesDisabled}
                  error={app.customRulesError}
                  onLabelChange={(value) => (app.newRuleLabel = value)}
                  onPatternChange={(value) => (app.newRulePattern = value)}
                  onActionChange={(value) => (app.newRuleAction = value)}
                  onAdd={app.addCustomRule}
                />
              </div>
            </details>
          </div>
        </details>
        {#if app.scanError}<div class="notice notice-error" role="alert">
            {app.scanError}
          </div>{/if}
      </div>
    </main>
    <footer class="scan-form-actions">
      <span class="muted text-sm"
        >Destination: {app.activeWorkspace?.label}</span
      ><UiButton variant="primary" disabled={startDisabled} onclick={startScan}
        >Start scan<Icon name="chevron" size={16} /></UiButton
      >
    </footer>
  {/if}
</div>
