# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Cpu color (1d0b97b)

### Changed
- Layout adjustments (9e66a36)

## [0.2.1] - 2025-04-07

### Fixed
- Fix graph total alignment (bfba067, 706747d)
- Fix warnings (78e5886)
- Fix chart white line (d6447db)
- Make Node column expand to fill remaining space (previous edit)

## [0.2.0] - 2025-04-07

### Fixed
- Fix clippy issues (f5540d2)
- Fix warnings (2cc1458)
- Fix node name extraction (9f6673b)

### Added
- Running nodes statistics (c646694)
- Natural ordering for nodes (a29427d)
- Split status display (185340b)
- Human-readable update interval values (076342a)
- Configurable update interval with sensible discrete values (aa67eec, 5bb30cf)
- Mouse scroll support (38a69e2)
- Node filtering (fcc9c19, 986bbcc)
- Scroll functionality (b3bfe65)

### Changed
- Alignment adjustments (09f98ba)
- Proper spacing adjustments (8520ce6, 0a67432)

### Removed
- Remove node mentions (1da50f2)

## [0.1.7] - 2025-04-07

### Fixed
- Fix server list display (529f10a)
- Fix log path inference with `--path` argument (4d3fd80)

## [0.1.6] - 2025-04-06

### Added
- Add `--log-path` argument (5fd74bb)

## [0.1.5] - 2025-04-06

### Changed
- Handle home folder for default path resolution (685fc72)

### Removed
- Remove comments (9bcba79)

## [0.1.4] - 2025-04-06

### Fixed
- Fix minor errors (4a043b4)

### Changed
- Improved path handling (b7cc649)

### Removed
- Remove error logs (855e864)

### Docs
- Update readme (e284256)

## [0.1.3] - 2025-04-06

### Merged
- Merge branch 'develop' (529b0eb)

## [0.1.2] - 2025-04-06

### Changed
- Performance improvements (2c9a03f, 31e352a)
- Improved column graph display (1b95793)
- Improved header graph display (d02ecbd, 12b0b4b)
- Added column colors (2da20e8)
- Update project description in `Cargo.toml` (e86a4b3)

### Docs
- Update Readme (a2e64ad)
- Update screenshot (0307567)

## [0.1.1] - 2025-04-06

### Fixed
- Fix warnings (223352b, c1c66e7, fbaac93)
- Fix alignment issues (1adf230, df23313)
- Fix gauge background color (d2af6bc)

### Added
- Better stats in the header (6818711, 0dcb99a)
- Total in/out graph (728ca05)
- Background color (9ebebfd)
- Improved gauges (871a6fa, 35243de)
- Added crate description (1a2ccd2)
- Add license (cd47bd0)
- Add screenshot (19d78fe)
- Add readme (8672c44)

### Changed
- Renamed project to "antop" (dd91b91)
- Restore margin (5c23c29)
- Align error column (8987df0)
- Put status column at the end (fe94864)
- Style adjustments (b47d7b7)

### Docs
- Update Readme (5600a56, c5b403d)

### Removed
- Remove unused gap (0e03b98)
- Cleanup comments (2a51541)

## [Pre-0.1.1] - 2025-04-04

*(Commits before the first version bump to 0.1.1)*

### Added
- Split peers into routing, status disappears (b861e81)
- Units display (73018d5)
- Text color options (de9572a, ad6f937)
- Both graphs are back (39058ad)
- Line chart restored (9c7b2d6)
- Speed chart restored (b3a4a09)
- List view instead of table (4826a33)
- Line chart attempt (a8b90d3)
- Error column (7504ad1, 2af8a77)
- Speed display (97b5de0)
- Node names display (ae9f1b7)
- CLI parameter handling (4a0d67f)
- Initial commit (5ed43c1)

### Fixed
- Fix status display (aa452b8)
- Trying to fix display issues (923f6f6)
- Sparkline fail attempt (d56c00c)

### Changed
- Alignment adjustments (5828ecd, 499c8ab, dd0ebea, a9d0004, f1935c1, 7825c0e, f455cf2)
- Better spacing (12116a4, c8bd683, a8cb51e, 4972e18)
- Better graphs (73d858c, 3ec0e16, de0caf5)
- Better header for graphs (26b30b7)
- Split UI and graph inside table (622dde6)
- Styling adjustments (ffee067, b6639f3, 473d7f5, a1901c6)
- Split list text (addbd12)
- Better UI (a52c13b, 88f0670)
- Better number formatting (ae6b942)
- Better units display (1657edf)
- Split code structure (dca103e)
- Running and working state (c89f71b)
- Running but fails state (e7fea0a)

### Other
- Yeah (6d827f6) 