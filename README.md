# Shard Launcher

A free, open-source launcher for **Minecraft: Java Edition**, built with Tauri, Rust and React.
Windows first; Linux and macOS are planned.

**English** · [Русский](README.ru.md)

## Download

Get the latest installer or portable zip from [Releases](https://github.com/Asd1if/Shard-Launcher/releases).
The launcher updates itself when a new release comes out.

## Features

**Instances**
- Vanilla, Fabric, Quilt, Forge and NeoForge, each instance with its own folder, Java, memory and arguments
- Automatic Java download, crash analysis with suggested fixes
- Snapshots with one-click rollback, taken automatically before updates
- World backups, screenshots, play time statistics
- Copy controls, sound and graphics settings between instances
- One-click optimization mods (Sodium/Embeddium, Lithium, FerriteCore and more)
- Import from Prism Launcher, MultiMC, Modrinth App, CurseForge and `.minecraft`

**Mods and modpacks**
- Modrinth and CurseForge search with filters by version, loader, category and side
- Mod pages with description, gallery, versions and dependencies
- Install, update, enable/disable; missing dependencies check
- Install, update and export modpacks (`.mrpack`, CurseForge `.zip`)

**Servers**
- Create a server from an instance or from Paper, Purpur, Fabric, Forge and other cores
- Console with colors, search and command autocomplete
- Players, operators, whitelist and bans; file manager with config highlighting
- Auto-restart, scheduled restarts, backups, CPU and memory graphs
- Play with friends over LAN or a tunnel, add the server to the game's list in one click

**Everything else**
- Microsoft sign-in and offline profiles, skins and capes with a 3D preview
- Themes (including System and Auto), animated backgrounds or your own image
- Minecraft news on the home page, Discord status, English and Russian interface

## Privacy

- You sign in only to your own Microsoft account (`XboxLive.signin` and `offline_access` scopes).
- Tokens are stored locally in the operating system's credential store and sent only to Microsoft, Xbox Live and Minecraft services.
- The launcher does not modify the game or bypass ownership checks. Offline profiles never call Mojang services.

## CurseForge

Release builds do not include a CurseForge API key. To browse CurseForge, get a free key at
[console.curseforge.com](https://console.curseforge.com/) and paste it in **Settings → CurseForge**.
Modrinth works without a key.

## Building from source

Requirements: Node.js 20+, Rust (stable) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

On Windows, `start.bat` runs the launcher in development mode and `build.bat` builds the installer and a portable zip into `release/`.

Optional build-time files in `src-tauri/` (or environment variables):

| File | Variable | Purpose |
|---|---|---|
| `msa-client-id.txt` | `SHARD_MSA_CLIENT_ID` | Azure app ID for Microsoft sign-in |
| `curseforge-api-key.txt` | `SHARD_CURSEFORGE_API_KEY` | CurseForge key built into a private build (never commit it) |
| `discord-client-id.txt` | `SHARD_DISCORD_CLIENT_ID` | Discord app ID for the status |
| `update-repo.txt` | `SHARD_UPDATE_REPO` | GitHub repo the launcher checks for updates |

Set `SHARD_PUBLIC_BUILD=1` to build without the CurseForge key even if the file exists.

## Releasing

Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, then push a tag:

```bash
git tag v0.5.1
git push origin v0.5.1
```

GitHub Actions builds the installer and the portable zip and publishes the release.

## License

[MIT](LICENSE) © Asd1if. Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.
