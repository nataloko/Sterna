/* Drive the C ABI from C, which is the only way to find out whether it is
 * usable from C.
 *
 * A Rust test calling these functions proves the logic and nothing about the
 * seam: it never compiles the header, never links the shared library, and
 * cannot notice that a struct the frontend has to fill in is unreachable
 * without a Rust type. This can, and it is deliberately written the way the Qt
 * shell will be — no helpers, no wrappers, just the header.
 *
 * Run it with ./run_abi.sh.
 */

/* For poll(2), which is how a frontend waits on tt_ssh_connect_poll_fd —
 * `-std=c11 -pedantic` hides everything POSIX until this is defined. */
#define _POSIX_C_SOURCE 200809L

#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <sterna.h>

/* Wait for the connection's descriptor, exactly as a frontend's event loop
 * does. A timeout is not a failure: readable is a wakeup, not a promise. */
static void wait_readable(int fd, int ms)
{
    struct pollfd pfd = {fd, POLLIN, 0};
    poll(&pfd, 1, ms);
}

static long now_ms(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

static int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            fprintf(stderr, "%s:%d: FAIL %s\n", __FILE__, __LINE__, #cond);    \
            failures++;                                                        \
        }                                                                      \
    } while (0)

#define CHECK_OK(expr)                                                         \
    do {                                                                       \
        TtStatus st_ = (expr);                                                 \
        if (st_ != TT_OK) {                                                    \
            fprintf(stderr, "%s:%d: FAIL %s -> %d (%s)\n", __FILE__, __LINE__, \
                    #expr, (int)st_, tt_last_error());                         \
            failures++;                                                        \
        }                                                                      \
    } while (0)

/* The codepoint in a cell, ignoring any combining marks. */
static uint32_t base(const TtCell *cell) { return cell->text[0]; }

/* Read a row and compare its base codepoints against ASCII. */
static void expect_row(const TtSession *s, size_t y, const char *want)
{
    size_t len = 0;
    const TtCell *row = tt_session_row(s, y, &len);
    CHECK(row != NULL);
    if (!row)
        return;
    CHECK(len == tt_session_cols(s));
    for (size_t x = 0; x < strlen(want); x++) {
        if (base(&row[x]) != (uint32_t)want[x]) {
            fprintf(stderr, "row %zu col %zu: want '%c', got U+%04X\n", y, x,
                    want[x], base(&row[x]));
            failures++;
            return;
        }
    }
}

static void test_screen(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    CHECK(cfg.cols == 80 && cfg.rows == 24);
    cfg.cols = 20;
    cfg.rows = 4;

    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);
    CHECK(tt_session_cols(s) == 20);
    CHECK(tt_session_rows(s) == 4);
    CHECK(!tt_session_is_connected(s));
    CHECK(tt_session_describe(s) == NULL);

    /* CR is a carriage RETURN by default, not CRLF — `ttset.c:643`'s else
     * branch. If this line lands on row 1 the settings are wrong, not the
     * parser. */
    static const char stream[] = "Hello, world!\rSecond line";
    tt_session_feed(s, (const uint8_t *)stream, sizeof stream - 1);
    expect_row(s, 0, "Second lined!");

    TtCursor cur;
    tt_session_cursor(s, &cur);
    CHECK(cur.y == 0);
    CHECK(cur.x == 11);
    CHECK(cur.visible);

    /* A feed queues damage; a title only when one arrives. */
    const TtEvent *events = NULL;
    size_t n = tt_session_drain_events(s, &events);
    CHECK(n == 1);
    CHECK(events != NULL && events[0].kind == TT_EVENT_KIND_DAMAGE);
    CHECK(tt_session_drain_events(s, &events) == 0);

    static const char title[] = "\033]2;a terminal\033\\";
    tt_session_feed(s, (const uint8_t *)title, sizeof title - 1);
    n = tt_session_drain_events(s, &events);
    CHECK(n == 2);
    CHECK(events[1].kind == TT_EVENT_KIND_TITLE);
    CHECK(events[1].text != NULL && strcmp(events[1].text, "a terminal") == 0);
    CHECK(strcmp(tt_session_title(s), "a terminal") == 0);

    tt_session_free(s);
}

static void test_scrollback_viewport(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 20;
    cfg.rows = 4;
    TtSession *s = tt_session_new(&cfg);

    static const char feed[] = "a\r\nb\r\nc\r\nd\r\ne\r\nf\r\n";
    tt_session_feed(s, (const uint8_t *)feed, sizeof feed - 1);
    CHECK(tt_session_scrollback_len(s) == 3);
    CHECK(tt_session_view_offset(s) == 0);
    expect_row(s, 0, "d");

    /* Scrolling back is what `tt_session_row` reports — there is no second
     * row function to pick wrongly between. */
    tt_session_set_view_offset(s, 2);
    CHECK(tt_session_view_offset(s) == 2);
    expect_row(s, 0, "b");

    /* SIZE_MAX means "as far back as it goes", because the core clamps. */
    tt_session_set_view_offset(s, SIZE_MAX);
    CHECK(tt_session_view_offset(s) == 3);
    expect_row(s, 0, "a");

    /* The cursor is on the live screen, so scrolling back moves it off the
     * bottom rather than dragging it into the history. */
    size_t crow = 999;
    tt_session_set_view_offset(s, 0);
    CHECK(tt_session_cursor_view_row(s, &crow));
    CHECK(crow == 3);
    tt_session_set_view_offset(s, 1);
    CHECK(!tt_session_cursor_view_row(s, &crow));

    /* And the core moves the offset itself so the view stays on the same
     * lines — which is why a frontend has to re-read it rather than trusting
     * what it last wrote. */
    tt_session_set_view_offset(s, 2);
    static const char more[] = "g\r\nh\r\n";
    tt_session_feed(s, (const uint8_t *)more, sizeof more - 1);
    CHECK(tt_session_view_offset(s) == 4);
    expect_row(s, 0, "b");

    tt_session_free(s);
}

static void expect_line(const TtSession *s, uint64_t n, char want)
{
    size_t len = 0;
    const TtCell *line = tt_session_line(s, n, &len);
    CHECK(line != NULL);
    if (!line)
        return;
    CHECK(len == tt_session_cols(s));
    if (base(&line[0]) != (uint32_t)want) {
        fprintf(stderr, "line %llu: want '%c', got U+%04X\n",
                (unsigned long long)n, want, base(&line[0]));
        failures++;
    }
}

/* Naming a line, which is what a selection has to hold rather than a row. */
static void test_absolute_lines(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 20;
    cfg.rows = 4;
    TtSession *s = tt_session_new(&cfg);

    static const char feed[] = "a\r\nb\r\nc\r\nd\r\ne\r\nf\r\n";
    tt_session_feed(s, (const uint8_t *)feed, sizeof feed - 1);
    /* Six lines through four rows: three have left the page, so the page
     * starts at line 3 and viewport row 0 is the same thing. */
    CHECK(tt_session_top_line(s) == 3);
    CHECK(tt_session_line_at(s, 0) == 3);
    expect_line(s, 0, 'a');
    expect_line(s, 3, 'd');

    /* The number outlives the row: 'b' is line 1 before and after. */
    uint64_t b = tt_session_line_at(s, 0) - 2;
    expect_line(s, b, 'b');
    static const char more[] = "g\r\nh\r\n";
    tt_session_feed(s, (const uint8_t *)more, sizeof more - 1);
    CHECK(tt_session_line_at(s, 0) == 5);
    expect_line(s, b, 'b');

    /* Scrolling the view renumbers nothing either. */
    tt_session_set_view_offset(s, 3);
    CHECK(tt_session_line_at(s, 0) == 2);
    expect_line(s, b, 'b');

    /* A line that has not been printed yet is absent rather than wrong, and
     * `out_len` is left alone — there is nothing to describe. */
    size_t len = 99;
    CHECK(tt_session_line(s, UINT64_MAX, &len) == NULL);
    CHECK(len == 99);
    CHECK(tt_session_line(s, tt_session_top_line(s) + 4, NULL) == NULL);

    tt_session_free(s);

    /* And one that has aged out of the scrollback, which is the case that
     * would otherwise read off the front of the buffer. */
    cfg.scrollback = 2;
    s = tt_session_new(&cfg);
    tt_session_feed(s, (const uint8_t *)feed, sizeof feed - 1);
    CHECK(tt_session_scrollback_len(s) == 2);
    CHECK(tt_session_line(s, 0, NULL) == NULL);
    expect_line(s, tt_session_top_line(s) - 2, 'b');
    tt_session_free(s);
}

