/* Drive the Windows C ABI from a Windows C program.
 *
 * The POSIX harness in abi.c is deliberately shaped like a Unix Qt frontend:
 * poll descriptors, Unix sockets and ptys. Compiling that under MinGW would
 * test a compatibility layer rather than the ABI the Windows shell consumes.
 * This one asks the native questions instead: HANDLE wakeups and a named-pipe
 * control client, alongside the platform-neutral screen and file surface.
 */

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <sterna.h>

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAIL %s\n", __FILE__, __LINE__, #cond);  \
            failures++;                                                        \
        }                                                                      \
    } while (0)

#define CHECK_OK(expr)                                                         \
    do {                                                                       \
        TtStatus st_ = (expr);                                                 \
        if (st_ != TT_OK) {                                                    \
            fprintf(stderr, "%s:%d: FAIL %s -> %d (%s)\n", __FILE__,         \
                    __LINE__, #expr, (int)st_, tt_last_error());               \
            failures++;                                                        \
        }                                                                      \
    } while (0)

static uint32_t base(const TtCell *cell) { return cell->text[0]; }

static void temp_name(char *out, size_t cap, const char *leaf)
{
    char dir[MAX_PATH + 1] = {0};
    DWORD n = GetTempPathA(MAX_PATH, dir);
    CHECK(n > 0 && n < MAX_PATH);
    snprintf(out, cap, "%ssterna-abi-%lu-%s", dir,
             (unsigned long)GetCurrentProcessId(), leaf);
}

static void expect_row(const TtSession *s, size_t y, const char *want)
{
    size_t len = 0;
    const TtCell *row = tt_session_row(s, y, &len);
    CHECK(row != NULL);
    if (!row)
        return;
    CHECK(len == tt_session_cols(s));
    for (size_t x = 0; x < strlen(want); x++)
        CHECK(base(&row[x]) == (uint32_t)(unsigned char)want[x]);
}

static void test_i18n(void)
{
    TtI18n *ja = tt_i18n_load("../../vendor/lang/ja_JP.lng");
    CHECK(ja != NULL);
    if (!ja)
        return;
    size_t len = 0;
    const uint8_t *text =
        tt_i18n_text(ja, "Tera Term", "MENU_FILE", "fallback", &len);
    static const char japanese[] = "ファイル(&F)";
    CHECK(text != NULL);
    CHECK(len == sizeof japanese - 1);
    CHECK(text && memcmp(text, japanese, len) == 0);
    tt_i18n_free(ja);

    char missing[MAX_PATH + 64];
    temp_name(missing, sizeof missing, "no-such-language.lng");
    CHECK(tt_i18n_load(missing) == NULL);
}

static void test_session(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    CHECK(cfg.cols == 80 && cfg.rows == 24);
    cfg.cols = 20;
    cfg.rows = 4;

    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);
    CHECK(tt_session_poll_fd(s) == -1);
    CHECK(tt_session_wait_handle(s) == NULL);

    static const char stream[] = "Hello, world!\rSecond line";
    tt_session_feed(s, (const uint8_t *)stream, sizeof stream - 1);
    expect_row(s, 0, "Second lined!");

    const TtEvent *events = NULL;
    CHECK(tt_session_drain_events(s, &events) == 1);
    CHECK(events != NULL && events[0].kind == TT_EVENT_KIND_DAMAGE);

    static const char title[] = "\033]2;a terminal\033\\";
    tt_session_feed(s, (const uint8_t *)title, sizeof title - 1);
    size_t count = tt_session_drain_events(s, &events);
    CHECK(count == 2);
    CHECK(events && events[1].kind == TT_EVENT_KIND_TITLE);
    CHECK(strcmp(tt_session_title(s), "a terminal") == 0);

    TtCursor cur;
    tt_session_cursor(s, &cur);
    CHECK(cur.x == 11 && cur.y == 0 && cur.visible);
    tt_session_free(s);
}

