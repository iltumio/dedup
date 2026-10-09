import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

import { fixture } from "./fixture";

async function startScan(page: Page) {
  await page.getByRole("button", { name: "Add folder", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Folder to add", exact: true })
    .fill("/photos");
  await page.getByRole("button", { name: "Start scan", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__dedupTest.calls.includes("scan_directory"),
      ),
    )
    .toBe(true);
}

test("scan progress and stop remain visible in the minimum window; completion is retained", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 500 });
  await fixture(page);
  await startScan(page);
  await expect(
    page.getByRole("heading", { name: "Scan in progress" }),
  ).toBeInViewport();
  await expect(
    page.getByRole("button", { name: "Stop scan", exact: true }),
  ).toBeInViewport();
  await expect(
    page.getByRole("textbox", { name: "Folder to add" }),
  ).toHaveCount(0);
  await page.evaluate(() => (window as any).__dedupTest.emit());
  await expect(page.getByText("3,400", { exact: true })).toBeVisible();
  await expect(
    page.getByText("Files processed", { exact: true }),
  ).toBeInViewport();
  await page.evaluate(() => (window as any).__dedupTest.finish());
  await expect(
    page.getByRole("heading", { name: "Scan complete", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Browse files", exact: true }).click();
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Scan complete", exact: true }),
  ).toBeVisible();
});

test("cancel and backend errors have distinct, recoverable outcomes", async ({
  page,
}) => {
  await fixture(page);
  await startScan(page);
  await page.evaluate(() => (window as any).__dedupTest.emit());
  await page.getByRole("button", { name: "Stop scan", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Scan stopped", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Files already stored may remain", { exact: false }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Add another folder" }).click();
  await page.getByRole("button", { name: "Start scan", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__dedupTest.calls.filter(
            (c: string) => c === "scan_directory",
          ).length,
      ),
    )
    .toBe(2);
  await page.evaluate(() => (window as any).__dedupTest.fail());
  await expect(
    page.getByRole("heading", { name: "Scan could not finish" }),
  ).toBeVisible();
  await expect(page.getByText("Disk is full", { exact: true })).toBeVisible();
});

test("refresh failure does not erase a successful scan; partial issues expose their log", async ({
  page,
}) => {
  await fixture(page);
  await startScan(page);
  await page.evaluate(() => {
    (window as any).__dedupTest.failRefresh = true;
    (window as any).__dedupTest.finish({
      errors_log_path: "/example/errors.log",
    });
  });
  await expect(
    page.getByRole("heading", { name: "Completed with issues" }),
  ).toBeVisible();
  await page.getByText("Scan details", { exact: false }).first().click();
  await expect(
    page.getByText("/example/errors.log", { exact: true }),
  ).toBeVisible();
});

test("files are bounded, searchable, and remain readable when selected and hovered", async ({
  page,
}) => {
  await fixture(page);
  await expect(page.locator(".file-table tbody tr")).toHaveCount(100);
  await page.getByRole("button", { name: "Next", exact: true }).click();
  await expect(page.getByText("2 / 3", { exact: true })).toBeVisible();
  await page
    .getByRole("textbox", { name: "Search this folder" })
    .fill("photo-249");
  await expect(page.locator(".file-table tbody tr")).toHaveCount(1);
  await page
    .getByRole("button", { name: "photo-249.jpg", exact: true })
    .click();
  await page
    .getByRole("button", { name: "photo-249.jpg", exact: true })
    .hover();
  await expect(page.locator(".file-table tr.selected")).toHaveCount(1);
  const audit = await new AxeBuilder({ page })
    .include(".file-table")
    .withRules(["color-contrast"])
    .analyze();
  expect(audit.violations).toEqual([]);
  await expect(
    page.getByText("Preview unavailable.", { exact: false }),
  ).toBeVisible();
});

test("late detail responses cannot overwrite the selected file", async ({
  page,
}) => {
  await fixture(page);
  await page.evaluate(() => ((window as any).__dedupTest.delayed = true));
  await page
    .getByRole("button", { name: "photo-000.jpg", exact: true })
    .click();
  await page
    .getByRole("button", { name: "photo-001.jpg", exact: true })
    .click();
  await expect(page.locator(".metadata-list")).toContainText("222 B");
  await page.waitForTimeout(500);
  await expect(page.locator(".metadata-list")).toContainText("222 B");
  await expect(page.locator(".metadata-list")).not.toContainText("111 B");
});

test("duplicate paths open details; compact layout offers a return to files", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 500 });
  await fixture(page);
  await page.getByRole("button", { name: "Duplicates", exact: true }).click();
  await page
    .getByRole("button", { name: "/Vacations/copy.jpg", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "copy.jpg", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".files-pane")).toBeHidden();
  await page.getByRole("button", { name: "Back to file list" }).click();
  await expect(page.locator(".files-pane")).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: "Folder path" }),
  ).toContainText("Vacations");
});

