<script lang="ts">
  import { app } from "$lib/state/app.svelte";
  import { formatSize } from "$lib/api/tauri";
  import Icon from "../ui/Icon.svelte";
  let {
    onFiles,
    onDuplicates,
    onNew,
  }: { onFiles: () => void; onDuplicates: () => void; onNew: () => void } =
    $props();
  let now = $state(Date.now());
  let title: HTMLHeadingElement;
  let copyMessage = $state("");
  const activity = $derived(app.activity);
  const running = $derived(activity?.status === "running");
  const heading = $derived(
    !activity
      ? "Your activity starts here"
      : app.cancelling
        ? "Stopping scan…"
        : running
          ? "Scan in progress"
          : activity.status === "cancelled"
            ? "Scan stopped"
            : activity.status === "failed"
              ? "Scan could not finish"
              : activity.result?.errors_log_path
                ? "Completed with issues"
                : "Scan complete",
  );
  const files = $derived(
    activity?.result?.total_files ?? activity?.progress?.files_processed ?? 0,
  );
  const bytes = $derived(
    activity?.result?.total_original_bytes ??
      activity?.progress?.bytes_processed ??
      0,
  );
  const copies = $derived(
    activity?.result?.duplicate_files ??
      activity?.progress?.duplicates_found ??
      0,
  );
  const skipped = $derived(
    activity?.result?.skipped_files ?? activity?.progress?.skipped_files ?? 0,
  );
  const seconds = $derived(
    activity
      ? Math.max(
          0,
          Math.floor(
            ((activity.finishedAt ?? now) - activity.startedAt) / 1000,
          ),
        )
      : 0,
  );
  const elapsed = $derived(
    `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`,
  );
  const staleSeconds = $derived(
    activity
      ? Math.max(
          0,
          Math.floor(
            (now - (activity.lastProgressAt ?? activity.startedAt)) / 1000,
          ),
        )
      : 0,
  );
  $effect(() => {
    if (!running) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  $effect(() => {
    heading;
    title?.focus();
  });
  async function copyLog() {
    try {
      await navigator.clipboard.writeText(
        activity?.result?.errors_log_path ?? "",
      );
      copyMessage = "Log path copied";
    } catch {
      copyMessage =
        "Could not copy. Select the path below to copy it manually.";
    }
  }
</script>

<div class="activity-view">
  <div class="activity-body">
    <div class:working={running} class="activity-symbol">
      <Icon
        name={running
          ? "activity"
          : activity?.status === "completed"
            ? "check"
            : "archive"}
        size={28}
      />
    </div>
    <p class="eyebrow">{activity?.archiveName ?? "THIS SESSION"}</p>
    <h2 class="page-title" bind:this={title} tabindex="-1">{heading}</h2>
    <p class="muted" role="status">
      {!activity
        ? "Add a folder to see scan progress and its result here."
        : running
          ? app.cancelling
            ? "Waiting for the current operation to stop safely."
            : "Your original files stay in place."
          : activity.status === "completed"
            ? "Your archive is ready to explore. Original files are unchanged."
            : "Files already stored may remain in the archive. You can scan the folder again."}
    </p>
    {#if activity}
      <div class="scan-metrics">
        <div>
          <span>Files processed</span><strong>{files.toLocaleString()}</strong>
        </div>
        <div>
          <span>Data processed</span><strong>{formatSize(bytes)}</strong>
        </div>
        <div>
          <span>Duplicate files</span><strong>{copies.toLocaleString()}</strong>
        </div>
        <div><span>Elapsed</span><strong>{elapsed}</strong></div>
      </div>
      {#if running}<progress
          class="progress progress-primary"
          aria-label="Scan running; total size is not yet known"
        ></progress>{/if}
      <div class="activity-location">
        <span class="eyebrow">{running ? "LATEST FILE" : "SOURCE FOLDER"}</span>
        <p
          class="font-path"
          title={running
            ? activity.progress?.current_file || activity.source
            : activity.source}
        >
          {running
            ? activity.progress?.current_file ||
              "Waiting for the first progress update…"
            : activity.source}
        </p>
      </div>
      {#if running && staleSeconds >= 15}<p class="muted text-sm">
          Last progress received {staleSeconds}s ago. Large files can take
          longer to process.
        </p>{/if}
      {#if activity.error}<div class="notice notice-error" role="alert">
          <strong>Unable to complete this scan</strong>
          <p>{activity.error}</p>
        </div>{/if}
      {#if running && app.scanError}<div
          class="notice notice-error"
          role="alert"
        >
          {app.scanError}
        </div>{/if}
      <details class="disclosure">
        <summary
          >Scan details <span>{skipped.toLocaleString()} skipped</span></summary
        >
        <div class="detail-content">
          <p>
            Target in archive: <span class="font-path">{activity.target}</span>
          </p>
          <p>
            Skipped files: {skipped.toLocaleString()} (including unchanged or excluded
            files).
          </p>
          {#if activity.result}<p>
              Data stored in this scan: {formatSize(
                activity.result.total_stored_bytes,
              )}.
            </p>{/if}
          <p>Results describe this scan, not the archive's total inventory.</p>
          {#if activity.result?.errors_log_path}<div class="notice">
              <strong>Some files could not be processed.</strong>
              <p class="font-path break-all">
                {activity.result.errors_log_path}
              </p>
              <button class="btn btn-sm" type="button" onclick={copyLog}
                >Copy log path</button
              ><span role="status">{copyMessage}</span>
            </div>{/if}
        </div>
      </details>
      <p class="session-note">
        Latest scan in this app session. Activity is cleared when the app
        closes.
      </p>
    {/if}
  </div>
  <footer class="activity-actions">
    {#if running}<span class="muted text-sm"
        >Archive browsing resumes after the scan.</span
      ><button
        class="btn btn-outline"
        type="button"
        disabled={app.cancelling}
        onclick={app.requestCancel}
        >{app.cancelling ? "Stopping…" : "Stop scan"}</button
      >
    {:else}<button class="btn btn-ghost" type="button" onclick={onNew}
        >Add another folder</button
      >
      <div class="flex gap-2">
        {#if activity?.workspaceId === app.activeWorkspace?.id}<button
            class="btn"
            type="button"
            onclick={onDuplicates}>View duplicates</button
          >{/if}<button class="btn btn-primary" type="button" onclick={onFiles}
          >Browse files</button
        >
      </div>{/if}
  </footer>
</div>