static void test_files(void)
{
    char ini[MAX_PATH + 64];
    char log[MAX_PATH + 64];
    char keys[MAX_PATH + 64];
    temp_name(ini, sizeof ini, "settings.ini");
    temp_name(log, sizeof log, "session.log");
    temp_name(keys, sizeof keys, "keyboard.cnf");
    DeleteFileA(ini);
    DeleteFileA(log);
    DeleteFileA(keys);

    FILE *f = fopen(ini, "wb");
    CHECK(f != NULL);
    if (f) {
        fputs("[Tera Term]\r\nTerminalSize=100,40\r\n", f);
        fclose(f);
    }
    f = fopen(keys, "wb");
    CHECK(f != NULL);
    if (f) {
        fputs("[VT editor keypad]\nUp=328\n", f);
        fclose(f);
    }

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    CHECK_OK(tt_session_settings_load(s, ini));
    CHECK(tt_session_cols(s) == 100 && tt_session_rows(s) == 40);
    CHECK_OK(tt_session_set_setting(s, "terminal.title", "win32 abi"));
    CHECK_OK(tt_session_settings_save(s, ini));
    CHECK_OK(tt_session_key_map_load(s, keys));

    TtLogOptions opts;
    tt_log_options_default(&opts);
    CHECK_OK(tt_session_log_start(s, log, &opts));
    static const char line[] = "\033[31mlogged\033[0m\r\n";
    tt_session_feed(s, (const uint8_t *)line, sizeof line - 1);
    CHECK(tt_session_log_bytes(s) == 7);
    tt_session_log_stop(s);

    f = fopen(log, "rb");
    CHECK(f != NULL);
    if (f) {
        char got[32] = {0};
        CHECK(fread(got, 1, sizeof got - 1, f) == 7);
        CHECK(strcmp(got, "logged\n") == 0);
        fclose(f);
    }

    tt_session_free(s);
    DeleteFileA(ini);
    DeleteFileA(log);
    DeleteFileA(keys);
}

static void test_command_line_and_serial(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    const char *args[] = {"/ssh", "/user=me", "myhost"};
    TtCmdLine *cmd = tt_cmdline_parse(args, 3, 0);
    CHECK(cmd != NULL);
    CHECK_OK(tt_cmdline_apply(cmd, s));
    TtStartup startup;
    CHECK(tt_cmdline_startup(cmd, s, &startup) == TT_STARTUP_OPEN);
    CHECK(startup.target == TT_TARGET_SSH);
    CHECK(startup.ssh.host && strcmp(startup.ssh.host, "myhost") == 0);
    CHECK(startup.ssh.user && strcmp(startup.ssh.user, "me") == 0);
    tt_cmdline_free(cmd);

    TtPortList *ports = tt_serial_enumerate();
    CHECK(ports != NULL);
    if (ports) {
        size_t n = tt_port_list_len(ports);
        for (size_t i = 0; i < n; i++) {
            const TtPortInfo *p = tt_port_list_at(ports, i);
            CHECK(p && p->device && p->open_path && p->label);
        }
        tt_port_list_free(ports);
    }

    TtSerialParams serial;
    tt_serial_params_default(&serial);
    CHECK(tt_session_connect_serial(s, "COM256", &serial) ==
          TT_ERR_DISCONNECTED);
    CHECK(!tt_session_is_connected(s));
    CHECK(tt_session_poll_fd(s) == -1);
    CHECK(tt_session_wait_handle(s) == NULL);
    tt_session_free(s);
}

static void on_exit_code(void *user, int32_t code)
{
    *(int32_t *)user = code;
}