test("archive switch refreshes overview; removal is reversible and leaves files intact", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await expect(page.getByText("251", { exact: true }).first()).toBeVisible();
  await page.getByRole("combobox", { name: "Current archive" }).click();
  await page.getByRole("option", { name: "Work archive", exact: true }).click();
  await expect(page.getByText("14", { exact: true }).first()).toBeVisible();
  await expect(page.getByText("251", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Manage archives" }).click();
  const row = page
    .getByRole("dialog")
    .locator("li")
    .filter({ hasText: "Work archive" });
  await row.getByText("Archive options", { exact: true }).click();
  await row.getByRole("button", { name: "Remove from list" }).click();
  await expect(
    page
      .getByRole("dialog")
      .getByText("“Work archive” removed. Its files remain on disk."),
  ).toBeVisible();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Undo", exact: true })
    .click();
  await expect(
    page.getByRole("dialog").getByText("Work archive", { exact: true }),
  ).toBeVisible();
});

test("first run creates and activates an archive, then offers a simple scan form", async ({
  page,
}) => {
  await fixture(page, true);
  await expect(
    page.getByRole("combobox", { name: "Current archive", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Create an archive", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Archive name", exact: true })
    .fill("My photos");
  await page
    .getByRole("textbox", { name: "Archive location", exact: true })
    .fill("/example/photos.store");
  await page
    .getByRole("button", { name: "Create archive", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "A folder. One safe copy." }),
  ).toBeVisible();
  await expect(
    page.getByText("My photos", { exact: true }).first(),
  ).toBeVisible();
  await expect(
    page.getByRole("textbox", { name: "Folder inside the archive" }),
  ).toBeHidden();
  expect(await page.evaluate(() => (window as any).__dedupTest.createCalls[0].format)).toBe("fastcdc");
});

test("primary screens meet automated accessibility checks in both themes", async ({
  page,
}) => {
  await fixture(page);
  for (const theme of ["light", "dark"]) {
    await page
      .getByRole("combobox", { name: "Appearance", exact: true })
      .click();
    await page
      .getByRole("option", {
        name: theme === "light" ? "Light" : "Dark",
        exact: true,
      })
      .click();
    await expect(page.locator(".file-table tbody tr")).toHaveCount(100);
    const result = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
      .analyze();
    expect(result.violations).toEqual([]);
  }
  await page.getByRole("button", { name: "Add folder", exact: true }).click();
  const formResult = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
    .analyze();
  expect(formResult.violations).toEqual([]);
});

test("custom selects support keyboard, cancellation and themed popups in a small window", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 500 });
  await fixture(page);
  const appearance = page.getByRole("combobox", {
    name: "Appearance",
    exact: true,
  });
  await appearance.focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("listbox")).toBeVisible();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(appearance).toHaveText("Dark");
  await expect(appearance).toBeFocused();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "night");
  await appearance.click();
  await expect(
    page.getByRole("option", { name: "Dark", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("option", { name: "System", exact: true }),
  ).toBeInViewport();
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
    .analyze();
  expect(result.violations).toEqual([]);
  await page.keyboard.press("Home");
  await page.keyboard.press("Escape");
  await expect(appearance).toHaveText("Dark");
  await expect(appearance).toBeFocused();
  await appearance.click();
  await page.getByRole("heading", { name: "Files", exact: true }).click();
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await page.getByRole("combobox", { name: "Sort files", exact: true }).click();
  await page
    .getByRole("option", { name: "Recently modified", exact: true })
    .click();
  await expect(
    page.getByRole("combobox", { name: "Sort files", exact: true }),
  ).toHaveText("Recently modified");
});

test("custom scan action select remains associated with its field label", async ({
  page,
}) => {
  await fixture(page);
  await page.getByRole("button", { name: "Add folder", exact: true }).click();
  await page.getByText("Advanced options", { exact: true }).click();
  await page.getByText("Create a reusable rule", { exact: true }).click();
  const action = page.getByRole("combobox", { name: "Action", exact: true });
  await page.getByText("Action", { exact: true }).click();
  await expect(action).toBeFocused();
  await action.click();
  await page.getByRole("option", { name: "Archive", exact: true }).click();
  await expect(action).toHaveText("Archive");
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
    .analyze();
  expect(result.violations).toEqual([]);
});