static void test_logging(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    TtLogOptions opts;
    tt_log_options_default(&opts);
    CHECK(!opts.raw);
    CHECK(opts.timestamp == TT_LOG_TIMESTAMP_NONE);

    CHECK(tt_session_log_path(s) == NULL);
    CHECK(tt_session_log_bytes(s) == 0);

    /* A directory cannot be opened for writing, so this is a real IO failure
     * reported through the status rather than through an event — a frontend
     * must not end up believing it is logging when it is not. */
    CHECK(tt_session_log_start(s, "/", &opts) == TT_ERR_IO);
    CHECK(strlen(tt_last_error()) > 0);
    CHECK(tt_session_log_path(s) == NULL);

    const char *path = "/tmp/tt-ffi-abi-test.log";
    CHECK_OK(tt_session_log_start(s, path, &opts));
    CHECK(tt_session_log_path(s) != NULL);
    CHECK(strcmp(tt_session_log_path(s), path) == 0);

    /* The escape sequence is consumed by the parser, so a text log gets the
     * text and nothing else. */
    static const char stream[] = "\033[31mlogged\033[0m\r\n";
    tt_session_feed(s, (const uint8_t *)stream, sizeof stream - 1);
    CHECK(tt_session_log_bytes(s) == 7);

    tt_session_log_stop(s);
    CHECK(tt_session_log_path(s) == NULL);

    FILE *f = fopen(path, "rb");
    CHECK(f != NULL);
    if (f) {
        char buf[64] = {0};
        size_t n = fread(buf, 1, sizeof buf - 1, f);
        fclose(f);
        CHECK(n == 7);
        CHECK(strcmp(buf, "logged\n") == 0);
        remove(path);
    }

    tt_session_free(s);
}

static void test_attributes(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 10;
    cfg.rows = 2;
    TtSession *s = tt_session_new(&cfg);

    /* Bold red on blue, then a plain cell. The colour bits are the point: a
     * cell without TT_ATTR2_FORE is asking for the configured default, not
     * for palette index 0. */
    static const char sgr[] = "\033[1;31;44mX\033[0mY";
    tt_session_feed(s, (const uint8_t *)sgr, sizeof sgr - 1);

    size_t len = 0;
    const TtCell *row = tt_session_row(s, 0, &len);
    CHECK(row != NULL);
    CHECK(base(&row[0]) == 'X');
    CHECK((row[0].attrs & TT_ATTR_BOLD) != 0);
    CHECK((row[0].attrs & TT_ATTR2_FORE) != 0);
    CHECK((row[0].attrs & TT_ATTR2_BACK) != 0);
    CHECK(row[0].fg == 1);
    CHECK(row[0].bg == 4);
    CHECK(base(&row[1]) == 'Y');
    CHECK((row[1].attrs & TT_ATTR_BOLD) == 0);
    CHECK((row[1].attrs & TT_ATTR2_COLOR_MASK) == 0);
    CHECK(row[0].width_class == TT_WIDTH_NARROW);

    /* A wide character occupies two cells and the second holds no text. A
     * frontend that paints per cell has to skip the pad or it draws the glyph
     * twice. */
    static const char wide[] = "\r\n\xe6\xbc\xa2";  /* U+6F22 */
    tt_session_feed(s, (const uint8_t *)wide, sizeof wide - 1);
    row = tt_session_row(s, 1, &len);
    CHECK(base(&row[0]) == 0x6F22);
    CHECK(row[0].width_class == TT_WIDTH_WIDE);
    CHECK(row[1].width_class == TT_WIDTH_PAD);

    tt_session_free(s);
}

static void test_input(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    /* Nothing is connected, so these have nowhere to go — and must still
     * succeed rather than queue without bound. */
    bool sent = false;
    CHECK_OK(tt_session_send_key(s, TT_KEY_UP, &sent));
    CHECK(sent);
    /* Hold, Print and Break have key ids so KEYBOARD.CNF can bind them and
     * put nothing on the wire. */
    CHECK_OK(tt_session_send_key(s, TT_KEY_HOLD, &sent));
    CHECK(!sent);

    CHECK_OK(tt_session_send_text(s, "ls -l\r", SIZE_MAX));
    CHECK_OK(tt_session_paste(s, "one\ntwo", 7));
    CHECK_OK(tt_session_focus(s, true));

    tt_session_set_cell_pixels(s, 8, 16);
    TtModifiers mods = {false, false, false};
    bool consumed = true;
    CHECK_OK(tt_session_mouse(s, TT_MOUSE_EVENT_PRESS, TT_BUTTON_LEFT, 40, 32,
                              mods, &consumed));
    /* No tracking mode is on, so the click belongs to the frontend. */
    CHECK(!consumed);
    CHECK(tt_session_mouse_tracking(s) == TT_TRACKING_NONE);

    static const char track[] = "\033[?1000h";
    tt_session_feed(s, (const uint8_t *)track, sizeof track - 1);
    CHECK(tt_session_mouse_tracking(s) == TT_TRACKING_VT200);
    CHECK_OK(tt_session_mouse(s, TT_MOUSE_EVENT_PRESS, TT_BUTTON_LEFT, 40, 32,
                              mods, &consumed));
    CHECK(consumed);

    /* Backspace is one of the two keys a frontend encodes itself, so its mode
     * has to be readable.
     *
     * The default is BS, not DEL, and it is another `else` branch rather than
     * a stated default: `ttset.c:877` reads the BSKey string with an empty
     * fallback and only "DEL" takes the DEL arm, so an absent key means BS.
     * Reading the initialiser instead — the same mistake the ColorFlag words
     * cost a day to — would put 0x7F on the wire for every backspace. */
    CHECK(tt_session_backspace_sends_bs(s));
    static const char bkm[] = "\033[?67l";
    tt_session_feed(s, (const uint8_t *)bkm, sizeof bkm - 1);
    CHECK(!tt_session_backspace_sends_bs(s));

    CHECK_OK(tt_session_resize(s, 132, 43));
    CHECK(tt_session_cols(s) == 132);
    CHECK(tt_session_rows(s) == 43);
    CHECK(tt_session_resize(s, 0, 24) == TT_ERR_INVALID);

    /* Not UTF-8, and it must be refused rather than mangled. */
    CHECK(tt_session_send_text(s, "\xff\xfe", 2) == TT_ERR_INVALID);

    tt_session_free(s);
}

static void test_palette(void)
{
    uint8_t r = 0, g = 0, b = 0;
    CHECK(tt_palette_rgb(0, &r, &g, &b));
    CHECK(r == 0 && g == 0 && b == 0);
    /* The VGA values, not xterm's 205/238/229 — using xterm's moves the
     * answer for most truecolor input. */
    CHECK(tt_palette_rgb(1, &r, &g, &b));
    CHECK(r == 128 && g == 0 && b == 0);
    CHECK(tt_palette_rgb(255, &r, &g, &b));
    CHECK(r == 238 && g == 238 && b == 238);
    CHECK(!tt_palette_rgb(256, &r, &g, &b));
}

