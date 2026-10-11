# Changelog

All notable changes to GravionLabs/agentlens are documented here.
Versioning follows [Semantic Versioning](https://semver.org/).

---

## [0.6.0] — 2026-10-11

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


