# Drupal DDEV Site Creator — Project Plan

A cross-platform desktop GUI app that creates new Drupal local dev sites with
DDEV, based on a set of user-selected options.

---

## 1. Goals

- Create Drupal 10, Drupal 11 or Drupal CMS projects in a few clicks (and
  newer versions, such as Drupal 12, by updating `compat.json` alone).
- Always use DDEV, running on Docker (Docker Desktop, OrbStack, Colima,
  Rancher Desktop) or Podman.
- Enforce valid combinations (for example, only PHP versions the chosen Drupal
  version supports).
- Always include code quality and testing tooling.
- Keep setups reproducible with presets and script export.
- Every project starts as a Git repository with a sensible `.gitignore`.

---

## 2. Features

### 2.1 Global Settings (saved once, reused)

- **Default user 1 credentials:** username, email and password.
  - The password is stored in the OS keychain, never in plain text.
  - Each project can override these.
- **Default projects folder,** such as `~/Projects`.
- **Container runtime:** detected, not chosen per project (see 2.5).
- **Default services preset.**
- **Default config sync path:** `../config/sync`.
- **Default IDE** for the "Open in IDE" action.

### 2.2 Project Basics

- **Project name:** sanitized into a machine name automatically, which becomes
  `<name>.ddev.site`.
  - Valid DDEV names are lowercase letters, numbers and hyphens (hostname
    rules).
  - Check that no DDEV project already uses it (`ddev list --json-output`).
- **File path:** a folder picker. The project folder is `<path>/<name>`.
  - Warn if the folder already exists or isn't empty.
  - Block folders inside another DDEV project.

### 2.3 Drupal Choice

All of this lives in `compat.json`, not in code.

| Option     | Composer package                   | DDEV type  | Install profile           | Includes Drush |
| ---------- | ---------------------------------- | ---------- | ------------------------- | -------------- |
| Drupal 10  | `drupal/recommended-project:^10`   | `drupal10` | `standard`                | No             |
| Drupal 11  | `drupal/recommended-project:^11`   | `drupal11` | `standard`                | No             |
| Drupal CMS | `drupal/cms`                       | `drupal11` | Drupal CMS's own (verify) | Yes            |

- Drupal 10 reaches end of life around December 2026. Mark it as "ending
  soon" in the UI and drop it from `compat.json` when support ends.
- Adding Drupal 12 should be a `compat.json` change only.

### 2.4 PHP and Database Versions (filtered by the Drupal choice)

| Drupal     | Allowed PHP               | Default |
| ---------- | ------------------------- | ------- |
| 10.x       | 8.1 – 8.3 (8.4 on 10.4+)  | 8.3     |
| 11.x       | 8.3 – 8.4 (8.5 on 11.3+?) | 8.3     |
| Drupal CMS | Same as its Drupal core   | 8.3     |

- This table is a snapshot. Check PHP and database support against the
  Drupal.org requirements pages before each release.
- `compat.json` holds Drupal ↔ PHP ↔ database versions, install profiles and
  the minimum DDEV version.

### 2.5 Environment

- **Container runtime:** detected in preflight, not stored per project. DDEV
  uses whatever Docker context or Podman setup is active.
  - Detect Docker Desktop, OrbStack, Colima, Rancher Desktop and Podman.
  - Check the runtime is running: `docker info`, `podman info` or
    `colima status`.
  - Podman needs a recent DDEV version (around v1.25+); check `ddev version`.
- **Web server:** Nginx (`nginx-fpm`) or Apache (`apache-fpm`).
- **Database:** MariaDB, MySQL or Postgres, versions from `compat.json`.
  - If Postgres is selected, warn that some contrib modules don't fully
    support it.

### 2.6 Services (preset, but customizable)

Each service has two parts: the DDEV add-on (container) and, where needed, the
Drupal wiring (module plus settings). The UI shows which services are wired
into Drupal and which are "container only".

