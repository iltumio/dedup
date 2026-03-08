import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

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
}

export interface ScanProgress {
	files_processed: number;
	dirs_processed: number;
	bytes_processed: number;
	bytes_stored: number;
	duplicates_found: number;
	current_file: string;
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

export async function findDuplicates(path: string): Promise<string[]> {
	return invoke('find_duplicates', { path });
}

export async function scanDirectory(
	source: string,
	storePath: string,
	targetPath: string
): Promise<ScanStats> {
	return invoke('scan_directory', { source, storePath, targetPath });
}

export async function findAllDuplicates(): Promise<[string, string[]][]> {
	return invoke('find_all_duplicates');
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
