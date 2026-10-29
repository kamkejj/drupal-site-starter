import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { callsTo, fakeBackend, type Options } from "./fake-backend";

async function open(page: Page, options: Options = {}) {
  await fakeBackend(page, options);
  await page.goto("/");
  await page.getByRole("button", { name: "Continue" }).click();
}

test("preflight meets automated WCAG AA checks", async ({ page }) => {
  await fakeBackend(page);
  await page.goto("/");

  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
    .analyze();

  expect(results.violations).toEqual([]);
});

test("wizard choices expose their current state and remain within a narrow viewport", async ({ page }) => {
  await open(page);
  await page.setViewportSize({ width: 320, height: 720 });
  await page.getByRole("button", { name: "2. Drupal and PHP" }).click();

  await expect(page.getByRole("button", { name: /Drupal 11/ })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "2. Drupal and PHP" })).toHaveAttribute("aria-current", "step");
  expect(await page.locator("html").evaluate((html) => html.scrollWidth <= html.clientWidth)).toBeTruthy();

  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
    .analyze();

  expect(results.violations).toEqual([]);
});

const themes = { latte: "light", frappe: "dark", macchiato: "dark", mocha: "dark" } as const;

for (const [theme, mode] of Object.entries(themes)) test(`every screen meets automated WCAG AA checks in the ${theme} theme`, async ({ page }) => {
  await page.addInitScript((p) => localStorage.setItem("preferences", p), JSON.stringify({ appearance: mode, themes: { [mode]: theme } }));
  const violations: string[] = [];
  const scan = async (screen: string) => {
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
      .analyze();
    violations.push(...results.violations.map((v) => `${screen}: ${v.id}`));
  };

  await open(page);
  await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
  await page.getByPlaceholder("My new site").fill("Acme Intranet");
  const steps = page.getByRole("button", { name: /^\d\. / });
  for (let i = 0; i < (await steps.count()); i++) {
    await steps.nth(i).click();
    await scan(`wizard step ${i + 1}`);
  }
  await page.getByRole("button", { name: "Create site" }).click();
  await expect(page.getByText("acme-intranet is ready.")).toBeVisible();
  await scan("run");

  const nav = page.getByRole("navigation");
  for (const screen of ["Sites", "Clone from Git", "Project defaults"]) {
    await nav.getByRole("button", { name: screen }).click();
    await scan(screen);
  }

  await page.getByRole("button", { name: "App settings" }).click();
  await expect(page.getByRole("dialog", { name: "App settings" })).toBeVisible();
  await scan("App settings");

  expect(violations).toEqual([]);
});

test("App settings opens from the keyboard and changes theme, mode and text size", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await open(page);
  const html = page.locator("html");
  const dialog = page.getByRole("dialog", { name: "App settings" });

  await page.keyboard.press("Control+Comma");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("radio", { name: "Match system" })).toBeChecked();
  await expect(html).toHaveAttribute("data-theme", "mocha");

  await page.emulateMedia({ colorScheme: "light" });
  await expect(html).toHaveAttribute("data-theme", "latte");

  await dialog.getByRole("radio", { name: "Dark", exact: true }).check();
  await expect(html).toHaveAttribute("data-theme", "mocha");
  expect((await callsTo(page, "plugin:window|set_theme")).at(-1)).toMatchObject({ value: "dark" });

  await dialog.getByRole("group", { name: "Dark theme" }).getByRole("radio", { name: "Frappé" }).check();
  await expect(html).toHaveAttribute("data-theme", "frappe");

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();

  await page.keyboard.press("Control+Equal");
  await page.keyboard.press("Control+Equal");
  await expect(html).toHaveAttribute("data-text-size", "larger");
  await page.keyboard.press("Control+Equal");
  await expect(html).toHaveAttribute("data-text-size", "larger");
  await page.keyboard.press("Control+0");
  await expect(html).toHaveAttribute("data-text-size", "default");
  await page.keyboard.press("Control+Minus");
  await expect(html).toHaveAttribute("data-text-size", "small");

  await page.reload();
  await expect(html).toHaveAttribute("data-theme", "frappe");
  await expect(html).toHaveAttribute("data-text-size", "small");
  await page.getByRole("button", { name: "App settings" }).click();
  await expect(dialog.getByRole("group", { name: "Dark theme" }).getByRole("radio", { name: "Frappé" })).toBeChecked();
});