| Service       | Add-on                                 | Drupal wiring                         | Default |
| ------------- | -------------------------------------- | ------------------------------------- | ------- |
| Mailpit       | Built into DDEV                        | None needed                           | On      |
| Adminer       | `ddev/ddev-adminer`                    | None needed                           | On      |
| Redis         | `ddev/ddev-redis`                      | `drupal/redis` (the add-on writes the `settings.php` block) | Off |
| Solr          | `ddev/ddev-solr`                       | `drupal/search_api_solr` + server and configset | Off |
| Elasticsearch | `ddev/ddev-elasticsearch`              | `drupal/elasticsearch_connector` (alpha) + server | Off |
| Varnish       | `ddev/ddev-varnish`                    | Container only (see below)            | Off     |
| Memcached     | `ddev/ddev-memcached`                  | `drupal/memcache` + `settings.php` block | Off  |
| Selenium      | `ddev/ddev-selenium-standalone-chrome` | None needed (the add-on sets the WebDriver env) | Off |

- **UI:** toggle chips with a "Reset to preset" button.
- Users can save their own service presets.
- Varnish stays "container only". Drupal's default page max-age is 0, so
  Varnish serves nothing stale. Wiring `drupal/purge` also needs a VCL that
  handles BAN by cache tag, which means replacing the add-on's own VCL.

### 2.7 Always Included: Code Quality and Testing

Adding `drupal/core-dev` (same major version as core) as a dev dependency
brings in:

- PHPUnit
- PHPCS with Coder (Drupal and DrupalPractice standards)
- PHPStan with `mglaman/phpstan-drupal`

Before installing it, the app allows its Composer plugins
(`dealerdirect/phpcodesniffer-composer-installer`,
`phpstan/extension-installer` and any others listed in `compat.json`), or
Composer stops and asks.

The app also generates these files:

- `phpcs.xml.dist`
- `phpstan.neon` (level 1 to start)
- `phpunit.xml`, with:
  - `SIMPLETEST_DB` set for the chosen database (`mysql://db:db@db/db` or
    `pgsql://db:db@db/db`)
  - `SIMPLETEST_BASE_URL=http://web`
  - the Selenium/WebDriver URL when that service is on
- Composer scripts for each tool: `composer lint`, `composer analyse` and
  `composer test`, run as `ddev composer lint` and so on.

### 2.8 Config Import/Export Directory

- Choose the path; the default is `../config/sync`, outside the webroot.
- The app creates the folder (`mkdir -p`) and writes this setting to
  `settings.php`, **above** the `settings.ddev.php` include. DDEV's file only
  sets a sync directory when none is set yet.

  ```php
  $settings['config_sync_directory'] = '../config/sync';
  ```

- **Optional:** run `drush cex` right after install to create the first config
  export.
- Import (`drush cim`) and Export (`drush cex`) buttons live on the Sites
  dashboard.

### 2.9 Git

- `git init`, a Drupal `.gitignore` (vendor, `web/core`, contrib, files,
  `settings.local.php`, `.ddev` generated files) and a first commit after
  install.
- Skipped with a warning if `git` isn't installed.

---

## 3. Command Pipeline

The live run and "Export as script" both use the same ordered list of steps,
so the two can't drift apart.

### Running commands from a GUI app

- **PATH:** apps launched from Finder or the Dock don't get the shell's PATH,
  so Homebrew's `/opt/homebrew/bin` is missing. On startup, load the PATH from
  the user's login shell (`fix-path-env` crate or `$SHELL -lc 'echo $PATH'`).
- **Only `ddev` is needed on the host.** Composer and Drush run inside the
  containers through `ddev composer` and `ddev drush`.
- **No prompts.** There is no terminal to answer one. Always set
  `DDEV_NONINTERACTIVE=true`, pass `-n` to Composer and `-y` to Drush.
- **Arguments, not shell strings.** Spawn `ddev` with an argument array; never
  build a shell string from user input.
- **Secrets.** The admin password goes in through an environment variable,
  never on the command line. The log viewer hides it. Exported scripts read it
  from `$DRUPAL_ADMIN_PASS` or ask for it, and never contain the value.

### Steps