static void test_serial(void)
{
    TtSerialParams p;
    tt_serial_params_default(&p);
    CHECK(p.baud == 9600);
    CHECK(p.data_bits == 8);
    CHECK(p.stop_bits == 1);
    CHECK(p.parity == TT_PARITY_NONE);
    CHECK(p.flow == TT_FLOW_CONTROL_NONE);
    CHECK(p.dtr == TT_PIN_CONTROL_ENABLE);
    CHECK(p.detect_break);

    /* Enumeration must work on a machine with no serial ports at all — an
     * empty list, not an error. */
    TtPortList *ports = tt_serial_enumerate();
    CHECK(ports != NULL);
    if (ports) {
        size_t n = tt_port_list_len(ports);
        for (size_t i = 0; i < n; i++) {
            const TtPortInfo *info = tt_port_list_at(ports, i);
            CHECK(info != NULL);
            CHECK(info->device != NULL);
            CHECK(info->open_path != NULL);
            CHECK(info->label != NULL);
            printf("  port: %s (open as %s)\n", info->label, info->open_path);
        }
        CHECK(tt_port_list_at(ports, n) == NULL);
        tt_port_list_free(ports);
    }

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    /* A path that does not exist is "disconnected", not some "no such port":
     * the case that matters is a saved profile naming an adapter that has
     * since been unplugged. */
    CHECK(tt_session_connect_serial(s, "/dev/tt-ffi-does-not-exist", &p) ==
          TT_ERR_DISCONNECTED);
    CHECK(strlen(tt_last_error()) > 0);
    CHECK(!tt_session_is_connected(s));

    /* An FTDI accepts CS5 and transmits eight bits anyway, so the layer reads
     * the setting back; five is refused before that, at the ABI, only when it
     * is not a data-bit count at all. */
    p.data_bits = 9;
    CHECK(tt_session_connect_serial(s, "/dev/null", &p) == TT_ERR_INVALID);
    p.data_bits = 8;
    p.stop_bits = 3;
    CHECK(tt_session_connect_serial(s, "/dev/null", &p) == TT_ERR_INVALID);

    /* A pump with nothing connected is not an error, and reports no bytes. */
    p.stop_bits = 1;
    size_t got = 12345;
    CHECK_OK(tt_session_pump(s, 1, &got));
    CHECK(got == 0);

    /* And there is nothing to wait on until something is connected. A shell
     * that handed -1 to QSocketNotifier would abort, so this is the value the
     * frontend branches on rather than one it can assume away. */
    CHECK(tt_session_poll_fd(s) == -1);

    /* Typing at a disconnected window queues nothing, so the retry timer a
     * frontend runs off this never starts. */
    CHECK_OK(tt_session_send_text(s, "hello", SIZE_MAX));
    CHECK(tt_session_pending_out(s) == 0);

    tt_session_free(s);
}

/* Every entry point takes null without crashing. A frontend will pass one
 * eventually — after a failed connect, or from a signal handler racing a
 * teardown — and an ABI that segfaults on it is one nobody can debug. */

/* The schema, from C — which is how the settings dialog will be built.
 *
 * The point of the metadata table is that the dialog holds no list of its own,
 * so this walks it the way a dialog would: every field, its page, its widget
 * kind, and for an enum the spellings a combo box would offer.
 */
static void test_settings(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    size_t n = tt_settings_field_count();
    CHECK(n > 0);

    int seen_enum = 0, seen_range = 0, seen_color = 0, seen_unlabelled = 0;
    for (size_t i = 0; i < n; i++) {
        TtSettingField f;
        CHECK(tt_settings_field(i, &f));
        CHECK(f.name && f.page && f.section && f.key && f.default_value && f.doc);
        /* The page is the dotted name's first component, which is what puts a
         * setting on a tab without a second table saying so. */
        CHECK(strncmp(f.name, f.page, strlen(f.page)) == 0);
        CHECK(f.name[strlen(f.page)] == '.');
        /* A setting with no dialog upstream has no label and still has to be
         * readable and writable. */
        if (!f.label)
            seen_unlabelled++;

        if (f.kind == TT_SETTING_KIND_ENUM) {
            seen_enum++;
            CHECK(f.choices > 1);
            CHECK(tt_settings_choice(i, f.choices) == NULL);
            int found_default = 0;
            for (size_t c = 0; c < f.choices; c++) {
                const char *spelling = tt_settings_choice(i, c);
                CHECK(spelling != NULL);
                if (spelling && strcmp(spelling, f.default_value) == 0)
                    found_default = 1;
            }
            CHECK(found_default);
        } else {
            CHECK(f.choices == 0);
            CHECK(tt_settings_choice(i, 0) == NULL);
        }
        if (f.kind == TT_SETTING_KIND_INT_RANGE) {
            seen_range++;
            CHECK(f.min < f.max);
        }
        if (f.kind == TT_SETTING_KIND_COLOR2)
            seen_color++;

        /* Every name in the table is one the session answers to. */
        CHECK(tt_session_setting(s, f.name) != NULL);
    }
    CHECK(seen_enum > 0);
    CHECK(seen_range > 0);
    CHECK(seen_color > 0);
    CHECK(seen_unlabelled > 0);

    TtSettingField f;
    CHECK(!tt_settings_field(n, &f));

    /* Reading and writing by name, and the value is the file's spelling both
     * ways so a combo box can round-trip it. */
    CHECK(strcmp(tt_session_setting(s, "terminal.id"), "VT100") == 0);
    CHECK_OK(tt_session_set_setting(s, "terminal.id", "VT320"));
    CHECK(strcmp(tt_session_setting(s, "terminal.id"), "VT320") == 0);
    CHECK(tt_session_setting(s, "no.such.setting") == NULL);
    CHECK(tt_session_set_setting(s, "no.such.setting", "1") == TT_ERR_INVALID);

    /* Applying reaches the running terminal: the size resizes the grid, and
     * the backspace key changes what the window has to send. */
    CHECK(tt_session_backspace_sends_bs(s));
    CHECK_OK(tt_session_set_setting(s, "keyboard.backspace", "DEL"));
    CHECK(!tt_session_backspace_sends_bs(s));
    CHECK_OK(tt_session_set_setting(s, "terminal.cols", "132"));
    CHECK(tt_session_cols(s) == 132);
    /* Out of range is not an error — it lands where the file would put it,
     * which for a size at or below the floor is the default rather than the
     * floor itself. */
    CHECK_OK(tt_session_set_setting(s, "terminal.cols", "0"));
    CHECK(tt_session_cols(s) == 80);

    /* A round trip through a file, including a key nothing here knows about:
     * a TERATERM.INI shared with a real Tera Term has to survive being
     * written back. */
    const char *path = "/tmp/tt-ffi-abi-settings.ini";
    FILE *f2 = fopen(path, "wb");
    CHECK(f2 != NULL);
    if (f2) {
        fputs("; a comment\r\n[Tera Term]\r\nTerminalSize=100,40\r\n"
              "SomethingElse=kept\r\n",
              f2);
        fclose(f2);
    }
    CHECK_OK(tt_session_settings_load(s, path));
    CHECK(tt_session_cols(s) == 100);
    CHECK(tt_session_rows(s) == 40);
    CHECK(strcmp(tt_session_setting(s, "terminal.id"), "VT100") == 0);

    CHECK_OK(tt_session_set_setting(s, "terminal.title", "sterna"));
    CHECK_OK(tt_session_settings_save(s, path));
    f2 = fopen(path, "rb");
    CHECK(f2 != NULL);
    if (f2) {
        char buf[2048] = {0};
        size_t got = fread(buf, 1, sizeof buf - 1, f2);
        fclose(f2);
        CHECK(got > 0);
        CHECK(strstr(buf, "; a comment") != NULL);
        CHECK(strstr(buf, "SomethingElse=kept") != NULL);
        CHECK(strstr(buf, "Title=sterna") != NULL);
        remove(path);
    }

    /* A file that is not there is a first run, not a failure. */
    CHECK_OK(tt_session_settings_load(s, "/tmp/tt-ffi-abi-no-such.ini"));
    CHECK(tt_session_cols(s) == 80);

    tt_session_free(s);
}

/* Parse, apply, resolve — the three calls a frontend makes at startup, in the
 * order it has to make them.
 *
 * The settings are reloaded first, and that is the sequence rather than a
 * convenience: applying writes *into* the settings and nothing takes it back
 * out, so a second line over the first would see the first one's port. A
 * frontend loads its file once and applies once; a test that reuses a session
 * has to say so. */
static TtStartupKind startup(const char *const *argv, size_t argc, TtSession *s,
                             TtStartup *out)
{
    CHECK_OK(tt_session_settings_load(s, "/tmp/tt-ffi-abi-no-such.ini"));
    TtCmdLine *cmd = tt_cmdline_parse(argv, argc, 0);
    CHECK(cmd != NULL);
    if (!cmd)
        return TT_STARTUP_ERROR;
    CHECK_OK(tt_cmdline_apply(cmd, s));
    TtStartupKind kind = tt_cmdline_startup(cmd, s, out);
    /* Deliberately freed before the caller looks at `out`: every string in it
     * is borrowed from the handle, so a use-after-free here would show up as
     * a wrong answer under ASan rather than as a passing test. Copy first. */
    if (out->host)
        out->host = strdup(out->host);
    if (out->path)
        out->path = strdup(out->path);
    if (out->reason)
        out->reason = strdup(out->reason);
    if (out->ssh.host)
        out->ssh.host = out->host;
    if (out->ssh.user)
        out->ssh.user = strdup(out->ssh.user);
    tt_cmdline_free(cmd);
    return kind;
}

static void free_startup(TtStartup *st)
{
    free((void *)st->path);
    free((void *)st->host);
    free((void *)st->reason);
    free((void *)st->ssh.user);
}

