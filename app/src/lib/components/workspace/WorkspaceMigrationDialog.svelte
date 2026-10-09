<script lang="ts">
  import { formatSize, pickDirectory } from "$lib/api/tauri";
  import { UiButton, UiDialog, UiField } from "$lib/components/ui";
  import { app } from "$lib/state/app.svelte";

  let { open, onClose, onBack, onOpenArchive }: {
    open: boolean;
    onClose: () => void;
    onBack: () => void;
    onOpenArchive: (id: string) => void;
  } = $props();
  const activity = $derived(app.migration);
  const completed = $derived(activity?.status === "completed");
  const locked = $derived(app.migrating || app.checkingMigration || completed);
  const title = $derived(completed ? "Migration complete" : app.migrating ? "Migration in progress"
    : activity?.status === "cancelled" ? "Migration stopped" : activity?.status === "failed" ? "Migration could not finish" : "Migrate to FastCDC");
  let now = $state(Date.now());
  $effect(() => {
    if (!open || !app.migrating) return;
    now = Date.now();
    const timer = setInterval(() => now = Date.now(), 1000);
    return () => clearInterval(timer);
  });
  const updateAge = $derived(Math.max(0, (now - (activity?.lastProgressAt ?? activity?.startedAt ?? now)) / 1000));
  const dataIdle = $derived((activity?.progress?.idle_seconds ?? 0) + updateAge);
  const preparing = $derived(!activity?.progress || activity.progress.phase === "preparing");
  const finalizing = $derived(activity?.progress?.phase === "finalizing" || activity?.progress?.phase === "completed");
  const totals = $derived(activity?.progress?.finalization);
  const elapsed = $derived(activity?.startedAt ? Math.max(0, ((app.migrating ? now : activity.finishedAt ?? now) - activity.startedAt) / 1000) : 0);
  const rate = $derived(updateAge > 2 ? 0 : activity?.progress?.rate_bytes_per_second ?? 0);
  const workerSlots = $derived(Array.from({ length: Math.min(32, Math.max(1, activity?.workers || 1)) }, (_, index) => index));
  function duration(seconds: number) {
    const whole = Math.floor(seconds);
    return `${Math.floor(whole / 3600).toString().padStart(2, "0")}:${Math.floor((whole % 3600) / 60).toString().padStart(2, "0")}:${(whole % 60).toString().padStart(2, "0")}`;
  }
  const stageLabel = { converting: "Converting", verifying: "Verifying", saving_checkpoint: "Saving checkpoint" };
  let browsing = $state(false);
  async function browse() {
    browsing = true;
    try {
      const destination = await pickDirectory("Choose an empty folder or resume destination");
      if (destination && activity) activity.destination = destination;
    } catch (e) { if (activity) activity.error = String(e); }
    finally { browsing = false; }
  }
</script>