```bash
set -euo pipefail
export DDEV_NONINTERACTIVE=true

mkdir -p ~/Sites/mysite && cd ~/Sites/mysite
ddev config --project-name=mysite --project-type=drupal11 --docroot=web \
  --php-version=8.3 --webserver-type=nginx-fpm --database=mariadb:10.11
ddev start
ddev composer create-project drupal/recommended-project:^11 -n   # or drupal/cms
ddev composer require drush/drush -n                    # skipped for Drupal CMS
ddev composer config allow-plugins.dealerdirect/phpcodesniffer-composer-installer true
ddev composer config allow-plugins.phpstan/extension-installer true
ddev composer require --dev 'drupal/core-dev:^11' --with-all-dependencies -n
ddev add-on get ddev/ddev-adminer                        # per selected service
ddev restart
# write templates: phpcs.xml.dist, phpstan.neon, phpunit.xml, composer scripts
# write config_sync_directory to settings.php (above the DDEV include)
mkdir -p config/sync
ddev drush site:install standard -y \
  --account-name=admin --account-mail=me@example.com \
  --account-pass="$DRUPAL_ADMIN_PASS" --site-name='My Site'
# per-service Drupal wiring (modules + settings), then:
ddev drush cex -y                                        # optional
git init && git add -A && git commit -m "Initial Drupal site"
```

### Step contract

Each step defines:

- **Run:** the command (or file write) and a timeout.
- **Check:** how to tell if it's already done (for safe resume).
- **Retry:** whether it can simply run again, or needs a cleanup first (for
  example, `site:install` needs the database emptied; `create-project` needs
  an empty folder).
- **Script form:** how it renders in the exported `.sh` file.

The run state is saved to a file in the project folder so a failed or closed
run can be resumed. The Create screen has **Cancel** and **Delete and start
over** (`ddev delete -Oy` plus removing the folder, after confirmation).

---

## 4. UX Flow

1. **Preflight:** on first launch and before each run. Checks:
   - DDEV is installed and at least the minimum version in `compat.json`
   - the container runtime is running
   - `mkcert -install` has been run (trusted HTTPS)
   - ports 80 and 443 are free
   - enough free disk space
2. **Wizard:** Project → Drupal and PHP → Environment → Services → Review.
3. **Review:** shows the generated commands and has an "Export as script"
   option.
4. **Create:** shows a live log with progress for each step. If a step fails,
   you can retry it (using its retry rule), cancel, or delete and start over.
   Each run's full log is saved to a file.
5. **Done:** buttons to open the site, open the project in your IDE, or run a
   one-time login (`drush uli`).

---

## 5. Tech Stack

### Recommendation: Tauri 2 + TypeScript (Svelte)

**Why it fits:**

- **Small and native:** around 10 MB, compared with about 150 MB for Electron.
  Runs on macOS and Linux; Windows is planned (see 5.1).
- **Running shell commands:** the Rust backend starts `ddev` and streams its
  output to the UI line by line.
- **Keychain support:** the `keyring` crate uses macOS Keychain, Windows
  Credential Manager or the Linux Secret Service.
- **Security:** the frontend can only call a small set of typed Tauri
  commands. Rust validates every input and spawns only `ddev` (and `git`)
  with argument arrays. Running processes through `tokio::process` skips
  Tauri's shell-plugin allowlist, so this validation is the real boundary.
- **Little Rust needed:** most of the code is TypeScript.

| Layer                 | Choice                                                   |
| --------------------- | -------------------------------------------------------- |
| Shell                 | Tauri 2                                                  |
| Frontend              | SvelteKit (static adapter)                               |
| UI kit                | shadcn-svelte + Tailwind                                 |
| Forms and validation  | Zod with Superforms                                      |
| State                 | Svelte stores                                            |
| Process runner        | Rust `tokio::process` and Tauri events for log streaming |
| PATH fix              | `fix-path-env` crate (or login-shell lookup)             |
| Secrets               | `keyring` crate                                          |
| Settings and presets  | `tauri-plugin-store` (JSON)                              |
| Folder picker         | `tauri-plugin-dialog`                                    |
| Compatibility data    | Bundled `compat.json`, with an optional remote update    |
| Packaging and updates | Tauri bundler (`.dmg`, `.msi`, `.AppImage`/`.deb`) and `tauri-plugin-updater` |
| Testing               | Vitest (UI), `cargo test` (runner), Playwright (end-to-end) |

### 5.1 Windows (WSL2)

On Windows, DDEV runs inside WSL2. The app has to run `wsl ddev …` and
convert Windows paths from the folder picker to Linux paths (projects should
live in the WSL filesystem for speed). The runner uses a `CommandExecutor`
interface (native vs. WSL) from the start, but Windows support ships after the
MVP.

### Alternatives