static void test_cmdline(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    TtStartup st;

    /* Nothing named: the New Connection dialog, which is upstream's first
     * arm and the one a reimplementation forgets. */
    const char *none[] = {""};
    CHECK(startup(none, 0, s, &st) == TT_STARTUP_DIALOG);
    free_startup(&st);

    /* `/DS` suppresses it, which is how a session that will `connect` for
     * itself starts up. */
    const char *ds[] = {"/DS"};
    CHECK(startup(ds, 1, s, &st) == TT_STARTUP_IDLE);
    free_startup(&st);

    /* A bare host name is telnet, and the port decides the protocol: 23
     * negotiates, anything else auto-detects, because a terminal server's
     * per-line port is not a telnet server. */
    const char *host[] = {"myhost"};
    CHECK(startup(host, 1, s, &st) == TT_STARTUP_OPEN);
    CHECK(st.target == TT_TARGET_TELNET);
    CHECK(st.host && strcmp(st.host, "myhost") == 0);
    CHECK(st.port == 23);
    CHECK(st.telnet.mode == TT_TELNET_NEGOTIATE);
    CHECK(st.telnet.term_type != NULL);
    free_startup(&st);

    const char *hostport[] = {"myhost:2323", "/T=1"};
    CHECK(startup(hostport, 2, s, &st) == TT_STARTUP_OPEN);
    CHECK(st.port == 2323);
    CHECK(st.telnet.mode == TT_TELNET_AUTO);
    free_startup(&st);

    /* SSH, which is not opened here: it has prompts, so it goes to
     * `tt_ssh_connect` and the window drives it. Port 0 means the config's,
     * then 22 — upstream would send it to `TCPPort=`, which is 23. */
    const char *ssh[] = {"/ssh", "/user=me", "/keyfile=/tmp/k", "myhost"};
    CHECK(startup(ssh, 4, s, &st) == TT_STARTUP_OPEN);
    CHECK(st.target == TT_TARGET_SSH);
    CHECK(st.ssh.host && strcmp(st.ssh.host, "myhost") == 0);
    CHECK(st.ssh.port == 0);
    CHECK(st.ssh.user && strcmp(st.ssh.user, "me") == 0);
    CHECK(st.ssh.use_ssh_config);
    CHECK(st.ssh.host_key_policy == TT_HOST_KEY_POLICY_ASK);
    CHECK(!st.no_known_hosts_check);
    free_startup(&st);

    /* A port that was asked for survives, and the hidden option that skips
     * the `known_hosts` check is folded into the policy rather than left for
     * the frontend to interpret. */
    const char *ssh2[] = {"/ssh", "/nosecuritywarning", "myhost:2222"};
    CHECK(startup(ssh2, 3, s, &st) == TT_STARTUP_OPEN);
    CHECK(st.ssh.port == 2222);
    CHECK(st.ssh.host_key_policy == TT_HOST_KEY_POLICY_ACCEPT_ANY);
    CHECK(st.no_known_hosts_check);
    /* No `/user=`, so null rather than "" — the difference between "whatever
     * ~/.ssh/config says" and "an empty user name". */
    CHECK(st.ssh.user == NULL);
    free_startup(&st);

    /* A serial line resolves `/C=1` through enumeration. Whether there is a
     * first port is the machine's business; the answer must be one of the two
     * and never a silently different device. */
    const char *com[] = {"/C=1", "/SPEED=115200", "/CPARITY=even"};
    TtStartupKind kind = startup(com, 3, s, &st);
    CHECK(kind == TT_STARTUP_OPEN || kind == TT_STARTUP_UNSUPPORTED);
    if (kind == TT_STARTUP_OPEN) {
        CHECK(st.target == TT_TARGET_SERIAL);
        CHECK(st.path && strncmp(st.path, "/dev/", 5) == 0);
        CHECK(st.serial.baud == 115200);
        CHECK(st.serial.parity == TT_PARITY_EVEN);
    } else {
        CHECK(st.reason && strstr(st.reason, "serial port") != NULL);
    }
    free_startup(&st);

    /* An out-of-range `/C=` is **dropped rather than clamped**: the port
     * stays whatever the settings file said, so the same shortcut opens a
     * different port here from the one it opens on a machine whose
     * `MaxComPort=1024`. It does not become 999, and it does not become 256.
     *
     * It still selects the serial transport, and `ComAutoConnect` starts true
     * on every call — so this connects rather than asking, which is why the
     * two cases below both need a `/M=` to reach the dialog at all. */
    const char *com999[] = {"/C=999", "/M=x", "/DS"};
    CHECK(startup(com999, 3, s, &st) == TT_STARTUP_IDLE);
    CHECK(strcmp(tt_session_setting(s, "serial.com_port"), "1") == 0);
    free_startup(&st);

    /* ...and an in-range one turns `ComAutoConnect` back on, after the option
     * loop rather than inside it, so the order of the two does not matter. */
    const char *com1m[] = {"/C=1", "/M=x", "/DS"};
    CHECK(startup(com1m, 3, s, &st) != TT_STARTUP_IDLE);
    free_startup(&st);

    /* Two transports upstream has and this does not say which, rather than
     * opening something else. */
    const char *replay[] = {"/R=session.log"};
    CHECK(startup(replay, 1, s, &st) == TT_STARTUP_UNSUPPORTED);
    CHECK(st.reason && strstr(st.reason, "replaying") != NULL);
    free_startup(&st);

    /* Applying reaches the settings, and through them the running terminal —
     * `/W=` is a title and `/H` is a window without one. */
    const char *win[] = {"/W=My Session", "/H", "/TIMEOUT=7", "/DS"};
    CHECK(startup(win, 4, s, &st) == TT_STARTUP_IDLE);
    CHECK(strcmp(tt_session_setting(s, "terminal.title"), "My Session") == 0);
    CHECK(strcmp(tt_session_setting(s, "window.hide_title"), "on") == 0);
    CHECK(strcmp(tt_session_setting(s, "connection.timeout"), "7") == 0);
    free_startup(&st);

    /* The options a window acts on itself, none of which is a setting. */
    const char *opts[] = {"/F=other.ini", "/L=out.log", "/M=setup.ttl",
                          "/I",           "/X=100",     "/DS"};
    TtCmdLine *cmd = tt_cmdline_parse(opts, 6, 0);
    CHECK(cmd != NULL);
    TtCmdLineInfo info;
    CHECK(tt_cmdline_info(cmd, &info));
    CHECK(info.setup_file && strcmp(info.setup_file, "other.ini") == 0);
    CHECK(info.log_file && strcmp(info.log_file, "out.log") == 0);
    CHECK(info.macro_kind == TT_MACRO_FILE);
    CHECK(info.macro_file && strcmp(info.macro_file, "setup.ttl") == 0);
    CHECK(info.minimize && !info.hide_window);
    /* `/X=` alone: upstream pairs them, so the axis that was not given is 0
     * and the window does not land wherever the manager felt like. */
    CHECK(info.has_x && info.x == 100);
    CHECK(!info.has_y);
    CHECK(info.unknown_count == 0);
    tt_cmdline_free(cmd);

    /* `/NOLOG` wins over `/L=`, which is what the manual says and what
     * upstream's own code does not do. */
    const char *nolog[] = {"/L=out.log", "/NOLOG"};
    cmd = tt_cmdline_parse(nolog, 2, 0);
    CHECK(cmd != NULL);
    CHECK(tt_cmdline_info(cmd, &info));
    CHECK(info.log_file == NULL);
    tt_cmdline_free(cmd);

    /* A `/D=` topic frees the startup macro name unconditionally, which is
     * the third state and the one that is easy to miss. */
    const char *dde[] = {"/D=topic"};
    cmd = tt_cmdline_parse(dde, 1, 0);
    CHECK(cmd != NULL);
    CHECK(tt_cmdline_info(cmd, &info));
    CHECK(info.macro_kind == TT_MACRO_CLEARED);
    CHECK(info.macro_file == NULL);
    tt_cmdline_free(cmd);

    /* A mistyped `/ssh` option is the only diagnostic in either parser. */
    const char *typo[] = {"/ssh", "/ssh-nosuchthing", "myhost"};
    cmd = tt_cmdline_parse(typo, 3, 0);
    CHECK(cmd != NULL);
    CHECK(tt_cmdline_info(cmd, &info));
    CHECK(info.unknown_count == 1);
    CHECK(tt_cmdline_unknown(cmd, 0) != NULL);
    CHECK(tt_cmdline_unknown(cmd, 1) == NULL);
    tt_cmdline_free(cmd);

    /* `max_com_port` bounds `/C=`, which is why it is a parameter: the same
     * line opens a port on a machine whose `MaxComPort=1024` and puts the
     * dialog up on one that took the default. */
    const char *com300[] = {"/C=300", "/DS"};
    cmd = tt_cmdline_parse(com300, 2, 1024);
    CHECK(cmd != NULL);
    CHECK_OK(tt_cmdline_apply(cmd, s));
    CHECK(strcmp(tt_session_setting(s, "serial.com_port"), "300") == 0);
    tt_cmdline_free(cmd);

    tt_session_free(s);
}