test("Open file is hidden only for a confirmed missing association", async ({
  page,
}) => {
  await fixture(page);
  await page.evaluate(() => {
    (window as any).__dedupTest.unsupportedOpen = true;
  });
  await page
    .getByRole("button", { name: "photo-001.jpg", exact: true })
    .click();
  await expect(
    page.getByText("No default application is associated with this file type."),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Open file", exact: true }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__dedupTest.unsupportedOpen = false;
  });
  await page
    .getByRole("button", { name: "photo-002.jpg", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Open file", exact: true }),
  ).toBeEnabled();
  await page.evaluate(() => {
    (window as any).__dedupTest.failOpenCheck = true;
    (window as any).__dedupTest.failOpen = true;
  });
  await page
    .getByRole("button", { name: "photo-003.jpg", exact: true })
    .click();
  await page.getByRole("button", { name: "Open file", exact: true }).click();
  await expect(
    page.getByText(/Could not open file:.*No application available/),
  ).toBeVisible();
});

test("a late association check cannot hide Open for the next file", async ({
  page,
}) => {
  await fixture(page);
  await page.evaluate(() => {
    (window as any).__dedupTest.delayed = true;
    (window as any).__dedupTest.unsupportedOpen = true;
  });
  await page
    .getByRole("button", { name: "photo-000.jpg", exact: true })
    .click();
  await expect(
    page.getByText("Checking available applications…"),
  ).toBeVisible();
  await page.evaluate(() => {
    (window as any).__dedupTest.unsupportedOpen = false;
  });
  await page
    .getByRole("button", { name: "photo-001.jpg", exact: true })
    .click();
  const open = page.getByRole("button", { name: "Open file", exact: true });
  await expect(open).toBeEnabled();
  await page.waitForTimeout(450);
  await expect(open).toBeEnabled();
});

test("the header CTA adds into the open folder and the form shows that destination", async ({
  page,
}) => {
  await fixture(page);
  await expect(page.getByRole("button", { name: "Add here" })).toHaveCount(0);
  await page.getByRole("button", { name: "Vacations", exact: true }).click();
  await page
    .getByRole("button", { name: "Add to Vacations", exact: true })
    .click();
  await expect(page.getByText("› /Vacations")).toBeVisible();
  await page.getByRole("button", { name: "Change", exact: true }).click();
  await expect(
    page.getByRole("textbox", { name: "Folder inside the archive" }),
  ).toBeFocused();
  await page.getByRole("button", { name: "Back to archive" }).click();
  await page.getByRole("button", { name: "Duplicates", exact: true }).click();
  await page.getByRole("button", { name: "Add folder", exact: true }).click();
  await expect(page.getByText("› /Vacations")).toHaveCount(0);
});

async function openMigration(page: Page) {
  await page.getByRole("button", { name: "Manage archives", exact: true }).click();
  const archive = page.locator("li").filter({ has: page.getByText("Photo archive", { exact: true }) });
  await archive.getByText("Archive options", { exact: true }).click();
  await archive.getByRole("button", { name: /^(Migrate to FastCDC|Resume migration)$/ }).click();
  await expect(page.getByRole("dialog", { name: /Migrate to FastCDC|Migration stopped/ })).toBeVisible();
}

async function startMigration(page: Page) {
  await openMigration(page);
  await page.getByRole("textbox", { name: "Destination folder", exact: true }).fill("/backups/photos-fastcdc");
  await page.getByRole("button", { name: "Start migration", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__dedupTest.migrationCalls.length)).toBe(1);
}