static void test_macro(void)
{
    char path[MAX_PATH + 64];
    temp_name(path, sizeof path, "macro.ttl");
    DeleteFileA(path);
    FILE *f = fopen(path, "wb");
    CHECK(f != NULL);
    if (!f)
        return;
    fputs("dispstr 'macro-ok'\nsetexitcode 7\n", f);
    fclose(f);

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    int32_t callback_code = 0;
    TtMacroUi ui = {0};
    ui.user = &callback_code;
    ui.set_exit_code = on_exit_code;
    const char *args[] = {path, NULL};
    TtMacro *m = tt_macro_start(s, args, &ui);
    CHECK(m != NULL);
    if (m) {
        HANDLE event = (HANDLE)tt_macro_wait_handle(m);
        CHECK(event != NULL);
        CHECK(tt_macro_poll_fd(m) == -1);
        ULONGLONG deadline = GetTickCount64() + 10000;
        while (tt_macro_running(m) && GetTickCount64() < deadline) {
            DWORD ready = WaitForSingleObject(event, 1000);
            CHECK(ready == WAIT_OBJECT_0 || ready == WAIT_TIMEOUT);
            tt_macro_service(m, s);
        }
        tt_macro_service(m, s);
        CHECK(!tt_macro_running(m));
        CHECK(tt_macro_exit_code(m) == 7);
        CHECK(callback_code == 7);
        expect_row(s, 0, "macro-ok");
        tt_macro_free(m);
        tt_session_unlink_macro(s);
    }
    tt_session_free(s);
    DeleteFileA(path);
}

static const char *on_title(void *user)
{
    (void)user;
    return "a Windows window";
}

static void test_control(void)
{
    TtCtlHost host = {0};
    host.title = on_title;
    TtCtl *ctl = tt_ctl_start("abiwin", &host);
    CHECK(ctl != NULL);
    if (!ctl)
        return;
    const char *path = tt_ctl_path(ctl);
    CHECK(path && strstr(path, "\\\\.\\pipe\\sterna-") == path);
    CHECK(tt_ctl_poll_fd(ctl) == -1);
    HANDLE event = (HANDLE)tt_ctl_wait_handle(ctl);
    CHECK(event != NULL);

    HANDLE pipe = INVALID_HANDLE_VALUE;
    ULONGLONG deadline = GetTickCount64() + 10000;
    while (pipe == INVALID_HANDLE_VALUE && GetTickCount64() < deadline) {
        pipe = CreateFileA(path, GENERIC_READ | GENERIC_WRITE, 0, NULL,
                           OPEN_EXISTING, 0, NULL);
        if (pipe == INVALID_HANDLE_VALUE)
            Sleep(10);
    }
    CHECK(pipe != INVALID_HANDLE_VALUE);
    if (pipe != INVALID_HANDLE_VALUE) {
        static const char request[] =
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"status\"}\n";
        DWORD written = 0;
        CHECK(WriteFile(pipe, request, sizeof request - 1, &written, NULL));
        CHECK(written == sizeof request - 1);

        TtConfig cfg;
        tt_config_default(&cfg);
        TtSession *s = tt_session_new(&cfg);
        char reply[4096] = {0};
        DWORD got = 0;
        deadline = GetTickCount64() + 10000;
        while (got == 0 && GetTickCount64() < deadline) {
            DWORD ready = WaitForSingleObject(event, 100);
            CHECK(ready == WAIT_OBJECT_0 || ready == WAIT_TIMEOUT);
            tt_ctl_service(ctl, s);
            DWORD available = 0;
            if (PeekNamedPipe(pipe, NULL, 0, NULL, &available, NULL) &&
                available > 0) {
                CHECK(ReadFile(pipe, reply, sizeof reply - 1, &got, NULL));
                reply[got] = 0;
            }
        }
        CHECK(got > 0);
        CHECK(strstr(reply, "\"title\":\"a Windows window\"") != NULL);
        CHECK(strstr(reply, "\"connected\":false") != NULL);
        tt_session_free(s);
        CloseHandle(pipe);
    }
    tt_ctl_free(ctl);
}

int main(void)
{
    printf("Sterna Windows core %s\n", tt_version());
    test_i18n();
    test_session();
    test_files();
    test_command_line_and_serial();
    test_macro();
    test_control();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("Windows ABI ok\n");
    return 0;
}
