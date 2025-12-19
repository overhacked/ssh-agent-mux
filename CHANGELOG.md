# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-12-19

### Added
- Environment variables support by @domwst
- Add checks for missing SSH_AUTH_SOCK during --install-config

### Changed
- Improve git-cliff CHANGELOG format
- Use path feature of shellexpand
- Make hard-coded crate binary name compile-time dynamic
- Update README.md config example

### Dependencies
- Bump Justintime50/homebrew-releaser from 2 to 3 by @dependabot[bot]
- Bump actions/checkout from 4 to 6 by @dependabot[bot]
- Bump service-manager from 0.8.0 to 0.9.0 by @dependabot[bot]
- Update MSRV to 1.83.0
- Bump flexi_logger from 0.30.1 to 0.31.7 by @dependabot[bot]
- Bump tokio from 1.45.0 to 1.48.0 by @dependabot[bot]
- Bump tempfile from 3.20.0 to 3.23.0 by @dependabot[bot]
- Bump color-eyre from 0.6.3 to 0.6.5 by @dependabot[bot]
- Bump serde from 1.0.219 to 1.0.228 by @dependabot[bot]
- Bump toml from 0.8.22 to 0.9.8 by @dependabot[bot]
- Bump duct from 1.0.0 to 1.1.1 by @dependabot[bot]
- Bump log from 0.4.27 to 0.4.29 by @dependabot[bot]
- Bump tempfile from 3.19.1 to 3.20.0 by @dependabot[bot]
- Bump tokio from 1.44.2 to 1.45.0 by @dependabot[bot]
- Bump duct from 0.13.7 to 1.0.0 by @dependabot[bot]
- Bump toml from 0.8.21 to 0.8.22 by @dependabot[bot]

### Fixed
- Create config directory when bootstrapping config file

## New Contributors
- @domwst made their first contribution
## [0.1.6] - 2025-04-28

Release v0.1.6

- Add CI workflow
- Add better error reporting to integration tests
- Add --log-file option
- Bump toml from 0.8.20 to 0.8.21 by @dependabot[bot]
- Cargo fmt
- Clean up quoting in homebrew formula generation
- Temporarily disable cross qemu tests
- Update MSRV to 1.81.0
* @overhacked made their first contribution in [#2](https://github.com/overhacked/ssh-agent-mux/pull/2)
* @dependabot[bot] made their first contribution

### Added
- Add CI workflow
- Add better error reporting to integration tests
- Add --log-file option

### Changed
- Cargo fmt
- Clean up quoting in homebrew formula generation

### Dependencies
- Bump toml from 0.8.20 to 0.8.21 by @dependabot[bot]

### Fixed
- Temporarily disable cross qemu tests
- Update MSRV to 1.81.0

## New Contributors
- @dependabot[bot] made their first contribution
## [0.1.5] - 2025-04-27

Release v0.1.5

Release v0.1.5
- Switch homebrew-releaser CI back to upstream
- Switch fork of homebrew-releaser to main branch
- Update Homebrew tap repository name
- Fix line length in release workflow
* @overhacked made their first contribution
- Switch homebrew-releaser CI back to upstream
- Switch fork of homebrew-releaser to main branch
- Update Homebrew tap repository name
- Fix line length in release workflow
* @overhacked made their first contribution

### Changed
- Switch homebrew-releaser CI back to upstream
- Switch fork of homebrew-releaser to main branch
- Update Homebrew tap repository name

### Fixed
- Fix line length in release workflow

## [0.1.4] - 2025-04-24

Release v0.1.4

Release v0.1.4
Release v0.1.4
- Fix homebrew-releaser workflow
- Test homebrew-releaser local changes
- Test homebrew-releaser local changes

### Changed
- Test homebrew-releaser local changes

### Fixed
- Fix homebrew-releaser workflow

## [0.1.3] - 2025-04-23

Release v0.1.3

- Fix homebrew-tap workflow step

### Added
- Add homebrew to release CI
- Add configuration reloading on SIGHUP
- Add some trace logging
- Add integration test
- Add --install-config option
- Automatic configuration file generation
- Add dependabot configuration
- Suggest how to configure on service-unsupported platforms

### Changed
- Move test harness into separate module
- Cargo fmt
- Cargo update
- Dependabot only manages upstream dependencies
- Move main and modules to a separate bin directory
- Extract logging module

### Fixed
- Fix homebrew-tap workflow step

## [0.1.1] - 2025-04-19

Release v0.1.1

- Prepare for v0.1.1
- Service management (as described in README)
- Add color-eyre and improve some error reporting

### Added
- Service management (as described in README)
- Add color-eyre and improve some error reporting

## [0.1.0] - 2025-04-19

`ssh-agent-mux` combines multiple agents' keys into a single agent, allowing
you to configure an SSH client just once. Provide all "upstream" SSH agents'
`SSH_AUTH_SOCK` paths in the `ssh-agent-mux` configuration and run
`ssh-agent-mux` via your login scripts or OS's user service manager. Point your
SSH configuration at `ssh-agent-mux`'s socket, and it will offer all available
public keys from upstream agents as available for authentication.

### Added
- Add release workflow and shell script

[0.2.0]: https://github.com/overhacked/ssh-agent-mux/compare/v0.1.6..v0.2.0
[0.1.6]: https://github.com/overhacked/ssh-agent-mux/compare/v0.1.5..v0.1.6
[0.1.5]: https://github.com/overhacked/ssh-agent-mux/compare/v0.1.4..v0.1.5
[0.1.4]: https://github.com/overhacked/ssh-agent-mux/compare/v0.1.3..v0.1.4
[0.1.3]: https://github.com/overhacked/ssh-agent-mux/compare/v0.1.1..v0.1.3
[0.1.1]: https://github.com/overhacked/ssh-agent-mux/compare/v0.1.0..v0.1.1
[0.1.0]: https://github.com/overhacked/ssh-agent-mux/compare/v0.0.0..v0.1.0

<!-- generated by git-cliff -->
