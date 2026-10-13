# Updates of the desktop application

The application updates itself from the latest GitHub release. It checks only when the user clicks
**Check for updates** (a link in the status bar at the bottom, next to the running version); it makes
no other network call. If a newer version exists, the status bar shows it with its release notes and
**Install and restart** downloads, verifies and installs it.

If something goes wrong, the header says which step failed and shows the error text next to it
(select it to copy):

- **Update check failed**: the check itself did not work.
- **Update failed**: the update was found but could not be installed. It stays available, so
  **Install and restart** can be clicked again.

Common causes: no network connection; the release has no package for this platform (for example a
release without an installer for your operating system or architecture); the download was removed
from the release; the signature does not match the public key.

## Signing

Update packages are signed with a Tauri updater key (this is separate from operating system code
signing, which the installers do not have yet):

- the public key is in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`)
- the private key and its password are the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the release workflow passes them to `tauri-action`
- `src-tauri/tauri.bundle.conf.json` turns on `createUpdaterArtifacts`; only the release workflow
  uses it, so local builds do not need the key
- the endpoint is `https://github.com/GravionLabs/agentlens/releases/latest/download/latest.json`,
  which `tauri-action` uploads with every release

**Keep a backup of the private key and its password.** If they are lost, installed applications can
never update again and users must install a new version by hand.

To make a new key: `pnpm tauri signer generate -w <file>`, then replace the public key in
`tauri.conf.json` and both secrets.

## Limits

- Linux `.deb` and `.rpm` installs are updated through the package manager; the in-app updater
  replaces AppImage, Windows and macOS installs.
- Versions before the first release with an updater key (0.4.0 and older) cannot update themselves.
