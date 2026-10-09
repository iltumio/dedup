import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

export async function pickDirectory(title?: string): Promise<string | null> {
	const selected = await open({ directory: true, multiple: false, title });
	return typeof selected === 'string' ? selected : null;
}

export async function pickFile(
	title?: string,
	filters?: { name: string; extensions: string[] }[]
): Promise<string | null> {
	const selected = await open({ directory: false, multiple: false, title, filters });
	return typeof selected === 'string' ? selected : null;
}

export interface DirEntry {
	name: string;
	is_dir: boolean;
	size: number;
	modified: number;
}

export interface FileMetadata {
	cid: number[];
	original_size: number;
	compressed_size: number;
	modified: number;
	created: number;
	permissions: number;
}

export interface ScanStats {
	total_files: number;
	total_dirs: number;
	unique_blobs: number;
	duplicate_files: number;
	total_original_bytes: number;
	total_stored_bytes: number;
	skipped_files: number;
	errors_log_path: string | null;
}

export interface ScanProgress {
	files_processed: number;
	dirs_processed: number;
	bytes_processed: number;
	bytes_stored: number;
	duplicates_found: number;
	skipped_files: number;
	current_file: string;
}

export type ScanRuleAction = 'ignore' | 'archive';

export interface ScanRule {
	pattern: string;
	action: ScanRuleAction;
}

export async function listDir(path: string): Promise<DirEntry[]> {
	return invoke('list_dir', { path });
}

export async function getFileMetadata(path: string): Promise<FileMetadata | null> {
	return invoke('get_file_metadata', { path });
}

export async function readFile(path: string): Promise<number[]> {
	return invoke('read_file', { path });
}

export async function canOpenFile(path: string): Promise<boolean | null> {
    return invoke('can_open_file', { path });
}

export async function openFile(path: string): Promise<void> {
	return invoke('open_file', { path });
}

export async function findDuplicates(path: string): Promise<string[]> {
	return invoke('find_duplicates', { path });
}

export async function scanDirectory(
	source: string,
	targetPath: string,
	bundleGitDirs = false,
	rules: ScanRule[] = []
): Promise<ScanStats> {
	return invoke('scan_directory', { source, targetPath, bundleGitDirs, rules });
}

export async function cancelScan(): Promise<void> {
	return invoke('cancel_scan');
}

export async function findAllDuplicates(): Promise<[string, string[]][]> {
	return invoke('find_all_duplicates');
}

export interface ExtensionStats {
	extension: string;
	total_files: number;
	duplicate_files: number;
	duplicate_pct: number;
	total_original_bytes: number;
	total_stored_bytes: number;
	bytes_saved: number;
}

export async function getExtensionStats(): Promise<ExtensionStats[]> {
	return invoke('get_extension_stats');
}

export function onScanProgress(callback: (progress: ScanProgress) => void): Promise<UnlistenFn> {
	return listen<ScanProgress>('scan-progress', (event) => {
		callback(event.payload);
	});
}

export function formatSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1_048_576) return `${(bytes / 1024).toFixed(1)} KB`;
	if (bytes < 1_073_741_824) return `${(bytes / 1_048_576).toFixed(1)} MB`;
	return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
}

export function formatTimestamp(ts: number): string {
	if (ts === 0) return '—';
	return new Date(ts * 1000).toLocaleString();
}

// ── Workspace types ──────────────────────────────────────

export interface WorkspaceStats {
	total_files: number;
	total_dirs: number;
	unique_blobs: number;
	duplicate_files: number;
	total_original_bytes: number;
	total_stored_bytes: number;
	scans_count: number;
	last_scan_at: number;
}

export interface Workspace {
	id: string;
	label: string;
	tags: string[];
	store_path: string;
	created_at: number;
	stats: WorkspaceStats;
}

export interface CustomScanRule {
	id: string;
	label: string;
	pattern: string;
	action: ScanRuleAction;
	enabled: boolean;
}

export interface WorkspacesConfig {
	pending_migrations?: PendingMigration[];
	workspaces: Workspace[];
	active_workspace_id: string | null;
	custom_scan_rules: CustomScanRule[];
}

export async function listWorkspaces(): Promise<WorkspacesConfig> {
	return invoke('list_workspaces');
}

export async function listCustomScanRules(): Promise<CustomScanRule[]> {
	return invoke('list_custom_scan_rules');
}

export async function saveCustomScanRules(rules: CustomScanRule[]): Promise<CustomScanRule[]> {
	return invoke('save_custom_scan_rules', { rules });
}

export type StorageFormat = 'legacy' | 'fastcdc';

export async function createWorkspace(
	label: string,
	tags: string[],
	storePath: string,
	format: StorageFormat = 'fastcdc'
): Promise<Workspace> {
	return invoke('create_workspace', { label, tags, storePath, format });
}

export async function switchWorkspace(workspaceId: string): Promise<Workspace> {
	return invoke('switch_workspace', { workspaceId });
}

export async function deleteWorkspace(workspaceId: string): Promise<void> {
	return invoke('delete_workspace', { workspaceId });
}

export async function exportWorkspaces(): Promise<string> {
	return invoke('export_workspaces');
}

export async function importWorkspaces(json: string): Promise<WorkspacesConfig> {
	return invoke('import_workspaces', { json });
}

export async function importWorkspace(storePath: string, label: string): Promise<Workspace> {
	return invoke('import_workspace', { storePath, label });
}

export interface PendingMigration {
    workspace_id: string;
    destination: string;
    label: string;
}

export interface MigrationProgress {
    unique_files: number;
    resumed_files: number;
    total_unique_files: number;
    bytes_processed: number;
    total_bytes: number;
    work_bytes: number;
    elapsed_seconds: number;
    rate_bytes_per_second: number;
    idle_seconds: number;
    workers: number;
    phase: 'preparing' | 'migrating' | 'finalizing' | 'completed';
    active_files: { path: string; stage: 'converting' | 'verifying' | 'saving_checkpoint'; bytes_processed: number; total_bytes: number }[];
    finalization?: { phase: 'reading_manifests' | 'counting_blobs'; processed: number; total: number } | null;
}

export function workspaceStorageFormat(workspaceId: string): Promise<number> {
    return invoke('workspace_storage_format', { workspaceId });
}

export function migrateWorkspace(workspaceId: string, destination: string, label: string, jobId: string, workers: number): Promise<Workspace> {
    return invoke('migrate_workspace', { workspaceId, destination, label, jobId, workers });
}

export function cancelMigration(jobId: string): Promise<void> {
    return invoke('cancel_migration', { jobId });
}

export function onMigrationProgress(jobId: string, callback: (progress: MigrationProgress) => void): Promise<UnlistenFn> {
    return listen<{job_id: string; progress: MigrationProgress}>('migration-progress', (event) => {
        if (event.payload.job_id === jobId) callback(event.payload.progress);
    });
}