test("migration shows progress, ignores stale events, imports the new archive and keeps the original", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 500 });
  await fixture(page);
  await startMigration(page);
  const dialog = page.getByRole("dialog", { name: "Migration in progress", exact: true });
  await expect(dialog.getByRole("button", { name: "Stop migration", exact: true })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: "Close dialog" })).toBeDisabled();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ unique_files: 9 }, "previous-job"));
  await expect(dialog.getByText("9 of 10 unique files verified")).toHaveCount(0);
  await page.evaluate(() => (window as any).__dedupTest.emitMigration());
  await expect(dialog.getByText("3 of 10 unique files verified", { exact: true })).toBeVisible();
  const accessibility = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
  expect(accessibility.violations).toEqual([]);
  await page.evaluate(() => (window as any).__dedupTest.finishMigration());
  await expect(page.getByRole("heading", { name: "Migration complete", exact: true })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Current archive" })).toHaveText("Photo archive");
  await page.evaluate(() => { (window as any).__dedupTest.failRefresh = true; });
  await page.getByRole("button", { name: "Open migrated archive", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Migration complete", exact: true }).getByRole("alert")).toContainText("Refresh unavailable");
  await page.getByRole("button", { name: "Open migrated archive", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Current archive" })).toHaveText("Photo archive (FastCDC)");
  const workspaces = await page.evaluate(() => (window as any).__dedupTest.migrationCalls[0]);
  expect(workspaces.workspaceId).toBe("one");
  expect(workspaces.destination).toBe("/backups/photos-fastcdc");
});

test("migration cancellation can be resumed after reopening the app", async ({ page }) => {
  await fixture(page);
  await startMigration(page);
  await page.evaluate(() => (window as any).__dedupTest.emitMigration());
  await page.getByRole("button", { name: "Stop migration", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Migration stopped", exact: true })).toBeVisible();
  await page.reload();
  await page.getByRole("button", { name: "photo-001.jpg", exact: true }).waitFor();
  await openMigration(page);
  await expect(page.getByRole("textbox", { name: "Destination folder", exact: true })).toHaveValue("/backups/photos-fastcdc");
  await page.getByRole("button", { name: "Resume migration", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__dedupTest.migrationCalls.length)).toBe(1);
  await page.evaluate(() => (window as any).__dedupTest.finishMigration());
  await expect(page.getByRole("heading", { name: "Migration complete", exact: true })).toBeVisible();
});

test("migration errors are retryable and refresh failure preserves successful completion", async ({ page }) => {
  await fixture(page);
  await startMigration(page);
  await page.evaluate(() => (window as any).__dedupTest.failMigration());
  await expect(page.getByRole("heading", { name: "Migration could not finish", exact: true })).toBeVisible();
  await expect(page.getByRole("alert").filter({ hasText: "Disk is full" })).toBeVisible();
  await page.getByRole("button", { name: "Retry migration", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__dedupTest.migrationCalls.length)).toBe(2);
  await page.evaluate(() => { (window as any).__dedupTest.failRefresh = true; (window as any).__dedupTest.finishMigration(); });
  await expect(page.getByRole("heading", { name: "Migration complete", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open migrated archive", exact: true })).toBeEnabled();
});

test("already-FastCDC archives cannot be migrated again, and destination picker works", async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { (window as any).__dedupTest.storageFormat = 2; });
  await openMigration(page);
  await expect(page.getByText("This archive already uses FastCDC.", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start migration", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Browse", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Destination folder", exact: true })).toHaveValue("/chosen/fastcdc-archive");
});

test("migration exposes byte progress before a file finishes and validates parallelism", async ({ page }) => {
  await fixture(page);
  await openMigration(page);
  await page.getByRole("textbox", { name: "Destination folder", exact: true }).fill("/backups/photos-fastcdc");
  const workers = page.getByRole("spinbutton", { name: "Parallel files", exact: true });
  const start = page.getByRole("button", { name: "Start migration", exact: true });
  await workers.fill("0");
  await expect(start).toBeDisabled();
  await workers.fill("2");
  await start.click();
  await expect.poll(() => page.evaluate(() => (window as any).__dedupTest.migrationCalls.length)).toBe(1);
  expect(await page.evaluate(() => (window as any).__dedupTest.migrationCalls[0].workers)).toBe(2);
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ unique_files: 0, resumed_files: 0 }));
  const dialog = page.getByRole("dialog", { name: "Migration in progress", exact: true });
  const file = dialog.getByRole("progressbar", { name: "Converting /backup/large.mov", exact: true });
  await expect(dialog.getByText("0 of 10 unique files verified", { exact: true })).toBeVisible();
  await expect(file).toHaveAttribute("value", "1048576");
  await expect(dialog.getByText("1 of 2 parallel files active", { exact: true })).toBeVisible();
  await expect(dialog.getByText("Read & verification rate", { exact: true })).toBeVisible();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({
    unique_files: 0, resumed_files: 0, work_bytes: 4194304,
    active_files: [{ path: "/backup/large.mov", stage: "converting", bytes_processed: 2097152, total_bytes: 8388608 }],
  }));
  await expect(file).toHaveAttribute("value", "2097152");
  await expect(dialog.getByText("0 of 10 unique files verified", { exact: true })).toBeVisible();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ phase: "finalizing", active_files: [] }));
  await expect(dialog.getByText("Calculating totals and adding the archive…", { exact: true })).toBeVisible();
  await page.evaluate(() => (window as any).__dedupTest.finishMigration());
  const completed = page.getByRole("dialog", { name: "Migration complete", exact: true });
  await expect(completed).toBeVisible();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ unique_files: 0 }));
  await expect(completed.getByText("0 of 10 unique files verified", { exact: true })).toHaveCount(0);
});

