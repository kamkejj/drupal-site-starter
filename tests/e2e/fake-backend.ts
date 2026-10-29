import type { Page } from "@playwright/test";
import { readFileSync } from "node:fs";

const compat = JSON.parse(readFileSync(new URL("../../src-tauri/resources/compat.json", import.meta.url), "utf8"));

export type Options = {
  settings?: Record<string, unknown>;
  /** Revision `check_compat_update` reports; higher than bundled means updated. */
  remoteRevision?: number;
  appUpdate?: { version: string; currentVersion: string; notes: string | null } | null;
  presets?: Record<string, unknown>[];
};

/**
 * Installs a stand-in for the Rust backend before the app loads. Every
 * invoke is recorded in `window.__calls`. Validation mirrors only what the
 * tests exercise; the Rust unit tests cover the real rules.
 */
export async function fakeBackend(page: Page, options: Options = {}) {
  await page.addInitScript(
    ({ compat, options }) => {
      const settings = {
        adminName: "admin",
        adminEmail: "admin@example.com",
        projectsDir: "/Users/me/Projects",
        services: ["mailpit", "adminer"],
        configSyncDir: "../config/sync",
        ide: "code",
        updateUrl: "",
        ...options.settings,
      };
      const calls: { cmd: string; args: any }[] = [];
      (window as any).__calls = calls;
      const callbacks = new Map<number, (data: unknown) => void>();
      let nextId = 1;

      const machineName = (s: string) =>
        s.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 63);

      function stepsFor(job: any) {
        if (job.kind === "clone") {
          const db = job.input.database.kind;
          const ids = ["git-clone", "ddev-config", "ddev-name", "ddev-start", "composer-install"];
          if (db === "existingConfig") ids.push("site-install", "admin-password");
          if (db === "dump") ids.push("import-db", "cache-rebuild");
          return ids;
        }
        const i = job.input;
        return [
          "mkdir", "ddev-config", "ddev-start", "create-project", "core-dev", "site-install",
          ...i.bundles.map((b: string) => `bundle:${b}:enable`),
          ...i.recipes.map((r: string) => `recipe:${r}`),
          ...(i.ci === "none" ? [] : [`ci:${i.ci}`]),
        ];
      }

      function plan(job: any) {
        const i = job.input;
        if (job.kind === "clone") {
          const errors = [];
          if (!/^(https?|ssh|git|file):\/\/.+|^[^\s/@]+@[^\s/:]+:.+/.test(i.repoUrl))
            errors.push({ field: "repoUrl", message: "Enter an HTTPS or SSH Git URL, such as git@github.com:me/site.git." });
          if (i.database.kind === "dump" && !i.database.path)
            errors.push({ field: "database", message: "Choose a database dump: .sql, .sql.gz, .sql.bz2, .sql.xz, .zip or .tar.gz." });
          if (errors.length) throw errors;
        }
        return {
          name: i.name,
          steps: stepsFor(job).map((id) => ({ id, title: id, timeoutSecs: 60, retry: { kind: "rerun" } })),
          script: job.kind === "create" ? "#!/usr/bin/env bash\n" : null,
          dir: `${i.parentDir}/${i.name}`,
          folderWarning: null,
          needsPassword: job.kind === "create" || i.database.kind === "existingConfig",
        };
      }

      function send(channel: { id: number }, index: number, message: unknown) {
        callbacks.get(channel.id)?.({ index, message });
      }

      const handlers: Record<string, (args: any) => unknown> = {
        get_compat: () => compat,
        load_settings: () => ({ settings, hasPassword: true }),
        save_settings: ({ settings: s }) => Object.assign(settings, s),
        preflight: () => [{ id: "ddev", label: "DDEV", level: "ok", detail: "v1.25.5" }],
        existing_projects: () => ["taken-site"],
        list_presets: () => options.presets ?? [],
        list_sites: () => [],
        machine_name: ({ display }) => machineName(display),
        suggested_clone_name: ({ url }) =>
          machineName((url.replace(/\/+$/, "").split(/[/:]/).pop() ?? "").replace(/\.git$/, "")),
        plan_project: ({ job }) => plan(job),
        start_run: ({ job, onEvent }) => {
          const ids = stepsFor(job);
          let n = 0;
          for (const id of ids) {
            send(onEvent, n++, { type: "step", id, status: "running" });
            send(onEvent, n++, { type: "log", id, line: `ran ${id}` });
            send(onEvent, n++, { type: "step", id, status: "done" });
          }
          send(onEvent, n, { type: "finished", ok: true, cancelled: false });
          return "/tmp/run.log";
        },
        check_compat_update: () => {
          const revision = options.remoteRevision ?? compat.revision;
          return { revision, updated: revision > compat.revision };
        },
        check_app_update: () => options.appUpdate ?? null,
      };

      (window as any).__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
        transformCallback(cb: (data: unknown) => void) {
          const id = nextId++;
          callbacks.set(id, cb);
          return id;
        },
        unregisterCallback: (id: number) => callbacks.delete(id),
        async invoke(cmd: string, args: any) {
          calls.push({ cmd, args: JSON.parse(JSON.stringify(args ?? {})) });
          const handler = handlers[cmd];
          if (handler) return handler(args ?? {});
          if (cmd.startsWith("plugin:")) return null;
          throw `fake backend has no ${cmd}`;
        },
      };
    },
    { compat, options },
  );
}

export async function callsTo(page: Page, cmd: string): Promise<any[]> {
  return page.evaluate((cmd) => (window as any).__calls.filter((c: any) => c.cmd === cmd).map((c: any) => c.args), cmd);
}