| Option                    | When to choose it                                         |
| ------------------------- | --------------------------------------------------------- |
| Wails (Go)                | You'd rather write Go. DDEV is written in Go, so you can reuse its patterns. |
| Electron + TypeScript     | You want everything in JS/Node with no Rust; the app is heavier. |
| Local web UI (PHP)        | You want to stay in PHP; it isn't a real desktop app, and secrets and folder picking are clumsier. |

---

## 6. Architecture

```text
UI (Svelte)
  └─ Wizard → builds a ProjectConfig (validated with Zod)
       └─ invoke("create_project", config)

Rust backend
  ├─ env: login-shell PATH, DDEV_NONINTERACTIVE, CommandExecutor (native/WSL)
  ├─ preflight: ddev version, runtime, mkcert, ports, disk
  ├─ pipeline: ordered steps → spawn → emit("log", line) / emit("step", status)
  │    └─ run state file, cancel, resume, delete-and-restart
  ├─ secrets: keyring get/set, log redaction
  ├─ templates: phpcs/phpstan/phpunit, settings.php edits, .gitignore
  └─ script export: render the same steps to a .sh file (no secrets)
```

### Suggested Project Structure

```text
drupal-ddev-creator/
├─ src/                      # Svelte frontend
│  ├─ lib/
│  │  ├─ schema/             # Zod schemas (ProjectConfig, Settings, Preset)
│  │  ├─ stores/             # settings, presets, run state
│  │  └─ components/         # wizard steps, log console, service chips
│  └─ routes/                # preflight, wizard, settings, dashboard
├─ src-tauri/
│  ├─ src/
│  │  ├─ env.rs              # PATH fix, CommandExecutor
│  │  ├─ preflight.rs
│  │  ├─ pipeline.rs         # step definitions, runner, run state
│  │  ├─ secrets.rs
│  │  ├─ templates.rs        # file generation and settings.php edits
│  │  └─ main.rs
│  ├─ resources/
│  │  ├─ compat.json         # Drupal ↔ PHP ↔ DB, profiles, min DDEV version
│  │  └─ templates/          # phpcs.xml.dist, phpstan.neon, phpunit.xml, .gitignore
│  └─ tauri.conf.json
└─ PLAN.md
```

### Core Data Model (draft)

```ts
// Drupal, PHP and database options come from compat.json, not from code.
type ProjectConfig = {
  name: string;
  path: string;
  drupal: string;            // key in compat.json, e.g. "11" or "cms"
  php: string;
  webserver: "nginx-fpm" | "apache-fpm";
  database: { type: "mariadb" | "mysql" | "postgres"; version: string };
  services: string[];
  admin: { name: string; email: string; passwordRef: string };
  configSyncDir: string;
  exportConfigOnInstall: boolean;
  initGit: boolean;
};
```

---

## 7. Testing the App

- **Dry-run runner:** a mock `CommandExecutor` that records commands and plays
  back canned output, so UI and pipeline tests run in seconds.
- **Snapshot tests:** the step list and exported script for a set of sample
  configs.
- **Smoke test:** one real DDEV run in CI (Linux) for Drupal 11 and Drupal CMS.

---

## 8. Roadmap

### MVP (macOS and Linux)

- Preflight checks, PATH fix and non-interactive runs
- Global settings, with secrets in the keychain
- Wizard with filtering based on `compat.json`
- Pipeline runner with a live log, saved logs, cancel, retry and resume
- "Export as script" (reuses the same steps)
- Code quality and testing files always generated
- Config sync directory setup
- Git init with `.gitignore`
- Redis and Solr wired into Drupal; other services as "container only"

### v1.1

- Saved presets, with import/export as JSON or YAML
- Done screen actions (launch, IDE, `drush uli`)
- Drupal wiring for the remaining services

### v1.2

- Dashboard of existing sites: start, stop, delete, snapshot, `cim`/`cex`
  - Lists every project in `ddev list`, not only ones this app created.
  - Delete is `ddev delete -y`: DDEV snapshots the database first, and the
    project files stay, since the site may be the user's own work.
- Remote update of `compat.json`
- Auto-updater
- Windows support via WSL2

### Later

- Clone an existing project from a Git repo
- Module bundles and Drupal Recipes
- CI templates (GitHub Actions, GitLab CI)