test("App settings opens over a running setup without interrupting it", async ({ page }) => {
  await open(page);
  await page.getByPlaceholder("My new site").fill("Acme Intranet");
  await page.getByRole("button", { name: "5. Review" }).click();
  await page.getByRole("button", { name: "Create site" }).click();
  await page.keyboard.press("Control+Comma");
  await page.keyboard.press("Escape");
  await expect(page.getByText("acme-intranet is ready.")).toBeVisible();
  expect(await callsTo(page, "start_run")).toHaveLength(1);
});

test("the menu shows every item on every view, even while a setup runs", async ({ page }) => {
  await open(page, { holdRun: true });
  await page.getByPlaceholder("My new site").fill("Acme Intranet");
  await page.getByRole("button", { name: "5. Review" }).click();
  await page.getByRole("button", { name: "Create site" }).click();

  const nav = page.getByRole("navigation", { name: "Primary" });
  const items = ["Current setup", "New site", "Sites", "Clone from Git", "Project defaults"];
  for (const current of items) {
    await nav.getByRole("button", { name: current }).click();
    for (const item of items) await expect(nav.getByRole("button", { name: item })).toBeVisible();
    await expect(nav.locator('[aria-current="page"]')).toHaveText(current);
  }

  await nav.getByRole("button", { name: "New site" }).click();
  await page.getByPlaceholder("My new site").fill("Second Site");
  await page.getByRole("button", { name: "5. Review" }).click();
  await expect(page.getByText("Another setup is running.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Create site" })).toBeDisabled();

  await nav.getByRole("button", { name: "Current setup" }).click();
  await expect(page.getByLabel("Setup progress")).toBeVisible();
  expect(await callsTo(page, "start_run")).toHaveLength(1);
});

test("the window opens tall enough to show each New site step without scrolling, except Services", async ({ page }) => {
  const { width, height } = JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8")).app.windows[0];
  await page.setViewportSize({ width, height });
  await open(page);
  const steps = page.getByRole("button", { name: /^\d\. / });
  const overflowing: string[] = [];
  for (let i = 0; i < (await steps.count()); i++) {
    const label = (await steps.nth(i).textContent())!.trim();
    if (label === "4. Services") continue;
    await steps.nth(i).click();
    const scrollHeight = await page.evaluate(() => document.documentElement.scrollHeight);
    if (scrollHeight > height) overflowing.push(`${label} needs ${scrollHeight}px`);
  }
  expect(overflowing, `raise "height" in tauri.conf.json above ${height}px`).toEqual([]);
});

test("creates a Drupal 11 site with a bundle, a recipe and GitHub CI", async ({ page }) => {
  await open(page);
  await page.getByPlaceholder("My new site").fill("Acme Intranet");
  await page.getByRole("button", { name: "4. Services" }).click();
  await page.getByRole("button", { name: /SEO basics/ }).click();
  await page.getByRole("button", { name: /^Forms/ }).click();
  await page.getByLabel("Extra modules to enable").fill("media, admin_toolbar_search");
  await page.getByLabel("CI pipeline").selectOption("github");
  await page.getByRole("button", { name: "Next" }).click();

  await expect(page.getByText("SEO basics, Forms recipe")).toBeVisible();
  await expect(page.getByText("media, admin_toolbar_search")).toBeVisible();
  await expect(page.getByText("GitHub Actions")).toBeVisible();
  await page.getByRole("button", { name: "Create site" }).click();
  await expect(page.getByText("acme-intranet is ready.")).toBeVisible();

  const [run] = await callsTo(page, "start_run");
  expect(run.job.kind).toBe("create");
  expect(run.job.input).toMatchObject({
    name: "acme-intranet",
    bundles: ["seo"],
    recipes: ["forms"],
    extraModules: ["media", "admin_toolbar_search"],
    ci: "github",
  });

  await page.getByRole("navigation").getByRole("button", { name: "Sites" }).click();
  await expect(page.getByText("acme-intranet is ready.")).toBeHidden();
});

