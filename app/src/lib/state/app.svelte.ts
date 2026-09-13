import {
  scanDirectory,
  cancelScan,
  onScanProgress,
  listWorkspaces,
  listCustomScanRules,
  saveCustomScanRules,
  type ScanStats,
  type ScanProgress,
  type WorkspacesConfig,
  type CustomScanRule,
  type ScanRule,
  type ScanRuleAction,
} from "$lib/api/tauri";
import type { UnlistenFn } from "@tauri-apps/api/event";

export interface ScanActivity {
  workspaceId: string;
  archiveName: string;
  source: string;
  target: string;
  startedAt: number;
  finishedAt: number | null;
  lastProgressAt: number | null;
  status: "running" | "completed" | "cancelled" | "failed";
  progress: ScanProgress | null;
  result: ScanStats | null;
  error: string | null;
}

class AppState {
  theme = $state<"system" | "light" | "dark">("system");
  setTheme = (theme: "system" | "light" | "dark") => {
    this.theme = theme;
    try {
      localStorage.setItem("dedup-theme", theme);
    } catch {
      /* Optional preference. */
    }
  };
  activity = $state<ScanActivity | null>(null);
  workspaceError = $state<string | null>(null);
  workspacesLoading = $state(true);
  // ── Workspaces ──
  workspacesConfig = $state<WorkspacesConfig>({
    workspaces: [],
    active_workspace_id: null,
    custom_scan_rules: [],
  });
  treeRefreshKey = $state(0);

  // ── Custom scan rules ──
  customScanRules = $state<CustomScanRule[]>([]);
  activeCustomRuleIds = $state<string[]>([]);
  customRulesError = $state<string | null>(null);
  savingCustomRules = $state(false);
  newRuleLabel = $state("");
  newRulePattern = $state("");
  newRuleAction = $state<ScanRuleAction>("ignore");

  // ── Scan form ──
  scanSource = $state("");
  targetPath = $state("/");
  bundleGitDirs = $state(false);
  ignoreRustTarget = $state(false);
  ignoreNodeModules = $state(false);
  ignorePythonVenv = $state(false);
  scanning = $state(false);
  cancelling = $state(false);
  scanResult = $state<ScanStats | null>(null);
  scanError = $state<string | null>(null);
  progress = $state<ScanProgress | null>(null);

  // ── Derived ──
  activeWorkspace = $derived(
    this.workspacesConfig.workspaces.find(
      (w) => w.id === this.workspacesConfig.active_workspace_id,
    ) ?? null,
  );
  hasWorkspace = $derived(this.activeWorkspace !== null);

  // ── Loading ──
  loadWorkspaces = async () => {
    this.workspacesLoading = true;
    this.workspaceError = null;
    try {
      this.workspacesConfig = await listWorkspaces();
      this.treeRefreshKey++;
    } catch (e) {
      this.workspaceError = String(e);
    } finally {
      this.workspacesLoading = false;
    }
  };

  loadCustomScanRules = async () => {
    try {
      this.customScanRules = await listCustomScanRules();
      this.activeCustomRuleIds = this.customScanRules
        .filter((rule) => rule.enabled)
        .map((rule) => rule.id);
      this.customRulesError = null;
    } catch (e) {
      this.customRulesError = String(e);
    }
  };

  syncCustomScanRulesFromConfig = (nextRules: CustomScanRule[]) => {
    this.customScanRules = nextRules;
    this.activeCustomRuleIds = this.customScanRules
      .filter((rule) => rule.enabled)
      .map((rule) => rule.id);
  };

  // ── Scan form actions ──

  prepareScan = (presetTarget?: string) => {
    if (this.scanning) return;
    this.targetPath = presetTarget ?? "/";
    this.bundleGitDirs = false;
    this.ignoreRustTarget = false;
    this.ignoreNodeModules = false;
    this.ignorePythonVenv = false;
    this.activeCustomRuleIds = this.customScanRules
      .filter((rule) => rule.enabled)
      .map((rule) => rule.id);
    this.scanError = null;
    this.customRulesError = null;
    this.progress = null;
  };

  handlePresetChange = (id: string, checked: boolean) => {
    if (id === "git") this.bundleGitDirs = checked;
    if (id === "rust") this.ignoreRustTarget = checked;
    if (id === "node") this.ignoreNodeModules = checked;
    if (id === "python") this.ignorePythonVenv = checked;
  };

  buildScanRules = (): ScanRule[] => {
    const rules: ScanRule[] = [];
    if (this.bundleGitDirs) {
      rules.push({ pattern: "(^|/)\\.git$", action: "archive" });
    }
    if (this.ignoreRustTarget) {
      rules.push({ pattern: "(^|/)target$", action: "ignore" });
    }
    if (this.ignoreNodeModules) {
      rules.push({ pattern: "(^|/)node_modules$", action: "ignore" });
    }
    if (this.ignorePythonVenv) {
      rules.push({ pattern: "(^|/)(\\.venv|venv)$", action: "ignore" });
    }
    const activeCustomRules = new Set(this.activeCustomRuleIds);
    for (const rule of this.customScanRules) {
      if (activeCustomRules.has(rule.id)) {
        rules.push({ pattern: rule.pattern, action: rule.action });
      }
    }
    return rules;
  };

