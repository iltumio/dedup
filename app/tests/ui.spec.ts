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
