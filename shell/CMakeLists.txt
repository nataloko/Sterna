cmake_minimum_required(VERSION 3.21)
project(sterna-shell VERSION 0.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Debug CACHE STRING "" FORCE)
endif()

find_package(Qt6 6.4 REQUIRED COMPONENTS Widgets)

# --- the Rust core -----------------------------------------------------------
#
# Cargo is driven from here so one `cmake --build` produces a runnable shell,
# and its target directory is *inside the CMake build tree* rather than the
# workspace's own `crates/target`.
#
# That is not tidiness. The Qt work happens in the `sterna-fedora` container
# (Qt 6.11.1, matching the desktop) while the core's tests run in the Ubuntu
# one, and the two share a home directory — so a shared target directory would
# hand a library linked against one distribution's glibc to a binary built on
# the other. It works in one direction and fails confusingly in the other. A
# build tree per container costs one extra compile of the core and removes the
# question.

set(TT_CORE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../crates")
set(TT_CARGO_DIR "${CMAKE_BINARY_DIR}/cargo")

find_program(CARGO_EXECUTABLE cargo
    HINTS "$ENV{HOME}/.cargo/bin" "$ENV{CARGO_HOME}/bin")
if(NOT CARGO_EXECUTABLE)
    message(FATAL_ERROR
        "cargo not found. It is on PATH only for login shells in the dev "
        "container — export $HOME/.cargo/bin, or pass -DCARGO_EXECUTABLE=.")
endif()

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(TT_CARGO_FLAGS "")
    set(TT_CARGO_SUBDIR debug)
else()
    set(TT_CARGO_FLAGS --release)
    set(TT_CARGO_SUBDIR release)
endif()

set(TT_CORE_LIB "${TT_CARGO_DIR}/${TT_CARGO_SUBDIR}/${CMAKE_SHARED_LIBRARY_PREFIX}sterna${CMAKE_SHARED_LIBRARY_SUFFIX}")

# Always run: cargo does its own dependency tracking and is a no-op when
# nothing changed, which is cheaper and more correct than globbing the
# workspace's sources into a DEPENDS list that silently misses a new file.
add_custom_target(tt_core ALL
    BYPRODUCTS "${TT_CORE_LIB}"
    COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${TT_CARGO_DIR}"
            "${CARGO_EXECUTABLE}" build -p tt-ffi ${TT_CARGO_FLAGS}
    WORKING_DIRECTORY "${TT_CORE_ROOT}"
    COMMENT "cargo build -p tt-ffi"
    USES_TERMINAL VERBATIM)

add_library(sterna_core SHARED IMPORTED GLOBAL)
set_target_properties(sterna_core PROPERTIES
    IMPORTED_LOCATION "${TT_CORE_LIB}"
    INTERFACE_INCLUDE_DIRECTORIES "${TT_CORE_ROOT}/tt-ffi/include")

# --- the shell ---------------------------------------------------------------

add_library(sterna_shell STATIC
    src/MainWindow.cpp
    src/TerminalView.cpp
    src/Session.cpp
    src/Macro.cpp
    src/Theme.cpp
    src/SerialDialog.cpp
    src/SettingsDialog.cpp
    src/SshDialog.cpp
    src/SshPrompts.cpp
    src/TelnetDialog.cpp
    src/XferDialog.cpp)

add_dependencies(sterna_shell tt_core)
target_link_libraries(sterna_shell PUBLIC Qt6::Widgets sterna_core)
target_include_directories(sterna_shell PUBLIC src)
target_compile_options(sterna_shell PRIVATE -Wall -Wextra)

add_executable(sterna
    src/main.cpp
    resources/branding.qrc)
if(WIN32)
    enable_language(RC)
    target_sources(sterna PRIVATE resources/sterna.rc)
endif()
target_link_libraries(sterna PRIVATE sterna_shell)
target_compile_options(sterna PRIVATE -Wall -Wextra)

# --- tests -------------------------------------------------------------------
#
# The painter is the one part of the shell that nothing else can check: the
# differential suite proves the *grid* matches Tera Term, and stops at the
# point where cells become pixels. `QWidget::grab()` re-renders offscreen,
# which is both what we want to test and the only screenshot that works here —
# GNOME's screenshot D-Bus API is locked down and `QScreen::grabWindow` returns
# blank under xcb and null under Wayland.

enable_testing()

add_executable(render_test tests/render_test.cpp)
target_link_libraries(render_test PRIVATE sterna_shell)
target_compile_options(render_test PRIVATE -Wall -Wextra)
add_test(NAME render COMMAND render_test)

# The only thing that exercises the *window's* event loop against a real
# server. Skips loudly without TT_SSH_HOST, so it is safe to leave in ctest.
add_executable(ssh_test tests/ssh_test.cpp)
target_link_libraries(ssh_test PRIVATE sterna_shell)
target_compile_options(ssh_test PRIVATE -Wall -Wextra)
add_test(NAME ssh COMMAND ssh_test)

add_executable(telnet_test tests/telnet_test.cpp)
target_link_libraries(telnet_test PRIVATE sterna_shell)
target_compile_options(telnet_test PRIVATE -Wall -Wextra)
add_test(NAME telnet COMMAND telnet_test)

# The only connection test here that skips nothing — a pty needs no server and
# no hardware — so it is what actually runs in CI.
add_executable(pty_test tests/pty_test.cpp)
target_link_libraries(pty_test PRIVATE sterna_shell)
target_compile_options(pty_test PRIVATE -Wall -Wextra)
add_test(NAME pty COMMAND pty_test)

# A file transfer, driven by the window's event loop rather than by a pump
# loop. It needs `lrzsz` for the transfer itself and skips loudly without it;
# the cancel case and the dialogs need nothing.
add_executable(xfer_test tests/xfer_test.cpp)
target_link_libraries(xfer_test PRIVATE sterna_shell)
target_compile_options(xfer_test PRIVATE -Wall -Wextra)
add_test(NAME xfer COMMAND xfer_test)

# A TTL macro against the window's event loop rather than a pump loop, which
# is where a re-entered modal dialog or an unserviced notifier shows up. Needs
# nothing: the one connected case forks `/bin/sh`.
add_executable(macro_test tests/macro_test.cpp)
target_link_libraries(macro_test PRIVATE sterna_shell)
target_compile_options(macro_test PRIVATE -Wall -Wextra)
add_test(NAME macro COMMAND macro_test)

# A Tera Term command line, from `argv` to a window that has connected. Needs
# nothing either: the one case that connects does so to a listening socket it
# opens itself.
add_executable(cmdline_test tests/cmdline_test.cpp)
target_link_libraries(cmdline_test PRIVATE sterna_shell)
target_compile_options(cmdline_test PRIVATE -Wall -Wextra)
add_test(NAME cmdline COMMAND cmdline_test)

# --- the performance gate ----------------------------------------------------
#
# NOT a test, and deliberately not in `ctest`: it takes tens of seconds, its
# numbers depend on the machine, and a gate that fails because somebody started
# a compile is a gate people learn to ignore. `bench/bench.py` is what compares
# it against a baseline.
#
# It runs the corpus generator out of `tt-bench` so the bytes crossing the pty
# are the same ones the engine is measured on alone — the difference between
# the two numbers is what the window costs. Its own cargo target, because a
# release binary is wanted here even when the shell is built Debug, and because
# nothing else in this build needs it.

set(TT_BENCH_EXE "${TT_CARGO_DIR}/release/tt-bench")

add_custom_target(tt_bench_emitter
    BYPRODUCTS "${TT_BENCH_EXE}"
    COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${TT_CARGO_DIR}"
            "${CARGO_EXECUTABLE}" build -p tt-bench --release
    WORKING_DIRECTORY "${TT_CORE_ROOT}"
    COMMENT "cargo build -p tt-bench --release (the bench corpus generator)"
    USES_TERMINAL VERBATIM)

# `EXCLUDE_FROM_ALL`, so an ordinary build — and CI's — does not compile it or
# the release-profile cargo target it drags in. Build it by name:
#
#   cmake --build build-release --target bench_shell
add_executable(bench_shell EXCLUDE_FROM_ALL tests/bench_shell.cpp)
add_dependencies(bench_shell tt_bench_emitter)
target_link_libraries(bench_shell PRIVATE sterna_shell)
target_compile_options(bench_shell PRIVATE -Wall -Wextra)
target_compile_definitions(bench_shell PRIVATE TT_BENCH_EXE="${TT_BENCH_EXE}")

# So the build tree runs without LD_LIBRARY_PATH, and so an installed one finds
# the core next to itself.
#
# The install rpath is `$ORIGIN/../lib` and not a path, because the whole point
# of the AppImage is that it has no idea where it will be mounted. Cargo gives a
# cdylib no `DT_SONAME` (see `tt-ffi/build.rs`), so what the binary records is
# whatever path the linker was handed — which out of tree is *relative*, and ran
# only from the build directory until that was fixed.
set_target_properties(sterna render_test ssh_test telnet_test pty_test cmdline_test
                      bench_shell PROPERTIES
    BUILD_RPATH "${TT_CARGO_DIR}/${TT_CARGO_SUBDIR}"
    INSTALL_RPATH "$ORIGIN/../lib")

# --- install -----------------------------------------------------------------
#
# `lib` and `bin` spelled out rather than taken from GNUInstallDirs: this
# prefix is an AppDir, not a system one, and on Fedora GNUInstallDirs would say
# `lib64` while everything downstream — linuxdeploy, the AppRun it writes —
# expects `usr/lib`. A packaging layout is not a distribution layout.
install(TARGETS sterna RUNTIME DESTINATION bin)
install(FILES "${TT_CORE_LIB}" DESTINATION lib)
