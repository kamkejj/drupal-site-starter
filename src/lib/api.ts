import { Channel, invoke } from "@tauri-apps/api/core";

// Mirrors the Rust types in src-tauri/src. Rust parses every input; these
// types only describe what crosses the bridge.

export type DbType = "mariadb" | "mysql" | "postgres";
export type Database = { type: DbType; version: string };
export type Webserver = "nginx-fpm" | "apache-fpm";

export type DrupalRelease = {
  id: string;
  label: string;
  package: string;
  ddevType: string;
  installProfile: string;
  composerConfig?: [string, string][];
  reinstall?: string[];
  drushPackage: string | null;
  devPackage: string;
  php: string[];
  defaultPhp: string;
  databases: Partial<Record<DbType, string[]>>;
  defaultDatabase: Database;
  status: "supported" | "ending-soon" | "preview";
  statusNote?: string | null;
};

export type Wiring =
  | { kind: "none" }
  | { kind: "containerOnly" }
  | { kind: "module"; packages: string[]; modules: string[]; setup: { title: string; drush: string[] }[]; settings: string | null };

export type Service = {
  id: string;
  label: string;
  addon: string | null;
  wiring: Wiring;
  defaultOn: boolean;
};

export type Bundle = {
  id: string;
  label: string;
  description: string;
  packages: string[];
  dev: boolean;
  modules: string[];
  releases: string[];
};

export type Recipe = {
  id: string;
  label: string;
  description: string;
  package: string;
  releases: string[];
};

/** Empty `releases` means every release. */
export const availableFor = (item: { releases: string[] }, release: string) =>
  item.releases.length === 0 || item.releases.includes(release);

export type Compat = {
  revision: number;
  minDdevVersion: string;
  minDdevVersionPodman: string;
  composerPlugins: string[];
  drupal: DrupalRelease[];
  services: Service[];
  bundles: Bundle[];
  recipes: Recipe[];
};

export type Ci = "none" | "github" | "gitlab";

export type ProjectInput = {
  name: string;
  parentDir: string;
  drupal: string;
  php: string;
  webserver: Webserver;
  database: Database;
  services: string[];
  bundles: string[];
  recipes: string[];
  ci: Ci;
  adminName: string;
  adminEmail: string;
  siteName: string;
  configSyncDir: string;
  exportConfigOnInstall: boolean;
  initGit: boolean;
  httpsPort: number | null;
};

export type DbSource = { kind: "existingConfig" } | { kind: "dump"; path: string } | { kind: "none" };

export type CloneInput = {
  repoUrl: string;
  parentDir: string;
  name: string;
  drupal: string;
  database: DbSource;
  adminName: string;
  adminEmail: string;
};

/** A run the Run screen drives. */
export type Job = { kind: "create"; input: ProjectInput } | { kind: "clone"; input: CloneInput };

export type FieldError = { field: keyof ProjectInput | keyof CloneInput; message: string };

export type Retry =
  | { kind: "rerun" }
  | { kind: "cleanupFirst" }
  | { kind: "startOver"; reason: string };

export type Step = { id: string; title: string; timeoutSecs: number; retry: Retry };

export type Plan = {
  name: string;
  steps: Step[];
  script: string | null;
  dir: string;
  folderWarning: string | null;
  needsPassword: boolean;
};

export type AppSettings = {
  adminName: string;
  adminEmail: string;
  projectsDir: string;
  services: string[];
  configSyncDir: string;
  ide: string;
  updateUrl: string;
};

export type Preset = Pick<
  ProjectInput,
  | "drupal"
  | "php"
  | "webserver"
  | "database"
  | "services"
  | "bundles"
  | "recipes"
  | "ci"
  | "configSyncDir"
  | "exportConfigOnInstall"
  | "initGit"
> & { name: string };

export type CompatUpdate = { revision: number; updated: boolean };
export type AppUpdate = { version: string; currentVersion: string; notes: string | null };

export type CheckResult = {
  id: string;
  label: string;
  level: "ok" | "warn" | "fail";
  detail: string;
};

export type Site = {
  name: string;
  status: string;
  kind: string;
  dir: string;
  url: string | null;
};

export type SiteAction = "start" | "stop" | "snapshot" | "configImport" | "configExport" | "delete";

export type StepStatus =
  | { status: "running" }
  | { status: "done" }
  | { status: "skipped" }
  | { status: "failed"; message: string; retry: Retry };

export type RunEvent =
  | ({ type: "step"; id: string } & StepStatus)
  | { type: "log"; id: string; line: string }
  | { type: "finished"; ok: boolean; cancelled: boolean };

export const api = {
  compat: () => invoke<Compat>("get_compat"),
  machineName: (display: string) => invoke<string>("machine_name", { display }),
  loadSettings: () =>
    invoke<{ settings: AppSettings; hasPassword: boolean }>("load_settings"),
  saveSettings: (settings: AppSettings, password: string | null) =>
    invoke<void>("save_settings", { settings, password }),
  preflight: (projectsDir: string) => invoke<CheckResult[]>("preflight", { projectsDir }),
  existingProjects: () => invoke<string[]>("existing_projects"),
  portUser: (port: number) => invoke<string | null>("port_user", { port }),
  listSites: () => invoke<Site[]>("list_sites"),
  siteAction: (name: string, action: SiteAction) => invoke<void>("site_action", { name, action }),
  projectUrl: (name: string) => invoke<string>("project_url", { name }),
  plan: (job: Job) => invoke<Plan>("plan_project", { job }),
  exportScript: (input: ProjectInput, path: string) =>
    invoke<void>("export_script", { input, path }),
  startRun: (job: Job, password: string | null, resume: boolean, onEvent: (e: RunEvent) => void) => {
    const channel = new Channel<RunEvent>();
    channel.onmessage = onEvent;
    return invoke<string>("start_run", { job, password, resume, onEvent: channel });
  },
  cancelRun: () => invoke<void>("cancel_run"),
  deleteProject: (job: Job) => invoke<void>("delete_project", { job }),
  listPresets: () => invoke<Preset[]>("list_presets"),
  savePreset: (preset: Preset) => invoke<Preset[]>("save_preset", { preset }),
  deletePreset: (name: string) => invoke<Preset[]>("delete_preset", { name }),
  importPresets: (path: string) => invoke<Preset[]>("import_presets", { path }),
  exportPresets: (path: string) => invoke<void>("export_presets", { path }),
  openInIde: (job: Job) => invoke<void>("open_in_ide", { job }),
  loginLink: (job: Job) => invoke<string>("login_link", { job }),
  suggestedCloneName: (url: string) => invoke<string>("suggested_clone_name", { url }),
  checkCompatUpdate: () => invoke<CompatUpdate>("check_compat_update"),
  checkAppUpdate: () => invoke<AppUpdate | null>("check_app_update"),
  installAppUpdate: () => invoke<void>("install_app_update"),
};

/** `plan_project` rejects with a FieldError list; other commands with a string. */
export function fieldErrors(e: unknown): FieldError[] {
  return Array.isArray(e) ? (e as FieldError[]) : [];
}
