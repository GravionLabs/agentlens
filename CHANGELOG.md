# Changelog

All notable changes to GravionLabs/agentlens are documented here.
Versioning follows [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

### Bug Fixes
- Build PATH with the platform separator in the cli_tools tests


## [0.10.2] — 2026-10-13

### Bug Fixes
- Tell a failed update installation from a failed check (#300) (#308)
- Tell a failed update installation from a failed check (#300)
- Harden the update failure states (#300)
- Address review findings for rounded token counts (#301)
- Address review findings for installer sidecars (#304)
- NSIS PATH hook skips near-limit values and works in every install mode (#315)
- Compile the PATH hooks for the uninstaller and guard machine PATH edits (#305)
- Recognize old copies and write the command line tools atomically (#306)
- **app**: Show each command line tools error once and label old copies (#306)
- **docs**: Describe the update of AppImage copies and propose ADR 0006 (#306)


### Documentation
- The installers include the CLI and the MCP server (#314)
- Describe the Windows installer PATH entry (#317)
- Describe the install command line tools action (#321)
- Accept ADR 0006 and name the second write exception in AGENTS.md (#306) (#327)


### Features
- Magnifier icons instead of A+ and A- for the zoom buttons (#302)
- Show token counts rounded on request (#301) (#323)
- Rounded token format and persisted setting (#309)
- Token format switch in the status bar (#311)
- **cli**: --rounded shows token counts as 1.55 M in text tables (#322)
- Ship the CLI and MCP server in every installer (#304) (#324)
- List the CLI and MCP server in bundle.externalBin (#312)
- Windows installer puts the install directory on the user PATH (#305) (#325)
- NSIS installer adds the install directory to PATH (#315)
- MSI WiX fragment adds the install directory to the user PATH (#316)
- Install the command line tools from the application (#306) (#326)
- **core**: Install and remove the command line tool links (#318)
- **app**: Commands and types for the command line tools (#319)
- **app**: Install command line tools action in the status bar (#320)


### Refactoring
- Route all token values through tokens() (#310)


## [0.10.0] — 2026-10-12

### Documentation
- Update CHANGELOG.md for v0.10.0


### Features
- Put every finding in proportion: share of all tokens and a context-size finding (#283)
- A finding for skills that are listed but only loaded through their files (#284)
- Say when an agent, command or skill names a skill that looks unused (#286)
- A finding for MCP servers whose static context is large and that are rarely called (#287)
- Copilot CLI reads session.usage_record when the session store has no rows (#280)
- Explain why the client's skill listing exceeds the computed baseline (#285)


## [0.9.1] — 2026-10-12

### Bug Fixes
- The event types of newer Copilot CLI versions are known (no warning)


## [0.9.0] — 2026-10-11

### Documentation
- A compatibility page that says what is stable (#274)
- Update CHANGELOG.md for v0.9.0


### Features
- The schema version in every machine-readable result (#273)


## [0.8.1] — 2026-10-11

### Bug Fixes
- Count typed skill commands as user triggers (#269)


## [0.8.0] — 2026-10-11

### Documentation
- Update CHANGELOG.md for v0.8.0


### Features
- A download section on the site (#256)
- A measured flag on every token count that can be an estimate (#246)
- Count a skill as used when its files are read or its scripts run (#260)
- An advice analysis with findings and their evidence (#261)
- An MCP prompt and a glossary resource for agents (#262)


## [0.7.0] — 2026-10-11

### Documentation
- ADR 0004 for the MCP server (#204)
- Accept the ADR
- ADR 0005 for the documentation site (#214)
- Accept the ADR
- The user guide (#216)
- US English spelling (license, canceled)
- One glossary for the application and the guide (#217)
- A shorter README that points to the guide
- Describe the documentation site in AGENTS.md
- Update CHANGELOG.md for v0.7.0


### Features
- Documentation site generator reading docs/ in place (#215)
- Buttons after the Helix button; check for updates as a link
- Agentlens-mcp binary with read-only analysis tools (#206, #207)
- Ship agentlens-mcp in the release archives and document it (#208, #209)
- Agentlens mcp-setup registers the MCP server in the clients (#211)
- The application as a live demo (#218)


### Refactoring
- Share the analysis entry points between the app and the new server (#205)


## [0.6.0] — 2026-10-11

### Documentation
- Update CHANGELOG.md for v0.6.0


### Features
- Status bar with the active version and a check for updates button (#224)
- Scalable interface and a readable bar chart (#226)
- Application command that exports a view to a file (#231)
- Export menu in the page header of every view (#232)
- CSV export for every analysis in the application (#234, #235)


### Refactoring
- Move the HTML report rendering to agentlens-core (#230)
- Move the CSV rows of every analysis to agentlens-core (#233)


## [0.5.0] — 2026-10-11

### Bug Fixes
- Count a session under two project directories once (#186)
- Refresh the overview sample after the OpenCode note changed
- Number the release 0.5.0 with a commit message bump


### Documentation
- Exact token counts (#184)
- Update CHANGELOG.md for v0.5.0


### Features
- The application on real data: faster loading, refresh, readable tables (#188)
- Check for updates and install them from the application
- Application layout with a sidebar, a glossary and help for the terms (#194)
- Count_tokens client with an on-disk cache (#182)
- --exact flag (#183)
- Read the legacy OpenCode JSON storage (#23)
- Import Copilot CLI OpenTelemetry traces (#140)
- Document and test the VS Code Copilot OTel export (#140)


## [0.4.0] — 2026-10-10


