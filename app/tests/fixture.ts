import type { Page } from "@playwright/test";

export async function fixture(page: Page, empty = false) {
  await page.addInitScript(
    ({ empty }) => {
      const w = window as any;
      const stats = {
        total_files: 1000,
        total_dirs: 2,
        unique_blobs: 990,
        duplicate_files: 10,
        total_original_bytes: 120000,
        total_stored_bytes: 100000,
        scans_count: 1,
        last_scan_at: 1700000000,
      };
      const workspace = (id: string) => ({
        id,
        label: id === "one" ? "Photo archive" : "Work archive",
        tags: [],
        store_path: `/example/${id}.store`,
        created_at: 1700000000,
        stats: { ...stats },
      });
      const config = {
        workspaces: empty ? [] : [workspace("one"), workspace("two")],
        active_workspace_id: empty ? null : "one",
        custom_scan_rules: [],
      };
      let scanResolve: (value: unknown) => void;
      let scanReject: (value: unknown) => void;
      const callbacks = new Map();
      let nextId = 1;
      let listener = 0;
      const result = { ...stats, skipped_files: 2, errors_log_path: null };
      const api = (w.__dedupTest = {
        calls: [] as string[],
        delayed: false,
        failRefresh: false,
        failOpen: false,
        unsupportedOpen: false,
        failOpenCheck: false,
        emit: (overrides = {}) =>
          callbacks.get(listener)?.({
            event: "scan-progress",
            id: 1,
            payload: {
              files_processed: 3400,
              dirs_processed: 90,
              bytes_processed: 4400000,
              bytes_stored: 2100000,
              duplicates_found: 920,
              skipped_files: 23,
              current_file: "/photos/latest.jpg",
              ...overrides,
            },
          }),
        finish: (overrides = {}) => scanResolve({ ...result, ...overrides }),
        fail: () => scanReject("Disk is full"),
      });
      w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
      w.__TAURI_INTERNALS__ = {
        transformCallback: (fn: unknown) => {
          const id = nextId++;
          callbacks.set(id, fn);
          return id;
        },
        invoke: async (cmd: string, args: any = {}) => {
          api.calls.push(cmd);
          if (cmd === "list_workspaces") {
            if (api.failRefresh) {
              api.failRefresh = false;
              throw Error("Refresh unavailable");
            }
            return structuredClone(config);
          }
          if (cmd === "list_custom_scan_rules") return [];
          if (cmd === "save_custom_scan_rules") return args.rules;
          if (cmd === "plugin:event|listen") {
            listener = args.handler;
            return 1;
          }
          if (cmd === "plugin:event|unlisten") return;
          if (cmd === "scan_directory")
            return new Promise((resolve, reject) => {
              scanResolve = resolve;
              scanReject = reject;
            });
          if (cmd === "cancel_scan") {
            scanReject?.("scan cancelled");
            return;
          }
          if (cmd === "switch_workspace") {
            config.active_workspace_id = args.workspaceId;
            return workspace(args.workspaceId);
          }
          if (cmd === "create_workspace") {
            const ws = {
              ...workspace(`created-${config.workspaces.length}`),
              label: args.label,
              store_path: args.storePath,
            };
            config.workspaces.push(ws);
            return ws;
          }
          if (cmd === "delete_workspace") {
            config.workspaces = config.workspaces.filter(
              (ws) => ws.id !== args.workspaceId,
            );
            if (config.active_workspace_id === args.workspaceId)
              config.active_workspace_id = config.workspaces[0]?.id ?? null;
            return;
          }
          if (cmd === "import_workspaces") {
            for (const ws of JSON.parse(args.json).workspaces)
              if (!config.workspaces.some((existing) => existing.id === ws.id))
                config.workspaces.push(ws);
            return structuredClone(config);
          }
          if (cmd === "list_dir")
            return args.path === "/"
              ? [
                  {
                    name: "Vacations",
                    is_dir: true,
                    size: 0,
                    modified: 1700000000,
                  },
                  ...Array.from({ length: 250 }, (_, i) => ({
                    name: `photo-${String(i).padStart(3, "0")}.jpg`,
                    is_dir: false,
                    size: i + 100,
                    modified: 1700000000,
                  })),
                ]
              : [
                  {
                    name: "copy.jpg",
                    is_dir: false,
                    size: 100,
                    modified: 1700000000,
                  },
                ];
          if (cmd === "get_file_metadata") {
            if (api.delayed && args.path.includes("000"))
              await new Promise((r) => setTimeout(r, 400));
            return {
              cid: [1, 2, 3],
              original_size: args.path.includes("000") ? 111 : 222,
              compressed_size: 100,
              modified: 1700000000,
              created: 1700000000,
              permissions: 420,
            };
          }
          if (cmd === "find_duplicates")
            return ["/photo-001.jpg", "/Vacations/copy.jpg"];
          if (cmd === "find_all_duplicates")
            return [["cid", ["/photo-001.jpg", "/Vacations/copy.jpg"]]];
          if (cmd === "read_file") throw Error("Missing preview");
          if (cmd === "can_open_file") {
            const supported = !api.unsupportedOpen;
            if (api.failOpenCheck) throw Error("Association check unavailable");
            if (api.delayed && args.path.includes("000"))
              await new Promise((resolve) => setTimeout(resolve, 350));
            return supported;
          }
          if (cmd === "open_file") {
            if (api.failOpen) throw Error("No application available");
            return;
          }
          if (cmd === "get_extension_stats")
            return [
              {
                extension: "jpg",
                total_files: config.active_workspace_id === "one" ? 251 : 14,
                duplicate_files: 1,
                duplicate_pct: 1,
                total_original_bytes: 10000,
                total_stored_bytes: 5000,
                bytes_saved: 5000,
              },
            ];
          throw Error("Unhandled test command: " + cmd);
        },
      };
    },
    { empty },
  );
  await page.goto("/");
  // Wait for the first IPC-backed render; SSR controls exist before hydration.
  if (!empty)
    await page
      .getByRole("button", { name: "photo-001.jpg", exact: true })
      .waitFor();
  else
    await page
      .getByRole("button", { name: "Create an archive", exact: true })
      .waitFor();
}