test("an invalid extra module sends you back to the Services step", async ({ page }) => {
  await open(page);
  await page.getByPlaceholder("My new site").fill("Acme");
  await page.getByRole("button", { name: "4. Services" }).click();
  await page.getByLabel("Extra modules to enable").fill("media, drupal/webform");
  await page.getByRole("button", { name: "Next" }).click();

  const extraModules = page.getByLabel("Extra modules to enable");
  await expect(page.getByText(`"drupal/webform" isn't a module machine name.`)).toBeVisible();
  await expect(extraModules).toHaveValue("media, drupal/webform");
  await expect(extraModules).toHaveAttribute("aria-invalid", "true");
  await expect(extraModules).toHaveAttribute("aria-describedby", "extra-modules-help extra-modules-error");
});

test("Drupal 10 offers bundles but drops recipes", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "4. Services" }).click();
  await page.getByRole("button", { name: /Privacy basics/ }).click();
  await page.getByRole("button", { name: "2. Drupal and PHP" }).click();
  await page.getByRole("button", { name: /Drupal 10/ }).click();
  await page.getByRole("button", { name: "4. Services" }).click();

  await expect(page.getByRole("button", { name: /Admin essentials/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Privacy basics/ })).toHaveCount(0);
  await expect(page.getByText("Recipes need Drupal 11 or Drupal CMS.")).toBeVisible();
});

test("a preset names the services, bundles and recipes it can't use", async ({ page }) => {
  const preset = {
    name: "Old agency",
    drupal: "10",
    php: "8.3",
    webserver: "nginx-fpm",
    database: { type: "mariadb", version: "10.11" },
    services: ["redis", "couchdb"],
    bundles: [],
    recipes: ["privacy-basic"],
    extraModules: ["media"],
    ci: "none",
    configSyncDir: "../config/sync",
    exportConfigOnInstall: true,
    initGit: true,
  };
  await open(page, { presets: [preset] });
  await page.getByLabel("Start from preset").selectOption("Old agency");

  await expect(page.getByText("Left out because they aren't available for Drupal 10: couchdb, Privacy basics.")).toBeVisible();
});

test("clones a repo, showing field errors from the backend first", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Clone from Git" }).click();
  await page.getByPlaceholder("git@github.com:acme/intranet.git").fill("git@github.com:acme/Intranet.git");
  await expect(page.getByText("https://intranet.ddev.site")).toBeVisible();

  await page.getByLabel(/Import a database dump/).check();
  await page.getByRole("button", { name: "Clone site" }).click();
  await expect(page.getByText(/Choose a database dump/)).toBeVisible();

  await page.getByLabel(/No database yet/).check();
  await page.getByRole("button", { name: "Clone site" }).click();
  await expect(page.getByText("intranet is ready.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Log in as admin" })).toHaveCount(0);

  const [run] = await callsTo(page, "start_run");
  expect(run.job).toMatchObject({ kind: "clone", input: { repoUrl: "git@github.com:acme/Intranet.git", name: "intranet", database: { kind: "none" } } });
});

test("refuses a name DDEV already uses", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Clone from Git" }).click();
  await page.getByPlaceholder("git@github.com:acme/intranet.git").fill("https://github.com/acme/taken-site.git");
  await expect(page.getByText("A DDEV project named taken-site already exists.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Clone site" })).toBeDisabled();
});

test("Project defaults checks the update URL for data and app updates", async ({ page }) => {
  await open(page, { remoteRevision: 2, appUpdate: { version: "0.2.0", currentVersion: "0.1.0", notes: null } });
  await page.getByRole("button", { name: "Project defaults" }).click();
  await page.getByLabel("Update URL").fill("https://updates.example.com/creator");
  await page.getByRole("button", { name: "Check now" }).click();

  await expect(page.getByText("Drupal and service data updated to revision 2.")).toBeVisible();
  await expect(page.getByText("Version 0.2.0 is available (you have 0.1.0).")).toBeVisible();
  const [saved] = await callsTo(page, "save_settings");
  expect(saved.settings.updateUrl).toBe("https://updates.example.com/creator");
});

test("checks for updates on launch only when a URL is set", async ({ page }) => {
  await open(page, { settings: { updateUrl: "https://updates.example.com" }, appUpdate: { version: "0.2.0", currentVersion: "0.1.0", notes: null } });
  await expect(page.getByRole("button", { name: "Update to 0.2.0" })).toBeVisible();

  const quiet = await page.context().newPage();
  await open(quiet);
  await expect(quiet.getByRole("button", { name: "Project defaults" })).toBeVisible();
  expect(await callsTo(quiet, "check_app_update")).toHaveLength(0);
});
