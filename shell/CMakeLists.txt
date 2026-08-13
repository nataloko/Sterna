cmake_minimum_required(VERSION 3.21)
project(sterna-shell VERSION 0.1.5 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Debug CACHE STRING "" FORCE)
endif()

# PrintSupport is a separate qtbase module, present in Fedora's native package
# and in `mingw64-qt6-qtbase` alike. It is what File > Print and the media copy
# sequences reach; without it a terminal that a host asks to print has nowhere
# to send the page.
find_package(Qt6 6.4 REQUIRED COMPONENTS Widgets PrintSupport Network)

# `-Wall -Wextra` is GCC and Clang's spelling. MSVC takes `-` as a switch lead
# too, so `-Wall` reaches it and means "every warning including the ones the
# standard library trips over"; `-Wextra` is not a switch it has at all.
if(MSVC)
    set(TT_WARNINGS /W4)
else()
    set(TT_WARNINGS -Wall -Wextra)
endif()

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

# Cross-compiling needs `--target`, and passing it moves cargo's output down a
# directory named after the triple. A *native* build must not be given one:
# `--target` even for the host disables the `[build]` config the workspace may
# rely on and rebuilds proc macros, so the two cases genuinely differ. The
# default is the MinGW triple because that is what a Fedora `mingw64-cmake`
# build is, and the cache entry is what a different one overrides.
if(NOT DEFINED TT_CARGO_TARGET)
    if(CMAKE_CROSSCOMPILING AND WIN32)
        set(TT_CARGO_TARGET "x86_64-pc-windows-gnu")
    else()
        set(TT_CARGO_TARGET "")
    endif()
endif()
set(TT_CARGO_TARGET "${TT_CARGO_TARGET}" CACHE STRING
    "Rust target triple for the core, empty for the host's own")

if(TT_CARGO_TARGET)
    set(TT_CARGO_TARGET_FLAGS --target "${TT_CARGO_TARGET}")
    set(TT_CARGO_OUT "${TT_CARGO_DIR}/${TT_CARGO_TARGET}/${TT_CARGO_SUBDIR}")
else()
    set(TT_CARGO_TARGET_FLAGS "")
    set(TT_CARGO_OUT "${TT_CARGO_DIR}/${TT_CARGO_SUBDIR}")
endif()

# Cargo's own names, not CMake's. `CMAKE_SHARED_LIBRARY_PREFIX` is `lib` for a
# MinGW target, and cargo emits `sterna.dll` there — taking CMake's answer
# looks right and points at a file that is never written. The import library
# is the half a Windows link actually resolves against, and MSVC and MinGW
# spell it differently.
if(WIN32)
    set(TT_CORE_LIB "${TT_CARGO_OUT}/sterna.dll")
    if(MSVC)
        set(TT_CORE_IMPLIB "${TT_CARGO_OUT}/sterna.dll.lib")
    else()
        set(TT_CORE_IMPLIB "${TT_CARGO_OUT}/libsterna.dll.a")
    endif()
else()
    set(TT_CORE_LIB "${TT_CARGO_OUT}/libsterna.so")
    set(TT_CORE_IMPLIB "")
endif()

# Always run: cargo does its own dependency tracking and is a no-op when
# nothing changed, which is cheaper and more correct than globbing the
# workspace's sources into a DEPENDS list that silently misses a new file.
# The import library is a byproduct too, and on Windows it is the one the link
# actually consumes — leave it out and the generator reports a missing input
# with no rule to make it, naming a file cargo does in fact write.
add_custom_target(tt_core ALL
    BYPRODUCTS "${TT_CORE_LIB}" ${TT_CORE_IMPLIB}
    COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${TT_CARGO_DIR}"
            "${CARGO_EXECUTABLE}" build -p tt-ffi ${TT_CARGO_FLAGS}
            ${TT_CARGO_TARGET_FLAGS}
    WORKING_DIRECTORY "${TT_CORE_ROOT}"
    COMMENT "cargo build -p tt-ffi"
    USES_TERMINAL VERBATIM)

# Windows has no rpath, so the loader looks beside the executable and nowhere
# else useful. Every binary here lands in the build root, so one copy serves
# them all — and it belongs on `tt_core` rather than on `sterna`, because the
# tests are run without ever building the application.
if(WIN32)
    add_custom_command(TARGET tt_core POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${TT_CORE_LIB}" "${CMAKE_BINARY_DIR}"
        COMMENT "sterna.dll beside the executables")
endif()