<UiDialog {open} {title} size="extra-wide" closeDisabled={app.migrating || app.checkingMigration || browsing} {onClose}>
  {#if activity}
    <div class="flex flex-col gap-4">
      <p class="text-sm">{activity.workspace.label}</p>
      {#if completed}
        <p role="status">Your FastCDC archive has been added to the archive list. The original archive is still available.</p>
        <p class="font-path text-sm break-all">{activity.result?.store_path}</p>
      {:else if !app.migrating}
        <p class="muted text-sm">Share unchanged parts of similar files. The original archive stays untouched; allow space for both archives during conversion.</p>
        {#if app.migrationFormat === 2}<p class="notice" role="status">This archive already uses FastCDC.</p>{/if}
        {#if app.checkingMigration}<p class="notice" role="status">Checking archive format…</p>{/if}
        {#if activity.status === "cancelled" || activity.status === "failed"}
          <p class="notice">If conversion already started, verified content remains at the destination. Use the same location to resume.</p>
        {/if}
        <fieldset disabled={locked || browsing} class="min-w-0 grid gap-3">
          <UiField label="New archive name">
            <input class="input input-bordered input-sm w-full" type="text" value={activity.label}
              oninput={(event) => activity.label = event.currentTarget.value} />
          </UiField>
          <UiField label="Destination folder" hint="Choose an empty folder, including a dedicated subfolder on the same disk, or this migration's previous destination. Avoid the original archive's blobs folder.">
            <div class="flex gap-2">
              <input class="input input-bordered input-sm min-w-0 flex-1 font-path" type="text" value={activity.destination}
                oninput={(event) => activity.destination = event.currentTarget.value} />
              <UiButton onclick={browse} loading={browsing}>Browse</UiButton>
            </div>
          </UiField>
          <UiField label="Parallel files" hint="How many files to process at once (1–32). You can change this when resuming.">
            <input class="input input-bordered input-sm w-24" type="number" min="1" max="32" step="1" value={activity.workers}
              oninput={(event) => activity.workers = event.currentTarget.valueAsNumber} />
          </UiField>
        </fieldset>
      {/if}
      {#if app.migrating || activity.progress}
        <div class="grid gap-3">
          {#if app.migrating}
            <div class="notice grid gap-1" role="status">
              <p class="flex items-center gap-2 text-sm">
                <span class="loading loading-spinner loading-xs motion-reduce:animate-none" aria-hidden="true"></span>
                {preparing ? "Preparing archive…" : finalizing ? totals?.phase === "reading_manifests" ? "Reading archive index…"
                  : totals?.phase === "counting_blobs" ? "Calculating stored size…" : "Calculating totals and adding the archive…"
                  : `${activity.progress!.active_files.length} of ${activity.progress!.workers} parallel files active`}
              </p>
              <p class="text-xs muted">{updateAge >= 5 ? `Waiting for an update · last update ${Math.floor(updateAge)}s ago` : "Updates arriving · last update just now"}</p>
              <div class="min-h-4">
                {#if !preparing && !finalizing && dataIdle >= 15}
                  <p class="text-xs">No data progress for {Math.floor(dataIdle)}s. The disk may be busy.</p>
                {/if}
              </div>
            </div>
            <p class="font-path text-xs muted break-all">{activity.destination}</p>
          {/if}
          <dl class="grid grid-cols-2 gap-2 text-xs sm:grid-cols-3">
            <div><dt class="muted">Elapsed</dt><dd class="tabular-nums text-sm">{duration(elapsed)}</dd></div>
            <div><dt class="muted">Read &amp; verification rate</dt><dd class="tabular-nums text-sm">{formatSize(rate)}/s</dd></div>
            <div><dt class="muted">Data processed this run</dt><dd class="tabular-nums text-sm">{formatSize(activity.progress?.work_bytes ?? 0)}</dd></div>
          </dl>
          <p class="text-xs muted">Processing includes uncompressed reads and verification, not disk write speed.</p>
          <div class="min-h-18">
            {#if app.migrating && finalizing && totals}
              <div class="grid gap-2" aria-live="polite" aria-atomic="true">
                <progress class="progress progress-primary w-full" aria-label="Archive totals progress"
                  value={totals.processed} max={Math.max(totals.total, 1)}></progress>
                <p class="text-sm">{totals.processed.toLocaleString()} of {totals.total.toLocaleString()} {totals.phase === "reading_manifests" ? "unique files counted" : "chunks counted"}</p>
                <p class="text-xs muted">Converted files are already verified.</p>
              </div>
            {:else if activity.progress && activity.progress.phase !== "preparing"}
              <div class="grid gap-2" aria-live="polite" aria-atomic="true">
                <progress class="progress progress-primary w-full" aria-label="Migration progress"
                  value={activity.progress.unique_files} max={Math.max(activity.progress.total_unique_files, 1)}></progress>
                <p class="text-sm">{activity.progress.unique_files.toLocaleString()} of {activity.progress.total_unique_files.toLocaleString()} unique files verified</p>
                <p class="text-xs muted">{formatSize(activity.progress.bytes_processed)} of {formatSize(activity.progress.total_bytes)} verified · {activity.progress.resumed_files.toLocaleString()} reused from checkpoints</p>
              </div>
            {:else}<progress class="progress progress-primary w-full" aria-label="Migration progress"></progress>{/if}
          </div>
          {#if app.migrating}
            <!-- Keep every configured worker's space, including while preparing or finalizing. -->
            <ul class="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-2 lg:grid-cols-3" aria-label="Files in progress">
              {#each workerSlots as index (index)}
                {@const file = activity.progress?.active_files[index]}
                <li class="h-14 min-w-0 grid content-start gap-1" aria-hidden={!file}>
                  {#if file}
                    <p class="font-path truncate text-xs leading-4" title={file.path}>{file.path}</p>
                    <div class="flex justify-between gap-2 text-xs leading-4 muted">
                      <span class="min-w-0 truncate">{stageLabel[file.stage]}</span>
                      <span class="shrink-0 tabular-nums">{formatSize(file.bytes_processed)} / {formatSize(file.total_bytes)}</span>
                    </div>
                    <progress class="progress w-full" aria-label={`${stageLabel[file.stage]} ${file.path}`}
                      value={file.bytes_processed} max={Math.max(file.total_bytes, 1)}></progress>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
      {#if activity.error}<p class="alert alert-error text-sm break-words" role="alert">{activity.error}</p>{/if}
      {#if app.workspaceError}<p class="notice notice-error" role="alert">{app.workspaceError}</p>{/if}
    </div>
  {/if}
  {#snippet actions()}
    {#if completed && activity?.result}
      <UiButton onclick={onClose}>Keep original open</UiButton>
      <UiButton variant="primary" onclick={() => onOpenArchive(activity!.result!.id)}>Open migrated archive</UiButton>
    {:else if app.migrating}
      <UiButton onclick={() => app.requestMigrationCancel()} disabled={app.cancellingMigration}>
        {app.cancellingMigration ? "Stopping…" : "Stop migration"}
      </UiButton>
    {:else}
      <UiButton variant="ghost" onclick={onBack} disabled={app.checkingMigration || browsing}>Back to archives</UiButton>
      <UiButton variant="primary" onclick={() => app.runMigration()}
        disabled={app.checkingMigration || browsing || app.migrationFormat !== 1 || !activity?.destination.trim() || !activity?.label.trim() || !Number.isInteger(activity?.workers) || activity!.workers < 1 || activity!.workers > 32}>
        {activity?.status === "cancelled" ? "Resume migration" : activity?.status === "failed" ? "Retry migration" : "Start migration"}
      </UiButton>
    {/if}
  {/snippet}
</UiDialog>