test("migration distinguishes responding updates from stalled data and missing updates", async ({ page }) => {
  await fixture(page);
  await startMigration(page);
  await page.clock.install();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ idle_seconds: 40, rate_bytes_per_second: 0 }));
  const dialog = page.getByRole("dialog", { name: "Migration in progress", exact: true });
  await expect(dialog.getByText("Updates arriving · last update just now", { exact: true })).toBeVisible();
  await expect(dialog.getByText(/No data progress for 40s/)).toBeVisible();
  await page.clock.runFor(6000);
  await expect(dialog.getByText(/Waiting for an update · last update 6s ago/)).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Stop migration", exact: true })).toBeInViewport();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ idle_seconds: 0, rate_bytes_per_second: 2097152 }));
  await expect(dialog.getByText("Updates arriving · last update just now", { exact: true })).toBeVisible();
  await expect(dialog.getByText(/No data progress/)).toHaveCount(0);
  await dialog.getByRole("button", { name: "Stop migration", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Migration stopped", exact: true })).toBeVisible();
});

test("migration reports real archive totals progress without resizing the dialog", async ({ page }) => {
  await fixture(page);
  await startMigration(page);
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({ phase: "finalizing", active_files: [] }));
  const dialog = page.getByRole("dialog", { name: "Migration in progress", exact: true });
  const box = dialog.locator(".modal-box");
  const initial = await box.boundingBox();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({
    phase: "finalizing", active_files: [],
    finalization: { phase: "reading_manifests", processed: 3, total: 4 },
  }));
  await expect(dialog.getByText("Reading archive index…", { exact: true })).toBeVisible();
  await expect(dialog.getByText("3 of 4 unique files counted", { exact: true })).toBeVisible();
  await page.evaluate(() => (window as any).__dedupTest.emitMigration({
    phase: "finalizing", active_files: [],
    finalization: { phase: "counting_blobs", processed: 25, total: 100 },
  }));
  await expect(dialog.getByText("Calculating stored size…", { exact: true })).toBeVisible();
  await expect(dialog.getByText("25 of 100 chunks counted", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("progressbar", { name: "Archive totals progress" })).toHaveAttribute("value", "25");
  await expect(dialog.getByRole("progressbar", { name: "Archive totals progress" })).toHaveAttribute("max", "100");
  await expect(dialog.getByRole("button", { name: "Stop migration", exact: true })).toBeInViewport();
  const current = await box.boundingBox();
  expect(current?.height).toBe(initial?.height);
  expect(current?.width).toBe(initial?.width);
  const accessibility = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
  expect(accessibility.violations).toEqual([]);
});

test("archive creation offers the original format and resets the choice for the next archive", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 500 });
  await fixture(page, true);
  await page.getByRole("button", { name: "Create an archive", exact: true }).click();
  const format = page.getByRole("combobox", { name: "Archive format", exact: true });
  await expect(format).toHaveText("FastCDC");
  await expect(page.getByText("Shares identical files and unchanged parts of similar files.", { exact: true })).toBeVisible();
  await page.getByText("Archive format", { exact: true }).click();
  await expect(format).toBeFocused();
  await format.click();
  await page.getByRole("option", { name: "Original (whole files)", exact: true }).click();
  await expect(format).toHaveText("Original (whole files)");
  await expect(page.getByText("Shares identical whole files. You can migrate to FastCDC later.", { exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: "Archive name", exact: true }).fill("Original photos");
  await page.getByRole("textbox", { name: "Archive location", exact: true }).fill("/example/original.store");
  const accessibility = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
  expect(accessibility.violations).toEqual([]);
  await page.getByRole("button", { name: "Create archive", exact: true }).click();
  await expect(page.getByRole("heading", { name: "A folder. One safe copy." })).toBeVisible();
  expect(await page.evaluate(() => (window as any).__dedupTest.createCalls[0])).toMatchObject({
    label: "Original photos", storePath: "/example/original.store", format: "legacy",
  });
  await page.getByRole("button", { name: "Back to archive", exact: true }).click();
  await page.getByRole("button", { name: "Manage archives", exact: true }).click();
  await page.getByRole("button", { name: "Create archive", exact: true }).click();
  await expect(format).toHaveText("FastCDC");
  await page.getByRole("button", { name: "Back", exact: true }).click();
  await page.getByRole("button", { name: "Open existing", exact: true }).click();
  await expect(format).toHaveCount(0);
});