add_library(sterna_core SHARED IMPORTED GLOBAL)
set_target_properties(sterna_core PROPERTIES
    IMPORTED_LOCATION "${TT_CORE_LIB}"
    INTERFACE_INCLUDE_DIRECTORIES "${TT_CORE_ROOT}/tt-ffi/include")
if(WIN32)
    set_target_properties(sterna_core PROPERTIES
        IMPORTED_IMPLIB "${TT_CORE_IMPLIB}")
endif()

# --- the shell ---------------------------------------------------------------

add_library(sterna_shell STATIC
    src/Branding.cpp
    src/MainWindow.cpp
    src/TerminalPage.cpp
    src/WindowTitle.cpp
    src/TerminalView.cpp
    src/Session.cpp
    src/Macro.cpp
    src/Plugins.cpp
    src/ConnectBar.cpp
    src/Control.cpp
    src/I18n.cpp
    src/Theme.cpp
    src/PasteDialog.cpp
    src/Printer.cpp
    src/SerialDialog.cpp
    src/SettingsDialog.cpp
    src/SshDialog.cpp
    src/SshPrompts.cpp
    src/TabRows.cpp
    src/TelnetDialog.cpp
    src/XferDialog.cpp
    resources/branding.qrc)

add_dependencies(sterna_shell tt_core)
target_link_libraries(sterna_shell PUBLIC Qt6::Widgets Qt6::PrintSupport sterna_core)
target_include_directories(sterna_shell PUBLIC src)
target_compile_options(sterna_shell PRIVATE ${TT_WARNINGS})
target_compile_definitions(sterna_shell PRIVATE
    TT_SOURCE_LANG_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../vendor/lang")

# The network stack is loaded only after Help > Check for Updates. Linking it
# into `sterna_shell` raised an idle AppImage's PSS by about 5 MB even when no
# request was made; a local library keeps the dialogs in-process and pays that
# memory only on the explicit action.
add_library(sterna_updater SHARED src/Updater.cpp)
add_dependencies(sterna_updater tt_core)
target_link_libraries(sterna_updater PUBLIC Qt6::Widgets Qt6::Network sterna_core)
target_include_directories(sterna_updater PUBLIC src)
target_compile_options(sterna_updater PRIVATE ${TT_WARNINGS})
if(WIN32)
    set_target_properties(sterna_updater PROPERTIES PREFIX "")
    target_link_libraries(sterna_updater PRIVATE shell32)
endif()

add_executable(sterna src/main.cpp)
add_dependencies(sterna sterna_updater)
if(WIN32)
    enable_language(RC)
    file(TO_CMAKE_PATH
        "${CMAKE_CURRENT_SOURCE_DIR}/../assets/branding/sterna/sterna.ico"
        TT_ICON)
    configure_file(resources/sterna.rc.in sterna.rc @ONLY)
    target_sources(sterna PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/sterna.rc")
    # A GUI-subsystem binary, as `ttermpro.exe` is. A console-subsystem one
    # opens a console window behind every session started from the desktop,
    # and closing that window kills the terminal. `Qt6::Core` carries the
    # `WinMain` this needs, so `main` is untouched — what changes is that
    # there is no stderr unless the launcher supplied one, which is why
    # `MainWindow::note` asks before writing to it.
    set_target_properties(sterna PROPERTIES WIN32_EXECUTABLE ON)
endif()
target_link_libraries(sterna PRIVATE sterna_shell)
target_compile_options(sterna PRIVATE ${TT_WARNINGS})

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
target_compile_options(render_test PRIVATE ${TT_WARNINGS})
add_test(NAME render COMMAND render_test)

# The only thing that exercises the *window's* event loop against a real
# server. Skips loudly without TT_SSH_HOST, so it is safe to leave in ctest.
add_executable(ssh_test tests/ssh_test.cpp)
target_link_libraries(ssh_test PRIVATE sterna_shell)
target_compile_options(ssh_test PRIVATE ${TT_WARNINGS})
add_test(NAME ssh COMMAND ssh_test)

add_executable(telnet_test tests/telnet_test.cpp)
target_link_libraries(telnet_test PRIVATE sterna_shell)
target_compile_options(telnet_test PRIVATE ${TT_WARNINGS})
add_test(NAME telnet COMMAND telnet_test)

# The only connection test here that skips nothing — a pty needs no server and
# no hardware — so it is what actually runs in CI.
#
# UNIX only, and unlike `control_test` the obstacle is not the transport: every
# case is a shell script. `stty raw -echo` is how the keyboard cases stop the
# child echoing, `od -An -t x1` is how the bytes are made visible and
# `stty size` is how a resize is observed — none of which `cmd.exe` can be
# asked for. Naming a Windows shell would compile and then assert against
# output that was never going to arrive; the Windows equivalent is a different
# test, and `macro_test` is what drives ConPTY through this event loop
# meanwhile.
if(UNIX)
    add_executable(pty_test tests/pty_test.cpp)
    target_link_libraries(pty_test PRIVATE sterna_shell)
    target_compile_options(pty_test PRIVATE ${TT_WARNINGS})
    add_test(NAME pty COMMAND pty_test)
endif()

# A file transfer, driven by the window's event loop rather than by a pump
# loop. It needs `lrzsz` for the transfer itself and skips loudly without it;
# the cancel case and the dialogs need nothing.
#
# UNIX only for the counterparty rather than for the code: `rz` is lrzsz, and
# there is no Windows build of it here to send to. The protocols themselves
# are covered on Windows by `tt-xfer`'s own tests.
if(UNIX)
    add_executable(xfer_test tests/xfer_test.cpp)
    target_link_libraries(xfer_test PRIVATE sterna_shell)
    target_compile_options(xfer_test PRIVATE ${TT_WARNINGS})
    add_test(NAME xfer COMMAND xfer_test)
endif()

# A TTL macro against the window's event loop rather than a pump loop, which
# is where a re-entered modal dialog or an unserviced notifier shows up. Needs
# nothing: the one connected case forks `/bin/sh`.
# Needs no printer: `PassThruPort` points at a file and the test reads back
# what a device would have received.
add_executable(print_test tests/print_test.cpp)
target_link_libraries(print_test PRIVATE sterna_shell)
target_compile_options(print_test PRIVATE ${TT_WARNINGS})
add_test(NAME print COMMAND print_test)

add_executable(macro_test tests/macro_test.cpp)
target_link_libraries(macro_test PRIVATE sterna_shell)
target_compile_options(macro_test PRIVATE ${TT_WARNINGS})
add_test(NAME macro COMMAND macro_test)

# Persistent Lua plugins use the same worker/frontend seam as macros, then add
# window-owned menu actions, shortcuts and connection lifecycle edges above it.
add_executable(plugin_test tests/plugin_test.cpp)
target_link_libraries(plugin_test PRIVATE sterna_shell)
target_compile_options(plugin_test PRIVATE ${TT_WARNINGS})
add_test(NAME plugin COMMAND plugin_test)

# A Tera Term command line, from `argv` to a window that has connected. Needs
# nothing either: the one case that connects does so to a listening socket it
# opens itself.
add_executable(cmdline_test tests/cmdline_test.cpp)
target_link_libraries(cmdline_test PRIVATE sterna_shell)
if(WIN32)
    target_link_libraries(cmdline_test PRIVATE ws2_32)
endif()
target_compile_options(cmdline_test PRIVATE ${TT_WARNINGS})
add_test(NAME cmdline COMMAND cmdline_test)

# The control socket against the window's own event loop, which is the only
# place the notifier, the nested loops and the window's four callbacks meet.
# Needs nothing: it binds an endpoint of its own and talks to itself.
#
# Both platforms, and the client half is written twice rather than shimmed:
# Unix is a `sockaddr_un` and a `poll(2)`, Windows is a byte-mode named pipe
# opened with `CreateFileW` and drained with `PeekNamedPipe`. Four calls, and
# the waiting above them is shared — because the interesting claim here is
# that the wait has to be the *window's* event loop, which is a fact about the
# window rather than about the address. `ws2_32` is for the listening socket
# the `connect` case opens, not for the control channel.
add_executable(control_test tests/control_test.cpp)
target_link_libraries(control_test PRIVATE sterna_shell)
if(WIN32)
    target_link_libraries(control_test PRIVATE ws2_32)
endif()
target_compile_options(control_test PRIVATE ${TT_WARNINGS})
add_test(NAME control COMMAND control_test)

# Multiple sessions share one menu and status bar. This pins the active-page
# routing and the stronger property underneath it: output in one tab never
# changes another tab's grid.
add_executable(tabs_test tests/tabs_test.cpp)
target_link_libraries(tabs_test PRIVATE sterna_shell)
if(WIN32)
    target_link_libraries(tabs_test PRIVATE ws2_32)
endif()
target_compile_options(tabs_test PRIVATE ${TT_WARNINGS})
add_test(NAME tabs COMMAND tabs_test)

# Signed metadata is parsed without a socket; the fixture also crosses the C
# ABI and proves the committed signer matches the public key compiled into the
# core. No release server or private key is involved. Compile the updater source
# into the test: the Windows DLL deliberately exports only its C factory symbol,
# while these network-free C++ helpers are the test seam.
add_executable(update_test tests/update_test.cpp src/Updater.cpp)
add_dependencies(update_test tt_core)
target_link_libraries(update_test PRIVATE Qt6::Widgets Qt6::Network sterna_core)
target_include_directories(update_test PRIVATE src)
target_compile_options(update_test PRIVATE ${TT_WARNINGS})
target_compile_definitions(update_test PRIVATE
    TT_UPDATE_FIXTURE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../packaging/update")
add_test(NAME update COMMAND update_test)

# The terminal does not link the updater. This proves the packaged seam it
# actually uses: load the local library, resolve its single C symbol, create
# the QObject, then destroy it before unloading its code.
add_executable(update_load_test tests/update_load_test.cpp)
add_dependencies(update_load_test sterna_updater)
target_link_libraries(update_load_test PRIVATE Qt6::Widgets)
target_compile_options(update_load_test PRIVATE ${TT_WARNINGS})
target_compile_definitions(update_load_test PRIVATE
    TT_UPDATE_LIBRARY="$<TARGET_FILE:sterna_updater>")
add_test(NAME update_load COMMAND update_load_test)

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

if(TT_CARGO_TARGET)
    set(TT_BENCH_DIR "${TT_CARGO_DIR}/${TT_CARGO_TARGET}/release")
else()
    set(TT_BENCH_DIR "${TT_CARGO_DIR}/release")
endif()
set(TT_BENCH_EXE "${TT_BENCH_DIR}/tt-bench${CMAKE_EXECUTABLE_SUFFIX}")

add_custom_target(tt_bench_emitter
    BYPRODUCTS "${TT_BENCH_EXE}"
    COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${TT_CARGO_DIR}"
            "${CARGO_EXECUTABLE}" build -p tt-bench --release
            ${TT_CARGO_TARGET_FLAGS}
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
target_compile_options(bench_shell PRIVATE ${TT_WARNINGS})
target_compile_definitions(bench_shell PRIVATE TT_BENCH_EXE="${TT_BENCH_EXE}")

# So the build tree runs without LD_LIBRARY_PATH, and so an installed one finds
# the core next to itself.
#
# The install rpath is `$ORIGIN/../lib` and not a path, because the whole point
# of the AppImage is that it has no idea where it will be mounted. Cargo gives a
# cdylib no `DT_SONAME` (see `tt-ffi/build.rs`), so what the binary records is
# whatever path the linker was handed — which out of tree is *relative*, and ran
# only from the build directory until that was fixed.
# UNIX only twice over: an rpath is a Unix concept — Windows gets the copy of
# the DLL beside the executables instead — and `pty_test` is not a target
# there, which would be an error rather than a no-op.
if(UNIX)
    set_target_properties(sterna_updater PROPERTIES
        BUILD_RPATH "${TT_CARGO_OUT}"
        INSTALL_RPATH "$ORIGIN")
    set_target_properties(sterna render_test ssh_test telnet_test pty_test
                          cmdline_test plugin_test update_test bench_shell PROPERTIES
        BUILD_RPATH "${TT_CARGO_OUT}"
        INSTALL_RPATH "$ORIGIN/../lib")
endif()

# --- install -----------------------------------------------------------------
#
# `lib` and `bin` spelled out rather than taken from GNUInstallDirs: this
# prefix is an AppDir, not a system one, and on Fedora GNUInstallDirs would say
# `lib64` while everything downstream — linuxdeploy, the AppRun it writes —
# expects `usr/lib`. A packaging layout is not a distribution layout.
install(TARGETS sterna RUNTIME DESTINATION bin)
install(TARGETS sterna_updater
        RUNTIME DESTINATION bin
        LIBRARY DESTINATION lib)
# The DLL goes beside the executable, because that is where the Windows loader
# looks — an installed `lib/sterna.dll` is a program that will not start, and
# there is no rpath to say otherwise.
if(WIN32)
    install(FILES "${TT_CORE_LIB}" DESTINATION bin)
else()
    install(FILES "${TT_CORE_LIB}" DESTINATION lib)
endif()
install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/../vendor/lang/"
        DESTINATION share/sterna/lang
        FILES_MATCHING PATTERN "*.lng")