static void test_null_safety(void)
{
    tt_config_default(NULL);
    CHECK(tt_session_new(NULL) == NULL);
    tt_session_free(NULL);
    tt_session_set_write_timeout(NULL, 10);
    CHECK(tt_session_cols(NULL) == 0);
    CHECK(tt_session_rows(NULL) == 0);
    CHECK(tt_session_row(NULL, 0, NULL) == NULL);
    tt_session_cursor(NULL, NULL);
    CHECK(!tt_session_reverse_video(NULL));
    CHECK(tt_session_mouse_tracking(NULL) == TT_TRACKING_NONE);
    CHECK(tt_session_drain_events(NULL, NULL) == 0);
    CHECK(tt_session_pump(NULL, 0, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_poll_fd(NULL) == -1);
    CHECK(tt_session_scrollback_len(NULL) == 0);
    tt_log_options_default(NULL);
    CHECK(tt_session_log_start(NULL, "/tmp/x", NULL) == TT_ERR_INVALID);
    tt_session_log_stop(NULL);
    CHECK(tt_session_log_path(NULL) == NULL);
    CHECK(tt_session_log_bytes(NULL) == 0);
    CHECK(tt_session_view_offset(NULL) == 0);
    tt_session_set_view_offset(NULL, 5);
    CHECK(!tt_session_cursor_view_row(NULL, NULL));
    CHECK(tt_session_line_at(NULL, 0) == 0);
    CHECK(tt_session_top_line(NULL) == 0);
    CHECK(tt_session_line(NULL, 0, NULL) == NULL);
    CHECK(tt_session_pending_out(NULL) == 0);
    /* Null answers false, which happens to be the non-default — a frontend
     * that lost its session sends DEL rather than reading through a null. */
    CHECK(!tt_session_backspace_sends_bs(NULL));
    CHECK(tt_session_send_text(NULL, "x", 1) == TT_ERR_INVALID);
    CHECK(tt_session_focus(NULL, true) == TT_ERR_INVALID);
    CHECK(tt_session_send_break(NULL, 1) == TT_ERR_INVALID);
    tt_session_feed(NULL, NULL, 0);
    tt_session_disconnect(NULL);
    CHECK(!tt_session_is_connected(NULL));
    CHECK(tt_session_describe(NULL) == NULL);
    CHECK(tt_port_list_len(NULL) == 0);
    CHECK(tt_port_list_at(NULL, 0) == NULL);
    tt_port_list_free(NULL);
    tt_ssh_params_default(NULL);
    CHECK(tt_ssh_connect(NULL) == NULL);
    CHECK(tt_ssh_connect_poll_fd(NULL) == -1);
    CHECK(tt_ssh_connect_poll(NULL, NULL) == TT_SSH_FAILED);
    CHECK(tt_ssh_connect_host_key(NULL) == NULL);
    CHECK(tt_ssh_connect_auth(NULL) == NULL);
    tt_ssh_connect_answer_host_key(NULL, 1);
    tt_ssh_connect_answer_auth(NULL, NULL, 0);
    tt_ssh_connect_free(NULL);
    CHECK(tt_string_list_len(NULL) == 0);
    CHECK(tt_string_list_at(NULL, 0) == NULL);
    tt_string_list_free(NULL);
    tt_telnet_params_default(NULL, 23);
    CHECK(tt_session_connect_telnet(NULL, "h", 23, NULL) == TT_ERR_INVALID);
    tt_pty_params_default(NULL);
    CHECK(tt_session_connect_pty(NULL, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_close_note(NULL) == NULL);
    CHECK(!tt_session_supports_break(NULL));
    CHECK(tt_session_setting(NULL, "terminal.cols") == NULL);
    CHECK(tt_session_set_setting(NULL, "terminal.cols", "80") == TT_ERR_INVALID);
    CHECK(tt_session_settings_load(NULL, "/tmp/x.ini") == TT_ERR_INVALID);
    CHECK(tt_session_settings_save(NULL, "/tmp/x.ini") == TT_ERR_INVALID);

    /* A command line with no arguments is a valid one — it is what a bare
     * `sterna` has — so a null `argv` parses rather than failing. */
    TtCmdLine *empty = tt_cmdline_parse(NULL, 0, 0);
    CHECK(empty != NULL);
    TtStartup st_null;
    CHECK(tt_cmdline_startup(empty, NULL, &st_null) == TT_STARTUP_ERROR);
    CHECK(tt_cmdline_startup(empty, NULL, NULL) == TT_STARTUP_ERROR);
    CHECK(tt_cmdline_apply(empty, NULL) == TT_ERR_INVALID);
    CHECK(!tt_cmdline_info(empty, NULL));
    tt_cmdline_free(empty);
    /* And a null entry in a real argv is a caller bug, reported rather than
     * dereferenced. */
    const char *holed[] = {"/DS", NULL};
    CHECK(tt_cmdline_parse(holed, 2, 0) == NULL);
    CHECK(tt_cmdline_startup(NULL, NULL, NULL) == TT_STARTUP_ERROR);
    CHECK(tt_cmdline_apply(NULL, NULL) == TT_ERR_INVALID);
    CHECK(!tt_cmdline_info(NULL, NULL));
    CHECK(tt_cmdline_unknown(NULL, 0) == NULL);
    tt_cmdline_free(NULL);
    CHECK(tt_macro_start(NULL, NULL, NULL) == NULL);
    CHECK(tt_macro_poll_fd(NULL) == -1);
    CHECK(tt_macro_service(NULL, NULL) == 0);
    CHECK(!tt_macro_running(NULL));
    CHECK(tt_macro_exit_code(NULL) == 0);
    tt_macro_cancel(NULL);
    tt_macro_free(NULL);
    tt_session_unlink_macro(NULL);
    CHECK(!tt_settings_field(0, NULL));
    CHECK(tt_last_error() != NULL);

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    /* A null out-parameter is allowed everywhere it appears. */
    CHECK(tt_session_row(s, 0, NULL) != NULL);
    CHECK(tt_session_row(s, cfg.rows, NULL) == NULL);
    CHECK_OK(tt_session_pump(s, 0, NULL));
    CHECK(tt_session_send_text(s, NULL, 1) == TT_ERR_INVALID);
    CHECK(tt_session_connect_serial(s, "/dev/null", NULL) == TT_ERR_INVALID);
    CHECK(tt_session_connect_telnet(s, "h", 23, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_connect_telnet(s, NULL, 23, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_connect_pty(s, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_setting(s, NULL) == NULL);
    CHECK(tt_session_set_setting(s, NULL, "1") == TT_ERR_INVALID);
    CHECK(tt_session_settings_load(s, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_close_note(s) == NULL);
    /* A null argv is a caller bug; a `/M` that named nothing is the command
     * line's to report, and both refuse here rather than opening a file. */
    CHECK(tt_macro_start(s, NULL, NULL) == NULL);
    const char *no_name[] = {"/V", NULL};
    CHECK(tt_macro_start(s, no_name, NULL) == NULL);
    const char *missing[] = {"/tmp/tt-abi-no-such-macro.ttl", NULL};
    CHECK(tt_macro_start(s, missing, NULL) == NULL);
    tt_session_free(s);
}


/* Drive an SSH connection the way the shell will: poll, answer, poll.
 *
 * The server comes from TT_SSH_HOST/PORT/USER/KEY, and without them this
 * exercises the parts that need no server — the defaults, the null handling,
 * and the config alias list — then skips loudly rather than passing quietly.
 */
static void test_ssh(void)
{
    TtSshParams p;
    tt_ssh_params_default(&p);
    CHECK(p.host == NULL);
    CHECK(p.port == 0);
    CHECK(p.use_ssh_config);
    CHECK(p.use_agent);
    CHECK(!p.legacy);
    CHECK(p.host_key_policy == TT_HOST_KEY_POLICY_ASK);
    CHECK(p.connect_timeout_ms == 30000);

    /* Reading ~/.ssh/config must work on a machine that has none. */
    TtStringList *aliases = tt_ssh_config_aliases();
    CHECK(aliases != NULL);
    if (aliases) {
        size_t n = tt_string_list_len(aliases);
        for (size_t i = 0; i < n; i++)
            CHECK(tt_string_list_at(aliases, i) != NULL);
        CHECK(tt_string_list_at(aliases, n) == NULL);
        tt_string_list_free(aliases);
    }

    /* A host that is not there fails rather than hanging, and does it through
     * TT_SSH_FAILED rather than by returning null from tt_ssh_connect —
     * connecting is asynchronous, so nothing can be known at the start. */
    p.host = "127.0.0.1";
    p.port = 1;  /* nothing listens on tcpmux */
    p.use_ssh_config = false;
    p.connect_timeout_ms = 5000;
    TtSshConnect *c = tt_ssh_connect(&p);
    CHECK(c != NULL);
    if (c) {
        CHECK(tt_ssh_connect_poll_fd(c) >= 0);
        TtSshStep step;
        for (int i = 0; i < 2000; i++) {
            step = tt_ssh_connect_poll(c, NULL);
            if (step != TT_SSH_WORKING)
                break;
            wait_readable(tt_ssh_connect_poll_fd(c), 20);
        }
        CHECK(step == TT_SSH_FAILED);
        CHECK(strlen(tt_last_error()) > 0);
        tt_ssh_connect_free(c);
    }

    const char *host = getenv("TT_SSH_HOST");
    const char *key = getenv("TT_SSH_KEY");
    if (!host || !key) {
        printf("  ssh: SKIPPED (set TT_SSH_HOST and TT_SSH_KEY)\n");
        return;
    }

    const char *identities[2] = {key, NULL};
    tt_ssh_params_default(&p);
    p.host = host;
    p.port = (uint16_t)atoi(getenv("TT_SSH_PORT") ? getenv("TT_SSH_PORT") : "22");
    p.user = getenv("TT_SSH_USER");
    p.identities = identities;
    /* The agent belongs to whoever runs the tests and must not decide
     * whether they pass. */
    p.use_agent = false;
    /* No ~/.ssh/config: the point here is the ABI, not the file. */
    p.use_ssh_config = false;
    p.connect_timeout_ms = 15000;

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    c = tt_ssh_connect(&p);
    CHECK(c != NULL);
    if (!c) {
        tt_session_free(s);
        return;
    }
    int fd = tt_ssh_connect_poll_fd(c);
    CHECK(fd >= 0);

    int ready = 0, asked_host_key = 0;
    for (int i = 0; i < 4000; i++) {
        TtSshStep step = tt_ssh_connect_poll(c, s);
        if (step == TT_SSH_HOST_KEY) {
            const TtSshHostKeyPrompt *hk = tt_ssh_connect_host_key(c);
            CHECK(hk != NULL);
            if (hk) {
                CHECK(hk->host != NULL && strcmp(hk->host, host) == 0);
                CHECK(hk->algorithm != NULL && strlen(hk->algorithm) > 0);
                /* Every other client prints this form, and a user comparing
                 * it against a Post-it needs it to match. */
                CHECK(hk->fingerprint != NULL &&
                      strncmp(hk->fingerprint, "SHA256:", 7) == 0);
                printf("  ssh: %s %s\n", hk->algorithm, hk->fingerprint);
                asked_host_key = 1;
            }
            /* 2 = accept once: do not write to the user's known_hosts from a
             * test run. */
            tt_ssh_connect_answer_host_key(c, 2);
            continue;
        }
        if (step == TT_SSH_AUTH) {
            const TtSshAuthPrompt *a = tt_ssh_connect_auth(c);
            CHECK(a != NULL);
            /* The key should have answered; anything asked here is a
             * failure of the auth ordering, not of the prompt plumbing. */
            fprintf(stderr, "unexpected auth prompt kind %u\n",
                    a ? a->kind : 0);
            failures++;
            tt_ssh_connect_answer_auth(c, NULL, 0);
            continue;
        }
        if (step == TT_SSH_READY) {
            ready = 1;
            break;
        }
        if (step == TT_SSH_FAILED) {
            fprintf(stderr, "ssh connect failed: %s\n", tt_last_error());
            failures++;
            break;
        }
        wait_readable(fd, 20);
    }
    CHECK(asked_host_key);
    CHECK(ready);

    if (ready) {
        CHECK(tt_session_is_connected(s));
        /* One descriptor across the handover: a frontend registers its
         * notifier before connecting and keeps it. */
        CHECK(tt_session_poll_fd(s) == fd);
        CHECK(tt_session_describe(s) != NULL);

        /* And it is a terminal: type at it and read the screen back.
         *
         * `tt_session_pump` returns the moment the line is quiet — that is
         * the point of it — so waiting has to be done on the descriptor, not
         * by pumping in a loop. Pumping alone spins through a thousand
         * iterations in a millisecond and concludes the shell never started.
         */
        size_t got = 0;
        long deadline = now_ms() + 5000;
        /* A login shell is not ready when request_shell returns: the MOTD and
         * the first prompt arrive first, and anything typed before bash reads
         * is echoed by the pty and dropped. */
        long quiet_until = now_ms() + 500;
        while (now_ms() < deadline && now_ms() < quiet_until) {
            wait_readable(fd, 100);
            tt_session_pump(s, 20, &got);
            if (got > 0)
                quiet_until = now_ms() + 500;
        }
        CHECK_OK(tt_session_send_text(s, "echo abi-ok\n", 12));

        int seen = 0;
        deadline = now_ms() + 5000;
        while (now_ms() < deadline && !seen) {
            wait_readable(fd, 100);
            tt_session_pump(s, 20, &got);
            for (size_t y = 0; y < tt_session_rows(s) && !seen; y++) {
                size_t len = 0;
                const TtCell *row = tt_session_row(s, y, &len);
                char line[256] = {0};
                for (size_t x = 0; x < len && x < sizeof line - 1; x++)
                    line[x] = base(&row[x]) < 128 ? (char)base(&row[x]) : '?';
                /* The echoed command line contains the text too, so match a
                 * line that starts with the output rather than contains it. */
                if (strncmp(line, "abi-ok", 6) == 0)
                    seen = 1;
            }
        }
        CHECK(seen);
    }
    tt_ssh_connect_free(c);
    tt_session_free(s);
}


/* Telnet, against a real server when there is one.
 *
 * Synchronous, unlike SSH: telnet asks no questions, so there is no state
 * machine to drive and the whole path is one call.
 */
static void test_telnet(void)
{
    TtTelnetParams p;
    /* Upstream's rule, and the one thing here that is easy to get wrong: the
     * mode comes from the port. */
    tt_telnet_params_default(&p, 23);
    CHECK(p.mode == TT_TELNET_NEGOTIATE);
    tt_telnet_params_default(&p, 2001);
    CHECK(p.mode == TT_TELNET_AUTO);
    CHECK(p.input_speed == 38400);
    CHECK(!p.binary);
    CHECK(p.term_type == NULL); /* null means the default, not empty */

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    /* Nothing listens on tcpmux, and the failure has to be an error rather
     * than a hang. */
    tt_telnet_params_default(&p, 1);
    p.connect_timeout_ms = 2000;
    CHECK(tt_session_connect_telnet(s, "127.0.0.1", 1, &p) != TT_OK);
    CHECK(strlen(tt_last_error()) > 0);
    CHECK(!tt_session_is_connected(s));

    const char *host = getenv("TT_TELNET_HOST");
    if (!host) {
        printf("  telnet: SKIPPED (set TT_TELNET_HOST)\n");
        tt_session_free(s);
        return;
    }
    const char *port_s = getenv("TT_TELNET_PORT");
    uint16_t port = (uint16_t)(port_s ? atoi(port_s) : 23);

    tt_telnet_params_default(&p, port);
    /* The server is on 2323 rather than 23, so say so explicitly — the
     * default would auto-detect and never send the opening burst. */
    p.mode = TT_TELNET_NEGOTIATE;
    p.term_type = "vt100";
    CHECK_OK(tt_session_connect_telnet(s, host, port, &p));
    CHECK(tt_session_is_connected(s));
    /* Telnet has a break where SSH does not: a console server turns it into a
     * real one on the serial port behind it. */
    CHECK(tt_session_supports_break(s));
    printf("  telnet: %s\n", tt_session_describe(s));

    int fd = tt_session_poll_fd(s);
    CHECK(fd >= 0);

    size_t got = 0;
    long deadline = now_ms() + 5000;
    long quiet_until = now_ms() + 500;
    while (now_ms() < deadline && now_ms() < quiet_until) {
        wait_readable(fd, 100);
        tt_session_pump(s, 20, &got);
        if (got > 0)
            quiet_until = now_ms() + 500;
    }

    CHECK_OK(tt_session_send_text(s, "abi-telnet-ok\r\n", 15));
    int seen = 0;
    deadline = now_ms() + 5000;
    while (now_ms() < deadline && !seen) {
        wait_readable(fd, 100);
        tt_session_pump(s, 20, &got);
        for (size_t y = 0; y < tt_session_rows(s) && !seen; y++) {
            size_t len = 0;
            const TtCell *row = tt_session_row(s, y, &len);
            char line[256] = {0};
            for (size_t x = 0; x < len && x < sizeof line - 1; x++)
                line[x] = base(&row[x]) < 128 ? (char)base(&row[x]) : '?';
            if (strstr(line, "abi-telnet-ok"))
                seen = 1;
        }
    }
    CHECK(seen);

    tt_session_disconnect(s);
    CHECK(!tt_session_is_connected(s));
    tt_session_free(s);
}

/* The local shell — the one transport that needs nothing set up, so this is
 * the only connect path in this file that always runs.
 */
static void test_pty(void)
{
    TtPtyParams p;
    tt_pty_params_default(&p);
    CHECK(p.argv == NULL); /* null means the login shell, not "no program" */
    CHECK(p.argc == 0);
    CHECK(p.term == NULL); /* null means the default, not empty */
    CHECK(p.login_shell);

    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 40;
    cfg.rows = 6;
    TtSession *s = tt_session_new(&cfg);

    /* An explicit command, so the test does not depend on the developer's
     * shell or on their dotfiles. */
    const char *argv[] = {"/bin/sh", "-c", "stty size; printf 'pty-abi-ok\\r\\n'; exit 5"};
    p.argv = argv;
    p.argc = 3;
    p.term = "vt220-abi";
    CHECK_OK(tt_session_connect_pty(s, &p));
    CHECK(tt_session_is_connected(s));
    /* A pty has no line to break, and the menu is drawn from this. */
    CHECK(!tt_session_supports_break(s));
    CHECK(tt_session_close_note(s) == NULL);

    int fd = tt_session_poll_fd(s);
    CHECK(fd >= 0);

    int seen = 0, sized = 0;
    long deadline = now_ms() + 5000;
    while (now_ms() < deadline && tt_session_is_connected(s)) {
        wait_readable(fd, 100);
        size_t got = 0;
        tt_session_pump(s, 20, &got);
        for (size_t y = 0; y < tt_session_rows(s); y++) {
            size_t len = 0;
            const TtCell *row = tt_session_row(s, y, &len);
            char line[256] = {0};
            for (size_t x = 0; x < len && x < sizeof line - 1; x++)
                line[x] = base(&row[x]) < 128 ? (char)base(&row[x]) : '?';
            if (strstr(line, "pty-abi-ok"))
                seen = 1;
            /* The child starts at the *session's* size, not at 80x24. */
            if (strstr(line, "6 40"))
                sized = 1;
        }
    }
    CHECK(seen);
    CHECK(sized);

    /* The child exited, so the session must have noticed rather than sitting
     * on a quiet line, and must be able to say what happened to it. */
    CHECK(!tt_session_is_connected(s));
    const char *note = tt_session_close_note(s);
    CHECK(note != NULL);
    if (note) {
        printf("  pty: %s\n", note);
        CHECK(strstr(note, "exited with status 5") != NULL);
    }
    /* The descriptor belonged to the transport and went with it. */
    CHECK(tt_session_poll_fd(s) < 0);

    tt_session_free(s);
}

/* A ZMODEM send to a real `rz`, over a pty, driven entirely from C.
 *
 * The only test that exercises the transfer seam the way the shell will: a
 * job struct built as a dialog would build it, a pump loop that has to honour
 * `tt_session_transfer_deadline_ms` as well as the descriptor, and the result
 * read on the done event rather than guessed at. */
static void test_transfer(void)
{
    if (system("command -v rz >/dev/null 2>&1") != 0) {
        printf("  transfer: skipped, lrzsz is not installed\n");
        return;
    }

    char dir[] = "/tmp/tt-abi-xfer-XXXXXX";
    if (mkdtemp(dir) == NULL) {
        CHECK(0);
        return;
    }
    char src[256], got[256], cmd[600];
    snprintf(src, sizeof src, "%s/payload.bin", dir);
    snprintf(got, sizeof got, "%s/out/payload.bin", dir);
    snprintf(cmd, sizeof cmd, "mkdir -p %s/out", dir);
    CHECK(system(cmd) == 0);

    FILE *f = fopen(src, "wb");
    CHECK(f != NULL);
    if (!f)
        return;
    for (int i = 0; i < 32768; i++)
        fputc((i * 31 + i / 251) & 0xff, f);
    fclose(f);

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    TtPtyParams p;
    tt_pty_params_default(&p);
    const char *argv[] = {"/bin/sh", "-c", "rz -b 2>/dev/null"};
    p.argv = argv;
    p.argc = 3;
    p.cwd = NULL;
    /* `rz` writes to its working directory, and the pty starts the child in
     * the user's home unless told otherwise. */
    char cwd[256];
    snprintf(cwd, sizeof cwd, "%s/out", dir);
    p.cwd = cwd;
    CHECK_OK(tt_session_connect_pty(s, &p));

    TtXferJob job = {0};
    job.protocol = TT_XFER_PROTOCOL_Z_MODEM;
    job.sending = true;
    job.binary = true;
    const char *files[] = {src};
    CHECK_OK(tt_session_send_files(s, &job, files, 1));

    TtTransferStatus st;
    CHECK(tt_session_transfer_status(s, &st));
    CHECK(!tt_session_transfer_result(s, NULL)); /* nothing has finished yet */

    int fd = tt_session_poll_fd(s);
    int done = 0, saw_progress = 0;
    TtTransferResult result = {0};
    long deadline = now_ms() + 30000;
    while (!done && now_ms() < deadline) {
        size_t n = 0;
        tt_session_pump(s, 20, &n);
        const TtEvent *evs = NULL;
        size_t count = tt_session_drain_events(s, &evs);
        for (size_t i = 0; i < count; i++) {
            if (evs[i].kind == TT_EVENT_KIND_TRANSFER_PROGRESS)
                saw_progress = 1;
            if (evs[i].kind == TT_EVENT_KIND_TRANSFER_DONE) {
                CHECK(tt_session_transfer_result(s, &result));
                done = 1;
            }
        }
        if (done)
            break;
        /* Both wakeups matter. The descriptor carries the peer's answers; the
         * deadline carries the protocol's own retries, and on a quiet line
         * there is no descriptor wakeup at all. */
        long wait = tt_session_transfer_deadline_ms(s);
        wait_readable(fd, wait >= 0 && wait < 50 ? (int)wait + 1 : 50);
    }
    CHECK(done);
    CHECK(saw_progress);
    CHECK(result.success);
    CHECK(!result.cancelled);
    /* And it is a terminal again. */
    CHECK(!tt_session_transfer_status(s, &st));

    snprintf(cmd, sizeof cmd, "cmp -s %s %s", src, got);
    CHECK(system(cmd) == 0);

    tt_session_free(s);
    snprintf(cmd, sizeof cmd, "rm -rf %s", dir);
    CHECK(system(cmd) == 0);
}

static char macro_dir[64];

/* Write a macro out and hand back its path.
 *
 * A directory with a named file in it rather than `mkstemp`, because the name
 * matters: upstream fits `.TTL` onto a filename that has no extension at all
 * (`FitTTLFileName`), so a `tt-abi-macro-XXXXXX` template names a macro that
 * is not the file just written — which is right, and was the first thing this
 * test found.
 */
static const char *write_macro(const char *body)
{
    static char path[128];
    snprintf(macro_dir, sizeof macro_dir, "/tmp/tt-abi-macro-XXXXXX");
    if (mkdtemp(macro_dir) == NULL)
        return NULL;
    snprintf(path, sizeof path, "%s/m.ttl", macro_dir);
    FILE *f = fopen(path, "w");
    if (!f)
        return NULL;
    fputs(body, f);
    fclose(f);
    return path;
}

static void remove_macro(void)
{
    char cmd[128];
    snprintf(cmd, sizeof cmd, "rm -rf %s", macro_dir);
    CHECK(system(cmd) == 0);
}

/* What the macro below asked of its frontend, so the test can check that each
 * callback fired with what the script wrote. */
struct ui_log {
    int messages;
    char last_text[128];
    char last_title[128];
    int inputs;
    int list_choice;
    int exit_code;
    int errors;
};

static TtDialogEnd on_message(void *user, const char *text, const char *title)
{
    struct ui_log *log = user;
    log->messages++;
    snprintf(log->last_text, sizeof log->last_text, "%s", text);
    snprintf(log->last_title, sizeof log->last_title, "%s", title);
    return TT_DIALOG_OK;
}

static TtDialogEnd on_input(void *user, const char *text, const char *title,
                            const char *initial, bool password,
                            const char **out_text)
{
    struct ui_log *log = user;
    (void)text;
    (void)title;
    (void)initial;
    log->inputs++;
    /* Static rather than stack: the contract is only that it survives the
     * callback, but a frontend will use a QByteArray member and this is the
     * C equivalent. */
    static char answer[32];
    snprintf(answer, sizeof answer, password ? "hunter2" : "typed");
    *out_text = answer;
    return TT_DIALOG_OK;
}

static TtDialogEnd on_list(void *user, const char *text, const char *title,
                           const char *const *items, size_t count,
                           size_t selected, const TtListBoxOpts *opts,
                           size_t *out_index)
{
    struct ui_log *log = user;
    (void)text;
    (void)title;
    (void)selected;
    /* The macro asked for three items and a size; both should have arrived. */
    if (count == 3 && strcmp(items[2], "third") == 0 && opts->width == 40)
        log->list_choice = 1;
    *out_index = 2;
    return TT_DIALOG_OK;
}

static void on_exit_code(void *user, int32_t code)
{
    struct ui_log *log = user;
    log->exit_code = code;
}

static bool on_error(void *user, const TtMacroError *err)
{
    struct ui_log *log = user;
    log->errors++;
    fprintf(stderr, "  macro error: %s:%zu: %s\n", err->file, err->line_no,
            err->message);
    return true;
}

/* A macro against a real session, driven the way the shell's event loop will
 * drive it: wait on both descriptors, service the macro, pump the line. */
static void test_macro(void)
{
    const char *path = write_macro(
        "msg = 'hello '\n"
        "strconcat msg param2\n"
        "messagebox msg 'title'\n"
        "inputbox 'who' 'ask'\n"
        "strdim items 3\n"
        "items[0] = 'first'\n"
        "items[1] = 'second'\n"
        "items[2] = 'third'\n"
        /* The keyword is quoted because every TTL argument is an expression:
         * bare, `listboxsize` is a variable nobody defined. */
        "listbox 'pick' 'choose' items 'listboxsize=40x10'\n"
        "dispstr 'chose ' items[result] ' as ' inputstr\n"
        "setexitcode 7\n");
    CHECK(path != NULL);
    if (!path)
        return;

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    struct ui_log log = {0};
    TtMacroUi ui = {0};
    ui.user = &log;
    ui.message_box = on_message;
    ui.input_box = on_input;
    ui.list_box = on_list;
    ui.set_exit_code = on_exit_code;
    ui.error = on_error;

    /* The second argument is a parameter, which the script reads as `param2`
     * — the whole point of taking a command line rather than a path. */
    const char *argv[] = {path, "world", NULL};
    TtMacro *m = tt_macro_start(s, argv, &ui);
    CHECK(m != NULL);
    if (!m) {
        fprintf(stderr, "  %s\n", tt_last_error());
        tt_session_free(s);
        return;
    }

    int mfd = tt_macro_poll_fd(m);
    CHECK(mfd >= 0);
    long deadline = now_ms() + 10000;
    for (;;) {
        tt_macro_service(m, s);
        size_t n = 0;
        tt_session_pump(s, 0, &n);
        if (!tt_macro_running(m)) {
            /* Once more, for whatever the last line left behind. */
            tt_macro_service(m, s);
            break;
        }
        if (now_ms() > deadline) {
            CHECK(0);
            break;
        }
        wait_readable(mfd, 10);
    }

    CHECK(log.errors == 0);
    CHECK(log.messages == 1);
    CHECK(strcmp(log.last_text, "hello world") == 0);
    CHECK(strcmp(log.last_title, "title") == 0);
    CHECK(log.inputs == 1);
    CHECK(log.list_choice == 1);
    CHECK(log.exit_code == 7);
    CHECK(tt_macro_exit_code(m) == 7);
    /* Both answers came back the other way: `listbox` puts the chosen index in
     * `result` and `inputbox` puts its text in `inputstr`, so the line the
     * macro printed locally is what the two callbacks returned. */
    expect_row(s, 0, "chose third as typed");

    tt_macro_free(m);
    tt_session_unlink_macro(s);
    tt_session_free(s);
    remove_macro();
}

/* A macro with no dialogs at all: every callback null, which the header says
 * is "Unknown command" rather than a crash or a silent success. */
static void test_macro_without_a_frontend(void)
{
    /* The error dialog is refused too, and a null `error` stops the macro —
     * so the `dispstr` below never runs. */
    const char *path = write_macro("messagebox 'x' 'y'\ndispstr 'ran on'\n");
    CHECK(path != NULL);
    if (!path)
        return;

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    const char *argv[] = {path, NULL};
    TtMacro *m = tt_macro_start(s, argv, NULL);
    CHECK(m != NULL);
    if (m) {
        long deadline = now_ms() + 10000;
        while (tt_macro_running(m) && now_ms() < deadline) {
            tt_macro_service(m, s);
            wait_readable(tt_macro_poll_fd(m), 10);
        }
        tt_macro_service(m, s);
        CHECK(!tt_macro_running(m));
        CHECK(tt_macro_exit_code(m) == 0);
        tt_macro_free(m);
    }
    /* Nothing was printed: the macro stopped at the refused `messagebox`. */
    CHECK(base(&tt_session_row(s, 0, NULL)[0]) == ' ');

    tt_session_free(s);
    remove_macro();
}

/* A macro that ends without asking for anything still has to wake its
 * frontend, because a frontend has no timer to fall back on. */
static void test_macro_ends_quietly(void)
{
    /* `pause` sleeps on the macro's own thread and posts no job, so nothing
     * this side hears a word until the thread knocks on its way out. */
    const char *path = write_macro("pause 1\n");
    CHECK(path != NULL);
    if (!path)
        return;

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    const char *argv[] = {path, NULL};
    TtMacro *m = tt_macro_start(s, argv, NULL);
    CHECK(m != NULL);
    if (m) {
        struct pollfd pfd = {tt_macro_poll_fd(m), POLLIN, 0};
        /* Five seconds against a one-second sleep: a pass here is the wakeup
         * arriving, and a timeout is the bug this test exists for. */
        CHECK(poll(&pfd, 1, 5000) > 0);
        tt_macro_service(m, s);
        CHECK(!tt_macro_running(m));
        tt_macro_free(m);
        tt_session_unlink_macro(s);
    }
    tt_session_free(s);
    remove_macro();
}

/* Freeing a macro that is still running is the window closing on a script,
 * and it has to end rather than deadlock against the join. */
static void test_macro_cancelled(void)
{
    const char *path = write_macro(":top\npause 3600\ngoto top\n");
    CHECK(path != NULL);
    if (!path)
        return;

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    const char *argv[] = {path, NULL};
    TtMacro *m = tt_macro_start(s, argv, NULL);
    CHECK(m != NULL);
    if (m) {
        CHECK(tt_macro_running(m));
        long start = now_ms();
        tt_macro_free(m); /* cancels, drops the channel, joins */
        /* `pause` is broken into polls precisely so this is milliseconds. */
        CHECK(now_ms() - start < 2000);
    }
    tt_session_free(s);
    remove_macro();
}

int main(void)
{
    printf("Sterna core %s\n", tt_version());
    test_screen();
    test_attributes();
    test_scrollback_viewport();
    test_absolute_lines();
    test_logging();
    test_settings();
    test_cmdline();
    test_input();
    test_palette();
    test_serial();
    test_ssh();
    test_telnet();
    test_pty();
    test_transfer();
    test_macro();
    test_macro_without_a_frontend();
    test_macro_ends_quietly();
    test_macro_cancelled();
    test_null_safety();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("ABI ok\n");
    return 0;
}
