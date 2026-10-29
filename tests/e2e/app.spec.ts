import { expect, test, type Page } from "@playwright/test";
import { callsTo, fakeBackend, type Options } from "./fake-backend";

async function open(page: Page, options: Options = {}) {
  await fakeBackend(page, options);
  await page.goto("/");
  await page.getByRole("button", { name: "Continue" }).click();
}

test("creates a Drupal 11 site with a bundle, a recipe and GitHub CI", async ({ page }) => {
  await open(page);
  await page.getByPlaceholder("My new site").fill("Acme Intranet");
  await page.getByRole("button", { name: "4. Services" }).click();
  await page.getByRole("button", { name: /SEO basics/ }).click();
  await page.getByRole("button", { name: /^Forms/ }).click();
  await page.getByLabel("CI pipeline").selectOption("github");
  await page.getByRole("button", { name: "Next" }).click();

  await expect(page.getByText("SEO basics, Forms recipe")).toBeVisible();
  await expect(page.getByText("GitHub Actions")).toBeVisible();
  await page.getByRole("button", { name: "Create site" }).click();
  await expect(page.getByText("acme-intranet is ready.")).toBeVisible();

  const [run] = await callsTo(page, "start_run");
  expect(run.job.kind).toBe("create");
  expect(run.job.input).toMatchObject({ name: "acme-intranet", bundles: ["seo"], recipes: ["forms"], ci: "github" });
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

test("Settings checks the update URL for data and app updates", async ({ page }) => {
  await open(page, { remoteRevision: 2, appUpdate: { version: "0.2.0", currentVersion: "0.1.0", notes: null } });
  await page.getByRole("button", { name: "Settings" }).click();
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
  await expect(quiet.getByRole("button", { name: "Settings" })).toBeVisible();
  expect(await callsTo(quiet, "check_app_update")).toHaveLength(0);
});