  toggleCustomRule = (ruleId: string, checked: boolean) => {
    if (this.savingCustomRules) return;
    if (checked) {
      this.activeCustomRuleIds = Array.from(
        new Set([...this.activeCustomRuleIds, ruleId]),
      );
    } else {
      this.activeCustomRuleIds = this.activeCustomRuleIds.filter(
        (id) => id !== ruleId,
      );
    }
  };

  addCustomRule = async () => {
    if (
      this.savingCustomRules ||
      !this.newRuleLabel.trim() ||
      !this.newRulePattern.trim()
    )
      return;
    this.customRulesError = null;
    const newRuleId = `rule_${Date.now().toString(16)}`;
    const previousRules = this.customScanRules;
    const previousActiveRuleIds = this.activeCustomRuleIds;
    const activeBeforeSave = new Set(this.activeCustomRuleIds);
    const nextRules = [
      ...this.customScanRules,
      {
        id: newRuleId,
        label: this.newRuleLabel.trim(),
        pattern: this.newRulePattern.trim(),
        action: this.newRuleAction,
        enabled: true,
      },
    ];
    this.savingCustomRules = true;
    try {
      this.customScanRules = await saveCustomScanRules(nextRules);
      const savedRuleIds = new Set(this.customScanRules.map((rule) => rule.id));
      this.activeCustomRuleIds = Array.from(
        new Set([...activeBeforeSave, newRuleId]),
      ).filter((id) => savedRuleIds.has(id));
      this.newRuleLabel = "";
      this.newRulePattern = "";
      this.newRuleAction = "ignore";
    } catch (e) {
      this.customScanRules = previousRules;
      this.activeCustomRuleIds = previousActiveRuleIds;
      this.customRulesError = String(e);
    } finally {
      this.savingCustomRules = false;
    }
  };

  removeCustomRule = async (ruleId: string) => {
    if (this.savingCustomRules) return;
    this.customRulesError = null;
    const previousRules = this.customScanRules;
    const previousActiveRuleIds = this.activeCustomRuleIds;
    const nextRules = this.customScanRules.filter((rule) => rule.id !== ruleId);
    this.customScanRules = nextRules;
    this.activeCustomRuleIds = this.activeCustomRuleIds.filter(
      (id) => id !== ruleId,
    );
    this.savingCustomRules = true;
    try {
      this.customScanRules = await saveCustomScanRules(nextRules);
      const savedRuleIds = new Set(this.customScanRules.map((rule) => rule.id));
      this.activeCustomRuleIds = this.activeCustomRuleIds.filter((id) =>
        savedRuleIds.has(id),
      );
    } catch (e) {
      this.customScanRules = previousRules;
      this.activeCustomRuleIds = previousActiveRuleIds;
      this.customRulesError = String(e);
    } finally {
      this.savingCustomRules = false;
    }
  };

  runScan = async (): Promise<void> => {
    if (
      this.scanning ||
      !this.hasWorkspace ||
      !this.scanSource.trim() ||
      this.savingCustomRules
    )
      return;
    this.scanning = true;
    this.cancelling = false;
    this.scanError = null;
    this.scanResult = null;
    this.progress = null;
    const activity: ScanActivity = {
      workspaceId: this.activeWorkspace!.id,
      archiveName: this.activeWorkspace!.label,
      source: this.scanSource.trim(),
      target: this.targetPath.trim() || "/",
      startedAt: Date.now(),
      finishedAt: null,
      lastProgressAt: null,
      status: "running",
      progress: null,
      result: null,
      error: null,
    };
    this.activity = activity;
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await onScanProgress((p) => {
        this.progress = p;
        if (this.activity?.status === "running") {
          this.activity.progress = p;
          this.activity.lastProgressAt = Date.now();
        }
      });
      // Cancellation requested while the event subscription was being installed.
      if (this.cancelling) {
        this.activity!.status = "cancelled";
        return;
      }
      const result = await scanDirectory(
        activity.source,
        activity.target,
        false,
        this.buildScanRules(),
      );
      this.scanResult = result;
      this.activity!.result = result;
      this.activity!.status = "completed";
    } catch (e) {
      const message = String(e);
      if (message.includes("scan cancelled"))
        this.activity!.status = "cancelled";
      else {
        this.scanError = message;
        this.activity!.error = message;
        this.activity!.status = "failed";
      }
    } finally {
      unlisten?.();
      this.activity!.finishedAt = Date.now();
      this.scanning = false;
      this.cancelling = false;
      this.treeRefreshKey++;
      // A config refresh failure must not turn a completed scan into a failure.
      await this.loadWorkspaces();
    }
  };

  /** Request cancellation of an in-progress scan. */
  requestCancel = async () => {
    if (!this.scanning) return;
    this.cancelling = true;
    this.scanError = null;
    try {
      await cancelScan();
    } catch (e) {
      this.scanError = String(e);
      this.cancelling = false;
    }
  };
}

export const app = new AppState();
