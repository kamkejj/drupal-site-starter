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
  coreMajor: number;
  installProfile: string;
  includesDrush: boolean;
  php: string[];
  defaultPhp: string;
  databases: Partial<Record<DbType, string[]>>;
  defaultDatabase: Database;
  status: "supported" | "ending-soon";
  statusNote?: string | null;
};

export type Wiring =
  | { kind: "none" }
  | { kind: "containerOnly" }
  | { kind: "module"; packages: string[]; modules: string[] };

export type Service = {
  id: string;
  label: string;
  addon: string | null;
  wiring: Wiring;
  defaultOn: boolean;
};

export type Compat = {
  minDdevVersion: string;
  minDdevVersionPodman: string;
  composerPlugins: string[];
  drupal: DrupalRelease[];
  services: Service[];
};

export type ProjectInput = {
  name: string;
  parentDir: string;
  drupal: string;
  php: string;
  webserver: Webserver;
  database: Database;
  services: string[];
  adminName: string;
  adminEmail: string;
  siteName: string;
  configSyncDir: string;
  exportConfigOnInstall: boolean;
  initGit: boolean;
};

export type FieldError = { field: keyof ProjectInput; message: string };

export type Retry =
  | { kind: "rerun" }
  | { kind: "cleanupFirst" }
  | { kind: "startOver"; reason: string };

export type Step = { id: string; title: string; timeoutSecs: number; retry: Retry };

export type Plan = { steps: Step[]; script: string; dir: string; folderWarning: string | null };

export type AppSettings = {
  adminName: string;
  adminEmail: string;
  projectsDir: string;
  services: string[];
  configSyncDir: string;
  ide: string;
};

export type CheckResult = {
  id: string;
  label: string;
  level: "ok" | "warn" | "fail";
  detail: string;
};

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
  projectUrl: (name: string) => invoke<string>("project_url", { name }),
  plan: (input: ProjectInput) => invoke<Plan>("plan_project", { input }),
  exportScript: (input: ProjectInput, path: string) =>
    invoke<void>("export_script", { input, path }),
  startRun: (
    input: ProjectInput,
    password: string | null,
    resume: boolean,
    onEvent: (e: RunEvent) => void,
  ) => {
    const channel = new Channel<RunEvent>();
    channel.onmessage = onEvent;
    return invoke<string>("start_run", { input, password, resume, onEvent: channel });
  },
  cancelRun: () => invoke<void>("cancel_run"),
  deleteProject: (input: ProjectInput) => invoke<void>("delete_project", { input }),
};

/** `plan_project` rejects with a FieldError list; other commands with a string. */
export function fieldErrors(e: unknown): FieldError[] {
  return Array.isArray(e) ? (e as FieldError[]) : [];
}
