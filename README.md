# Drupal Site Starter

## Overview

Setting up a new Drupal site for local development takes many steps: running
`ddev config` with the right project type, PHP version, web server and
database; picking a PHP version the chosen Drupal release supports; running
`composer create-project`; adding Drush; allowing Composer plugins; installing
code quality tooling; adding DDEV add-ons for services such as Redis or Solr
and wiring each one into Drupal; setting the config sync directory; installing
the site; and creating the first Git commit. Doing this by hand is slow, easy
to get wrong, and hard to repeat the same way twice.

Drupal Site Starter is a desktop app for macOS and Linux that does all of this
for you. You choose your options in a wizard, and the app runs the full setup
with DDEV and gives you a working site.

The app:

- Creates Drupal 10, Drupal 11, Drupal 12 (preview) and Drupal CMS projects.
- Offers only valid combinations of Drupal, PHP and database versions, read
  from a bundled `compat.json` file that can be updated without a new app
  release.
- Uses Nginx or Apache, and MariaDB, MySQL or Postgres.
- Adds services as DDEV add-ons (Mailpit, Adminer, Redis, Solr,
  Elasticsearch, Varnish, Memcached and Selenium) and wires them into Drupal
  where needed.
- Always includes PHPUnit, PHPCS with the Drupal coding standards and
  PHPStan, with ready-to-run `composer lint`, `composer analyse` and
  `composer test` scripts.
- Sets the config sync directory outside the webroot.
- Adds optional module bundles, any other Drupal.org modules you name, Drupal
  CMS recipes and a GitHub Actions or GitLab CI pipeline.
- Starts every project as a Git repository with a Drupal `.gitignore`.
- Checks your system before each run: DDEV version, container runtime
  (Docker Desktop, OrbStack, Colima, Rancher Desktop or Podman), mkcert, free
  ports and disk space.
- Shows a live log of every step, and lets you retry a failed step, resume a
  stopped run, or delete the project and start over.
- Exports the same setup as a shell script, saves your choices as presets,
  and clones existing Drupal projects from a Git repository.
- Lists your DDEV sites in a dashboard, where you can start, stop, snapshot
  and delete them, and import or export their config.

## Install

### Requirements

- [DDEV](https://ddev.com/get-started/) 1.24.0 or newer (1.25.0 or newer with
  Podman).
- A container runtime: Docker Desktop, OrbStack, Colima, Rancher Desktop or
  Podman.
- mkcert, set up with `mkcert -install`.
- Git.

### Download a release

Download the installer for your system from the
[Releases page](https://github.com/kamkejj/drupal-site-starter/releases):

- macOS: the `.dmg` file (Apple silicon or Intel).
- Linux: the `.AppImage` or `.deb` file.

### Build from source

Building from source requires Node.js 24, pnpm, the Rust toolchain and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system.

```bash
git clone https://github.com/kamkejj/drupal-site-starter.git
```

```bash
cd drupal-site-starter
```

```bash
pnpm install
```

```bash
pnpm tauri build
```

## Usage

1. Open Drupal Site Starter and wait for the preflight checks to pass.
2. In the wizard, enter the project name `mysite`, choose a folder, choose
   Drupal 11, keep the default PHP version, web server, database and
   services, select **Next** through each step, and select **Create site**
   on the Review step.
3. When the run finishes, select **Open site** to open
   `https://mysite.ddev.site` and log in with the admin username and password
   from your settings.
