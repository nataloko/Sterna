# Development commands

The workspace provides `./dev setup`, `./dev build`, `./dev test`, `./dev run`,
`./dev package`, and `./dev clean`. Use `./dev help` for the available commands.
Use `./dev --dry-run build` to inspect the build command.

These commands require the adjacent `dev-setup` repository and Python 3.11 or
later on the host. `DEV_SETUP_ROOT` can select another location for that
repository. The native commands in the project README remain available.

Read `DEVELOPMENT.md` in the workspace root for the shared conventions.
The command definitions are in `dev-setup/projects.json`.

The wrapper selects the Distrobox environment. Generated compiler output goes
to `~/.cache/dev/<project>-<checkout-id>/<environment>/`. `XDG_CACHE_HOME` can
select another cache location. Separate checkouts have separate build output.
`./dev clean` removes only output with a matching ownership marker. It does not
remove source, virtual environments, samples, keys, or releases.

## Qt and Rust

Run `./dev setup` before the first build. The Rust version is in
`rust-toolchain.toml`. The CMake presets are in `shell/CMakePresets.json`.
The wrapper selects `linux-debug` and an external build directory. Direct
preset commands use the ignored `.cache/cmake/` directory in this repository.
Keep Cargo output inside its CMake build directory.

Use `linux-release` for release development and `linux-asan` for AddressSanitizer.
The `windows-release` preset requires the Fedora MinGW toolchain and the
`x86_64-pc-windows-gnu` Rust target. These presets do not replace the prescribed
release packaging environment.

Tests that require serial devices, servers, or Windows need their documented
prerequisites. A successful local build does not verify those tests.

`./dev package` builds the AppImage in the pinned manylinux container. It keeps
the release toolchain outside Projects. It copies the result and checksums to
`Releases/<project>/<revision>-<timestamp>-<id>/` in the workspace. It does not publish
or sign an update. The Windows installer retains its existing documented workflow.
