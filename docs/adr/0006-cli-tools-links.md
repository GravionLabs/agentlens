# 0006: Installing the command line tools from the application

- Status: Accepted
- Context: Feature "Installers also install the CLI and the MCP server" (#303), PBI #306 and its tasks #318 to #321; follows ADR 0003 (desktop application) and ADR 0004 (`mcp-setup --write`, the first exception to "read-only")

## Context

Every installer ships the CLI (`agentlens`) and the MCP server (`agentlens-mcp`) next to the application (#304). The `.deb` and `.rpm` packages put them in `/usr/bin` and the Windows installer adds its folder to `PATH`, but the macOS `.dmg` and the Linux AppImage have no install step: the programs are inside `AgentLens.app/Contents/MacOS` or inside the mounted image, and a user who wants them in a terminal or in an MCP client has to find them by hand. VS Code solves the same problem with *Install 'code' command in PATH*.

`AGENTS.md` says adapters never write and names `agentlens mcp-setup --write` as the only exception. An action that puts the two programs into a folder on `PATH` is a second write, outside the client data but on the user's machine, so it needs a decision.

## Options

1. **Documentation only.** Tell the user where the programs are and how to link them. Nothing is written, but most users never find the programs.
2. **An action in the application that links (or copies) the two programs into `~/.local/bin`**, on the user's click, with an undo.
3. **Write into `/usr/local/bin`** like VS Code. Needs administrator rights (a password prompt) and touches a system folder.
4. **Also edit the shell profile** so that `~/.local/bin` is on `PATH`. Convenient, but changes a file the user owns and maintains.

## Decision

- **Option 2**, accepted. The status bar of the application has *Install command line tools* and *Remove command line tools*, offered only on macOS and in an AppImage (not for the `.deb` and `.rpm` packages, not in the browser demo; on Windows the same actions edit the user `PATH` instead, see ADR 0008). The backend refuses the commands where the action is not offered, so the rule does not rest on the interface.
- **What is written:** exactly `~/.local/bin/agentlens` and `~/.local/bin/agentlens-mcp`, and the folder `~/.local/bin` when it is missing. Nothing else, and only on a click; looking at the state writes nothing.
- **Links on macOS, copies in an AppImage.** On macOS the links point into `AgentLens.app/Contents/MacOS`, which an update keeps, so they follow every update. The application refuses to link from a disk image (`/Volumes/...`) or from an App Translocation path and asks the user to move it to `/Applications` first. An AppImage has no stable path for its programs (the mount point `/tmp/.mount_<random>` changes on every start and disappears on exit, and `$APPIMAGE` starts the application, not the CLI), so the tools are copies with mode 0755. A copy does not follow an update; clicking *Install* again replaces it.
- **Never overwrite a foreign file.** What counts as ours: a link to the program next to the running application; an old link of ours (a link to a program of the same name in an AgentLens `.app` bundle, in an AppImage mount point or in the program folder of the application, dangling or not); an exact copy; an old copy, recognized by a fixed marker byte string that every `agentlens` and `agentlens-mcp` binary carries (the file is read, never run, and no record file is written). Anything else at those paths (the user's own script, a link of theirs to elsewhere, a directory) is named in the result and left alone, by *Install* and by *Remove*.
- **Atomic replacement.** A link or copy is written under a temporary name in `~/.local/bin` and renamed over the path, so a failed repair keeps the working tool; the temporary file is removed on failure.
- **Never edit a shell profile** (option 4 rejected). When `~/.local/bin` is not on the `PATH` of the application, the result shows the line to add for zsh and bash. That `PATH` can differ from a shell's (an application started from the Finder does not read the profile); the documentation says so.
- **Undo:** *Remove command line tools* deletes our links and copies (old ones too) and nothing else; the programs inside the application stay.
- **`AGENTS.md`:** the read-only rule names this action as the second exception, next to `mcp-setup --write`.

## Consequences

- macOS and AppImage users get the CLI and the MCP server on `PATH` with one click, and `agentlens mcp-setup` finds the server there.
- A second write exception exists, limited to two named files in `~/.local/bin`, opt-in, visible in the result and reversible.
- AppImage users have to click *Install* again after an update; until then the copies are the old version. The result says so.
- An `agentlens` built from source and installed into `~/.local/bin` (for example with `cargo install --root ~/.local`) carries the marker too and counts as an old copy of ours: *Install* replaces it with the application's version and *Remove* deletes it. It is an AgentLens binary of the same name, so this is accepted.
- The marker is a contract between versions: it must never change, or copies installed by older versions become foreign files.
