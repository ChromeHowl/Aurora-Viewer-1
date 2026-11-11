# Windows installer (Aurora Viewer +)

**Aurora Viewer +** is a personal build of Aurora Viewer with the changes
of this fork. It is not an official Aurora Viewer release
(official builds: [aurora-viewer.org](https://aurora-viewer.org) and the
[upstream releases](https://github.com/Aurora-Viewer/Aurora-Viewer/releases);
see [TRADEMARKS.md](../TRADEMARKS.md)).

Its installer is an ordinary Windows setup program that contains **no
viewer program**: it builds the viewer on the user's PC, from this fork's
public code, and keeps it up to date the same way. Nobody types a command.

## For users: three steps

1. **Run `AuroraViewerPlus-Setup.exe`.** Windows SmartScreen may say the
   program is not recognised (it is not signed): click *More info*, then
   *Run anyway*. Accept the licence (GPLv3), read the note about this build,
   choose the folder (default: `%LOCALAPPDATA%\Programs\AuroraViewerPlus`,
   no administrator rights needed) and whether you want a desktop shortcut.
2. **Let it build.** A window named *Aurora Viewer + - Install* shows each
   step in plain words, with a clock:
   - *Checking this PC...* If tools are missing, it lists them (Visual Studio
     2022 C++ build tools, Rust, Git) with their size, and asks before
     installing them. The Visual Studio tools need administrator rights:
     Windows asks, click *Yes*; the Visual Studio Installer shows its own
     progress.
   - *Downloading the latest code...*
   - *Building - this can take 20-40 minutes, your PC is working.* The count
     of compiled parts goes up; the last part (the viewer itself) shows no
     progress for 10-40 minutes: this is normal, leave the window open.
   - *Installing the new version...* then *Done*, with a *Start the viewer*
     button.

   The first time needs about 20 GB of free disk space and 20-60 minutes.
3. **Start it** from the Start menu (*Aurora Viewer +*) or the desktop.

**Updating:** Start menu > *Update Aurora Viewer +*. It downloads the
latest code, builds what changed (usually much faster than the first time)
and installs it. If nothing changed it says *already up to date* and offers
to build again anyway. The viewer must be closed when the new version is put
in place; the update asks for it.

**Something went wrong after an update?** Start menu > *Go back to the
previous version*. The version before the last update is always kept.

**Uninstalling:** Windows Settings > Apps > *Aurora Viewer +* >
Uninstall. It removes the viewer, its build files and the private Git
copy, and the `secondlife:` link handler when it opens this viewer. It
then asks whether to also delete your viewer data; the answer is *No*
unless you choose *Yes*, which deletes:

- the settings, chat logs and cache (`%APPDATA%\Aurora\AuroraViewer` and
  `%LOCALAPPDATA%\Aurora\AuroraViewer`);
- from the Windows credential store: the saved logins (passwords and
  second-factor authorizations), the cache key, the translation services'
  keys, the SOCKS 5 proxy user name and password, and the GitHub
  authorization of crash reports;
- the Aurora add-on of Blender and the Windows notification identity.

All of these are shared with any official Aurora Viewer on the same PC.
The local patches (below) are never deleted. The tools installed for the
build (Visual Studio Build Tools, Rust) stay: they are ordinary programs,
removable from Windows Settings > Apps (Rust: `rustup self uninstall`).

**Install folder:** a new or empty folder (Setup refuses a folder that
already holds other files, since uninstalling deletes the folders it made
there: `viewer`, `build`, `tools`, `logs`…). With *Browse*, Setup adds the
`AuroraViewerPlus` subfolder.

## What it installs, and from where

| Tool | Source | Check |
|---|---|---|
| Visual Studio 2022 Build Tools, workload *Desktop development with C++* (`Microsoft.VisualStudio.Workload.VCTools`, recommended components: MSVC x64 and the Windows SDK) | `winget` (`Microsoft.VisualStudio.2022.BuildTools`) when available, else Microsoft's bootstrapper `https://aka.ms/vs/17/release/vs_BuildTools.exe` | the bootstrapper must carry a valid Authenticode signature from Microsoft Corporation |
| Rust: rustup and the newest stable toolchain (profile minimal), for the user's account; before each build, `rustup update stable` brings the stable toolchain up to date (the consent dialog says so) | `https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe` | SHA-256 against the `.sha256` file published next to it |
| Git (MinGit, a private copy in the install folder), only when the PC has no Git 2.30 or newer | the latest release of `git-for-windows/git` on GitHub | SHA-256 against the digest GitHub publishes for the asset; Authenticode signature of `git.exe` (the Git for Windows code-signing certificate) |

Everything is downloaded over HTTPS only. Rust is updated to the newest
stable at every build, and must be at least the `rust-version` of the
code's `Cargo.toml` when it has one, else `minimumRust` of
`installer/engine/settings.json` (1.95.0). CMake is not needed: the viewer
is built without the `xr-static-loader` feature and uses the VR runtime's
own `openxr_loader.dll`.

## How it works

- `installer/AuroraFD.iss`: the Inno Setup script (welcome, licence, notice,
  folder, shortcuts, Apps & Features entry, uninstaller).
- `installer/engine/Update.ps1`: the engine, in Windows PowerShell 5.1,
  started with `-ExecutionPolicy Bypass` (so the execution policy never
  blocks it) by Setup (`-Mode Install`) and by the shortcuts (`-Mode Update`,
  `-Mode Restore`). Its window shows the step, the details, an elapsed-time
  clock and a *Cancel* button. Cancel stops at once, except while the
  Visual Studio or Rust installer runs: stopping those halfway could damage
  them, so the engine stops right after that step.
- `installer/engine/settings.json`: the product name, the install folder's
  name, the label of the build, the repository and branch, the minimum
  tools, the disk space asked for. `folderName` must match `AppFolder` in
  the `.iss`.

An update, step by step:

1. Prerequisites (above), with consent.
2. **Code**: the fork's `main` in `<install>\build\src`, a Git clone that
   downloads commits up front and only the files of the checked-out commit
   (`--filter=tree:0`). The version number comes from the official release
   tags (`git describe`): their names are read with `git ls-remote` and set
   as local tags on the same commits; nothing else is downloaded from the
   upstream repository.
3. **Local patches** (below).
4. **Build**: `cargo build --release --locked -p aurora-viewer`, with
   `AURORA_PERSONAL_BUILD=Aurora Viewer +` and `CARGO_TARGET_DIR=<install>\build\target`
   (kept between updates, so later builds only redo what changed). On a PC
   with less than 8 GB of memory, and as a second attempt when the first
   fails without a compile error, it builds in the **low-memory mode**:
   `CARGO_PROFILE_RELEASE_LTO=off` and `-j 2`.
5. **Package**: the commit's own `scripts/package.ps1` into
   `<install>\viewer.new`, plus a `version.txt`.
6. **Install**: once the viewer is closed, `viewer.previous` becomes
   `viewer.older`, `viewer` becomes `viewer.previous` and `viewer.new`
   becomes `viewer`, by renaming folders: nothing is ever half copied. The
   new viewer must answer `--version`; then `viewer.older` is deleted.
   Otherwise every rename is undone (the failed build is set aside in an
   `old-*` folder, deleted at the next run) and the first error is shown. A
   failed or cancelled update leaves the installed viewer as it was.

`state.json` records what the installed viewer was built from (commit,
patches, label, engine): when nothing changed, the update says so.

### The in-viewer updater

The official viewer updates itself from the upstream releases
(`crates/aurora-updater`). A build from this fork has a version with Git
metadata (`0.9.7-beta.20261111.1+120.g27feea600`), which already never
updates; on top of that, `AURORA_PERSONAL_BUILD` turns the updater off
whatever the version (`updater::official_updates`), so an official package
can never replace this build. The window is titled with the product name
(`Aurora Viewer + <version> (Release)`), and Preferences > Language and
updates says to use the *Update* shortcut instead of the update settings.
`--version` keeps printing `Aurora Viewer <version>`, in every language:
the scripts read it.

### Crash reports

A personal build never publishes crash reports on the official Aurora
Viewer tracker (`crash/window.rs`, `report_mode`): its crashes are not the
Aurora Viewer team's, and a report could name the local changes. The crash
window still shows the report and its folder; nothing is sent.

## For the owner

### Local patches

Private changes that must never go to GitHub live on your PC only, as patch files in:

```
%APPDATA%\AuroraViewerPlus\local-patches\
```

Every `*.patch` there is applied after the download, in name order (prefix
them `01-`, `02-`… to choose the order), with `git apply --3way`, each on
its own local commit. A patch that no longer applies is **skipped** with a
warning at the end of the update, and the update goes on without it.
Nothing about patches is shown when the folder does not exist, and the
patches never leave the PC.

To make a patch from a commit of a local clone:

```bat
git format-patch -1 <commit> --stdout > "%APPDATA%\AuroraViewerPlus\local-patches\10-my-change.patch"
```

Changing, adding or removing a patch makes the next *Update* rebuild, even
when the fork's code did not change. `version.txt` in the viewer's folder
lists the patches applied and skipped.

### Building the installer

The installer is compiled by GitHub Actions on the fork
(`.github/workflows/installer.yml`, which only runs on
`ChromeHowl/Aurora-Viewer-1`; it does not touch `release.yml`):

- **By hand**: GitHub > Actions > *Installer* > *Run workflow* (branch
  `main`). The installer is in the run's artifacts
  (`AuroraViewerPlus-Setup-0.0.<run>.zip`).
- **As a release**: push a tag `installer-v<major>.<minor>.<patch>` to the
  fork (here its remote is `origin`):

  ```bat
  git tag installer-v1.0.0 origin/main
  git push origin installer-v1.0.0
  ```

  The workflow publishes `AuroraViewerPlus-Setup.exe` in the release
  *Installer 1.0.0* (not marked as the latest release). The fork is public:
  anyone can download it, and it only builds the public code.

The workflow first parses every script with Windows PowerShell 5.1
(`scripts/check-scripts.ps1`), then uses the runner's Inno Setup when it is
6.3 or newer, else installs the pinned version (`INNO_SETUP`, through its
Chocolatey package, which checks the installer's checksum), checks the
signature of `ISCC.exe`, and compiles. Only the job that publishes a
release can write to the repository.

**Locally, as a fallback**: install [Inno Setup 6](https://jrsoftware.org/isdl.php)
(6.3 or newer), then right-click `installer\AuroraFD.iss` > *Compile*, or:

```bat
"%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" installer\AuroraFD.iss
```

The setup program is written to `installer\output\AuroraViewerPlus-Setup.exe`
(ignored by Git).

### Logs and folders

| What | Where |
|---|---|
| Installed viewer | `%LOCALAPPDATA%\Programs\AuroraViewerPlus\viewer\` (`version.txt`: version, commit, patches) |
| Previous version | `...\AuroraViewerPlus\viewer.previous\` |
| Code and build cache | `...\AuroraViewerPlus\build\src\`, `...\build\target\` (several GB) |
| Update logs (10 newest) | `...\AuroraViewerPlus\logs\update-<date>.log`; Start menu > *Update logs* |
| Setup's own log | `%TEMP%\Setup Log <date>.txt` |
| Local patches | `%APPDATA%\AuroraViewerPlus\local-patches\` |
| Viewer settings, chat logs, cache | `%APPDATA%\Aurora\AuroraViewer\`, `%LOCALAPPDATA%\Aurora\AuroraViewer\` (shared with Aurora Viewer) |

### Troubleshooting

| The window says | What to do |
|---|---|
| *The code could not be downloaded* | Check the internet connection, then *Update* again. |
| *The Visual Studio C++ build tools are still missing* | The administrator prompt was refused, or the Visual Studio Installer failed. Install *Visual Studio 2022 Build Tools* with *Desktop development with C++* by hand, then *Update*. A restart of Windows can help. |
| *Rust ... is installed, but the code needs ... or newer* | `rustup update stable` failed (network, or a Rust installed by another tool). Run *Update* again; if it persists, run `rustup update` in a command prompt. |
| *The build failed* | It already tried the low-memory mode. Close other programs (games, browsers) and *Update* again. The end of the log shows the error. |
| *This version of the code does not build* | The fork's latest code has a compile error, or a local patch broke it: remove the newest patch, or wait for a fix. |
| *... is open. Close it* | Quit the viewer (log out first), then *Retry*. |
| A local patch was skipped | It no longer matches the code: make it again from your clone (above). |
| Nothing seems to happen for a long time | Look at the clock: it counts. During the last build step the PC is busy for 10-40 minutes without new output. |

Every error is also in the log, with the commands that ran and their output.
The *Open the log* button opens it.
