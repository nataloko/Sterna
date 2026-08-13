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

#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

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

static size_t read_file(const char *path, char *buf, size_t cap)
{
    FILE *f = fopen(path, "rb");
    CHECK(f != NULL);
    if (!f || cap == 0)
        return 0;
    size_t got = fread(buf, 1, cap - 1, f);
    fclose(f);
    buf[got] = 0;
    return got;
}

static void test_i18n(void)
{
    TtI18n *ja = tt_i18n_load("../../vendor/lang/ja_JP.lng");
    CHECK(ja != NULL);
    if (!ja)
        return;

    size_t len = 123;
    const uint8_t *text =
        tt_i18n_text(ja, "Tera Term", "MENU_FILE", "fallback", &len);
    static const char japanese[] = "ファイル(&F)";
    CHECK(text != NULL);
    CHECK(len == sizeof japanese - 1);
    CHECK(text && memcmp(text, japanese, len) == 0);

    text = tt_i18n_text(ja, "Tera Term", "NO_SUCH_KEY", "source text", &len);
    CHECK(text != NULL && len == 11 && memcmp(text, "source text", len) == 0);
    CHECK(tt_i18n_text(ja, "Tera Term", "NO_SUCH_KEY", NULL, &len) == NULL);
    CHECK(len == 0);
    tt_i18n_free(ja);

    /* The result is a byte span, not a C string: upstream's file-dialog
     * filters use embedded NULs, and the ABI must not truncate them. */
    TtI18n *en = tt_i18n_load("../../vendor/lang/en_US.lng");
    CHECK(en != NULL);
    if (en) {
        text = tt_i18n_text(en, "Tera Term", "FILEDLG_OPEN_LOGFILE_FILTER",
                            NULL, &len);
        static const uint8_t filter[] = {'a', 'l', 'l', '(', '*', '.', '*', ')',
                                         0,   '*', '.', '*', 0,   0};
        CHECK(text != NULL && len == sizeof filter);
        CHECK(text && memcmp(text, filter, sizeof filter) == 0);
        tt_i18n_free(en);
    }

    CHECK(tt_i18n_load("/tmp/sterna-no-such-language.lng") == NULL);
}

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
    CHECK(tt_session_serial_baud(s) == 0);

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
    CHECK(cur.shape == TT_CURSOR_SHAPE_BLOCK);
    CHECK(!cur.nonblinking);

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

static void test_sixel(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 4;
    cfg.rows = 3;
    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);

    static const char image[] =
        "\033P7;1q\"1;1;2;6#2;2;100;0;0~~\033\\";
    tt_session_feed(s, (const uint8_t *)image, sizeof image - 1);

    const TtSixelImage *images = NULL;
    CHECK(tt_session_sixel_images(s, &images) == 1);
    CHECK(images != NULL);
    if (images) {
        CHECK(images[0].line == 0);
        CHECK(images[0].column == 0);
        CHECK(images[0].width == 2 && images[0].height == 6);
        CHECK(images[0].pixels_len == 2 * 6 * 4);
        CHECK(images[0].pixels != NULL);
        if (images[0].pixels) {
            CHECK(images[0].pixels[0] == 255);
            CHECK(images[0].pixels[1] == 0);
            CHECK(images[0].pixels[2] == 0);
            CHECK(images[0].pixels[3] == 255);
        }
    }

    /* A later cell write clears the image tile through the core rather than
     * leaving the painter to guess which output arrived first. */
    tt_session_feed(s, (const uint8_t *)"\033[1;1HX", 7);
    images = (const TtSixelImage *)1;
    CHECK(tt_session_sixel_images(s, &images) == 0);
    CHECK(images == NULL);
    tt_session_free(s);
}

static void test_remote_clipboard(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    const TtEvent *events = NULL;

    /* Access ships off but notification ships on, so a rejected request is
     * visible rather than silently discarded. */
    static const char denied[] = "\033]52;c;?\007";
    tt_session_feed(s, (const uint8_t *)denied, sizeof denied - 1);
    size_t n = tt_session_drain_events(s, &events);
    CHECK(n == 2);
    CHECK(events[0].kind == TT_EVENT_KIND_CLIPBOARD_READ_REJECTED);

    CHECK_OK(tt_session_set_setting(s, "clipboard.remote_access", "on"));
    tt_session_drain_events(s, &events); /* damage from applying settings */
    static const char allowed[] = "\033]52;c;?\007\033]52;p;aGk=\033\\";
    tt_session_feed(s, (const uint8_t *)allowed, sizeof allowed - 1);
    n = tt_session_drain_events(s, &events);
    /* The first settings application also gives the core its file title, so
     * the next feed reports that title edge alongside these three events. */
    CHECK(n == 4);
    CHECK(events[0].kind == TT_EVENT_KIND_CLIPBOARD_READ);
    CHECK(events[0].byte == 1);
    CHECK(events[0].text != NULL && strcmp(events[0].text, "c") == 0);
    CHECK(events[1].kind == TT_EVENT_KIND_CLIPBOARD_WRITE);
    CHECK(events[1].text != NULL && strcmp(events[1].text, "hi") == 0);

    /* There is no transport in this ABI unit, but this still crosses the C
     * seam and builds the exact reply before the disconnected session drops
     * it. */
    bool sent = false;
    CHECK_OK(tt_session_clipboard_reply(s, "c", "hé", SIZE_MAX, &sent));
    CHECK(sent);

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

    /* And the core moves the offset itself, which is why a frontend has to
     * re-read it rather than trusting what it last wrote. Which way it moves
     * is `AutoScrollOnlyInBottomLine`: shipped off, so output drags the view
     * back to the cursor, and on, so it holds the lines it was showing.
     * `tt-session`'s viewport tests have both; here the point is only that it
     * moved without anybody calling the setter. */
    tt_session_set_view_offset(s, 2);
    static const char more[] = "g\r\nh\r\n";
    tt_session_feed(s, (const uint8_t *)more, sizeof more - 1);
    CHECK(tt_session_view_offset(s) == 0);
    expect_row(s, 0, "f");

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

static void test_url_lookup(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 40;
    cfg.rows = 3;
    TtSession *s = tt_session_new(&cfg);

    static const char feed[] = "x http://example.test end";
    tt_session_feed(s, (const uint8_t *)feed, sizeof feed - 1);

    size_t len = 0;
    const TtCell *row = tt_session_row(s, 0, &len);
    CHECK(row != NULL && len == 40);
    CHECK((row[2].attrs & TT_ATTR_URL) != 0);
    CHECK(strcmp(tt_session_url_at(s, 0, 7), "http://example.test") == 0);
    CHECK(tt_session_url_at(s, 0, 0) == NULL);
    CHECK(tt_session_url_at(s, UINT64_MAX, 0) == NULL);
    CHECK(tt_session_url_at(s, 0, 40) == NULL);

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

/* The log's name is a template, and the frontend asks the core to expand it
 * rather than doing it itself — which is the whole point, since the rules are
 * a validator table, two `strftime` dialects and three `&`-escapes. */
static void test_log_name(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    /* Nothing connected: `&h` and `&p` expand to nothing at all, the way
     * `ConvertLognameW` does when `cv.Open` is false. */
    CHECK_OK(tt_session_set_setting(s, "log.default_path", "/tmp"));
    CHECK_OK(tt_session_set_setting(s, "log.default_name", "&h-&p.log"));
    const char *name = tt_session_log_name(s, NULL);
    CHECK(name != NULL);
    CHECK(strcmp(name, "/tmp/-.log") == 0);

    tt_session_set_connection_name(s, "router1", 2222);
    /* Still nothing: the escapes are gated on there being a connection, not on
     * something having been named. */
    CHECK(strcmp(tt_session_log_name(s, NULL), "/tmp/-.log") == 0);

    /* A relative request lands in the log directory, an absolute one does not,
     * and both go through the template. */
    CHECK(strcmp(tt_session_log_name(s, "out.log"), "/tmp/out.log") == 0);
    CHECK(strcmp(tt_session_log_name(s, "/var/tmp/a.log"), "/var/tmp/a.log") == 0);

    /* And starting a log with no options at all takes the settings' — which is
     * the ordinary call, and the one the window makes. */
    CHECK_OK(tt_session_set_setting(s, "log.default_name", "tt-ffi-abi-name.log"));
    const char *path = tt_session_log_name(s, NULL);
    CHECK(strcmp(path, "/tmp/tt-ffi-abi-name.log") == 0);
    CHECK_OK(tt_session_log_start(s, path, NULL));
    tt_session_feed(s, (const uint8_t *)"x\r\n", 3);
    tt_session_log_stop(s);
    remove("/tmp/tt-ffi-abi-name.log");

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

    /* A copied KEYBOARD.CNF is parsed in the core and returns only the actions
     * the window itself owns. The later internal key id wins a duplicate, not
     * whichever line happened to appear last in the file. */
    const char *keymap = "/tmp/tt-ffi-abi-keyboard.cnf";
    FILE *kf = fopen(keymap, "wb");
    CHECK(kf != NULL);
    if (kf) {
        fputs("[VT editor keypad]\nUp=328\nDown=328\n"
              "[Shortcut keys]\nEditPaste=850\n"
              "[User keys]\nUser1=1083,2,test.ttl\n"
              "User2=1084,3,50110tail\n",
              kf);
        fclose(kf);
    }
    CHECK_OK(tt_session_key_map_load(s, keymap));
    CHECK(tt_session_key_map_duplicate_count(s) == 1);
    CHECK(tt_session_key_map_duplicate(s, 0) == 328);
    CHECK(tt_session_key_map_duplicate(s, 1) == 0);

    TtKeyCodeResult action = {0};
    CHECK_OK(tt_session_send_key_code(s, 328, &action));
    CHECK(action.kind == TT_KEY_CODE_SENT);
    CHECK_OK(tt_session_send_key_code(s, 850, &action));
    CHECK(action.kind == TT_KEY_CODE_SHORTCUT);
    CHECK(action.value == TT_SHORTCUT_EDIT_PASTE);
    CHECK(action.text == NULL);
    CHECK_OK(tt_session_send_key_code(s, 1083, &action));
    CHECK(action.kind == TT_KEY_CODE_MACRO);
    CHECK(action.text != NULL && strcmp(action.text, "test.ttl") == 0);
    CHECK_OK(tt_session_send_key_code(s, 1084, &action));
    CHECK(action.kind == TT_KEY_CODE_COMMAND && action.value == 50110);
    CHECK_OK(tt_session_send_key_code(s, 999, &action));
    CHECK(action.kind == TT_KEY_CODE_UNMAPPED);
    CHECK_OK(tt_session_send_key_code(s, 999, NULL));
    remove(keymap);

    CHECK_OK(tt_session_send_text(s, "ls -l\r", SIZE_MAX));
    static const uint8_t raw[] = {0xE1, 0x00, 0xFF};
    CHECK_OK(tt_session_send_bytes(s, raw, sizeof raw));
    CHECK_OK(tt_session_send_bytes(s, NULL, 0));
    CHECK(tt_session_send_bytes(s, NULL, 1) == TT_ERR_INVALID);
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

    /* The old entry point is the immutable fallback for callers that do not
     * own a session. */
    CHECK(tt_palette_rgb(0, &r, &g, &b));
    CHECK(r == 0 && g == 0 && b == 0);
    /* The VGA values, not xterm's 205/238/229 — using xterm's moves the
     * answer for most truecolor input. */
    CHECK(tt_palette_rgb(1, &r, &g, &b));
    CHECK(r == 128 && g == 0 && b == 0);
    CHECK(tt_palette_rgb(255, &r, &g, &b));
    CHECK(r == 238 && g == 238 && b == 238);
    CHECK(!tt_palette_rgb(256, &r, &g, &b));

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);
    CHECK(tt_session_palette_rgb(s, 1, &r, &g, &b));
    CHECK(r == 128 && g == 0 && b == 0);

    CHECK_OK(tt_session_set_setting(
        s, "color.ansi_palette", "0,1,2,3,1,4,5,6"));
    CHECK(tt_session_palette_rgb(s, 0, &r, &g, &b));
    CHECK(r == 1 && g == 2 && b == 3);
    /* ANSIColor uses the legacy table order: its 1 becomes drawing index 9. */
    CHECK(tt_session_palette_rgb(s, 9, &r, &g, &b));
    CHECK(r == 4 && g == 5 && b == 6);

    /* The fallback is still the fallback, and null output pointers are fine. */
    CHECK(tt_palette_rgb(0, &r, &g, &b));
    CHECK(r == 0 && g == 0 && b == 0);
    CHECK(tt_session_palette_rgb(s, 0, NULL, NULL, NULL));
    CHECK(!tt_session_palette_rgb(s, 256, &r, &g, &b));
    CHECK(!tt_session_palette_rgb(NULL, 0, &r, &g, &b));
    tt_session_free(s);
}

static void test_window_ops(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 80;
    cfg.rows = 24;
    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);

    /* The reports go to the transport, which this unit has none of, so what
     * they answer is asserted in tt-vt and end to end by esctest. What
     * crosses the C seam is the snapshot going in and the actions coming
     * out. */
    TtWindowMetrics m;
    memset(&m, 0, sizeof m);
    m.x = 300; m.y = 120;
    m.client_x = 308; m.client_y = 156;
    m.width = 1288; m.height = 800;
    m.client_width = 1280; m.client_height = 768;
    m.cell_width = 16; m.cell_height = 32;
    m.screen_width = 2560; m.screen_height = 1440;
    m.iconified = true;
    tt_session_set_window_metrics(s, &m);
    /* Null is a no-op on both arguments rather than a crash. */
    tt_session_set_window_metrics(s, NULL);
    tt_session_set_window_metrics(NULL, &m);

    /* The actions come out as events and are read once for the batch. */
    tt_session_feed(s, (const uint8_t *)"\x1b[3;40;50t\x1b[2t", 14);
    const TtEvent *events = NULL;
    size_t nev = tt_session_drain_events(s, &events);
    int seen = 0;
    for (size_t i = 0; i < nev; i++) {
        if (events[i].kind == TT_EVENT_KIND_WINDOW_REQUEST) {
            seen++;
        }
    }
    CHECK(seen == 2);

    const TtWindowRequest *reqs = NULL;
    CHECK(tt_session_window_requests(s, &reqs) == 2);
    CHECK(reqs != NULL);
    CHECK(reqs[0].op == TT_WINDOW_OP_MOVE && reqs[0].x == 40 && reqs[0].y == 50);
    CHECK(reqs[1].op == TT_WINDOW_OP_ICONIFY);
    /* Reading does not consume: a frontend may see several of the events. */
    CHECK(tt_session_window_requests(s, &reqs) == 2);

    /* ...but the next event drain replaces the batch. */
    CHECK(tt_session_drain_events(s, &events) == 0);
    CHECK(tt_session_window_requests(s, &reqs) == 0);
    CHECK(reqs == NULL);

    tt_session_free(s);
}

static void test_printer(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    cfg.cols = 24;
    cfg.rows = 4;
    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);

    /* PrinterCtrlSequence ships off, so four of the five sequences do
     * nothing until the file turns them on. */
    tt_session_feed(s, (const uint8_t *)"\x1b[5iA\x1b[4i", 10);
    const TtEvent *events = NULL;
    tt_session_drain_events(s, &events);
    const TtPrinterEvent *jobs = NULL;
    CHECK(tt_session_printer_events(s, &jobs) == 0);
    CHECK(jobs == NULL);

    CHECK(tt_session_set_setting(s, "printer.control_sequences", "on") == TT_OK);
    tt_session_drain_events(s, &events);

    /* A whole job: open, the controls the terminal did not execute, close.
     * And a print-screen request, which carries no bytes at all. */
    tt_session_feed(s, (const uint8_t *)"\x1b[5iB\r\n\x1b[4i\x1b[0i", 16);
    size_t nev = tt_session_drain_events(s, &events);
    int seen = 0;
    for (size_t i = 0; i < nev; i++) {
        if (events[i].kind == TT_EVENT_KIND_PRINTER) {
            seen++;
        }
    }
    CHECK(seen == 4);

    CHECK(tt_session_printer_events(s, &jobs) == 4);
    CHECK(jobs != NULL);
    CHECK(jobs[0].op == TT_PRINTER_OP_OPEN && jobs[0].text == NULL);
    CHECK(jobs[1].op == TT_PRINTER_OP_WRITE);
    CHECK(jobs[1].text != NULL && strcmp(jobs[1].text, "B\r\n") == 0);
    CHECK(jobs[2].op == TT_PRINTER_OP_CLOSE);
    /* DECPEX defaults set, so the whole screen rather than the region. The
     * frontend needs the region's rows to honour the other answer. */
    CHECK(jobs[3].op == TT_PRINTER_OP_SCREEN && jobs[3].scroll_region == 0);
    /* The whole screen until DECSTBM says otherwise. Against the live row
     * count, not `cfg.rows`: applying a setting applies all of them, and the
     * terminal size is one of them. */
    size_t top = 99, bottom = 99;
    tt_session_scroll_region(s, &top, &bottom);
    CHECK(top == 0 && bottom == tt_session_rows(s) - 1);
    /* Reading does not consume, and the next drain replaces the batch. */
    CHECK(tt_session_printer_events(s, &jobs) == 4);
    CHECK(tt_session_drain_events(s, &events) == 0);
    CHECK(tt_session_printer_events(s, &jobs) == 0);
    CHECK(jobs == NULL);

    /* ...and DECSTBM moves it, which is the answer `CSI 0 i` needs when
     * DECPEX is reset. Inclusive rows, zero-based. */
    tt_session_feed(s, (const uint8_t *)"\x1b[2;3r", 6);
    tt_session_scroll_region(s, &top, &bottom);
    CHECK(top == 1 && bottom == 2);

    tt_session_free(s);
}

static void test_serial(void)
{
    TtSerialParams p;
    tt_serial_params_default(&p);
    /* 115200, not upstream's 9600 — docs/deviations.md. */
    CHECK(p.baud == 115200);
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
    CHECK(tt_session_wait_handle(s) == NULL);

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

    /* Duplicate session copies the in-memory values and the live grid size,
     * rather than reopening the source file and losing either kind of edit. */
    TtSession *copy = tt_session_new(&cfg);
    CHECK(copy != NULL);
    CHECK_OK(tt_session_set_setting(s, "terminal.title", "copied live"));
    CHECK_OK(tt_session_resize(s, 117, 43));
    CHECK_OK(tt_session_copy_settings(copy, s));
    CHECK(strcmp(tt_session_setting(copy, "terminal.title"), "copied live")
          == 0);
    CHECK(tt_session_cols(copy) == 117);
    CHECK(tt_session_rows(copy) == 43);
    CHECK_OK(tt_session_copy_settings(s, s));
    tt_session_free(copy);

    /* Cursor style is live state: once the file permits the control sequence,
     * the frontend sees the host's shape and blink choice rather than the
     * stale setting. DECSCUSR 4 is a steady underline. */
    CHECK_OK(tt_session_set_setting(s, "window.cursor_ctrl_allowed", "on"));
    static const char cursor_style[] = "\033[4 q";
    tt_session_feed(s, (const uint8_t *)cursor_style, sizeof cursor_style - 1);
    TtCursor cur;
    tt_session_cursor(s, &cur);
    CHECK(cur.shape == TT_CURSOR_SHAPE_HORIZONTAL);
    CHECK(cur.nonblinking);

    /* Shift+Escape cycles through only the modes admitted by DebugModes. The
     * frontend owns the key event, but the core owns this state and the raw
     * receive path it selects. */
    TtSession *debug = tt_session_new(&cfg);
    CHECK(debug != NULL);
    CHECK_OK(tt_session_set_setting(debug, "debug.enabled", "on"));
    CHECK_OK(tt_session_set_setting(debug, "debug.modes", "hex"));
    CHECK(tt_session_cycle_debug_mode(debug));
    static const char debug_feed[] = "\033[A";
    tt_session_feed(debug, (const uint8_t *)debug_feed, sizeof debug_feed - 1);
    size_t debug_len = 0;
    const TtCell *debug_row = tt_session_row(debug, 0, &debug_len);
    CHECK(debug_row != NULL && debug_len >= 8);
    if (debug_row && debug_len >= 8) {
        CHECK(base(&debug_row[0]) == '1');
        CHECK(base(&debug_row[1]) == 'B');
        CHECK(base(&debug_row[3]) == '5');
        CHECK(base(&debug_row[4]) == 'B');
        CHECK(base(&debug_row[6]) == '4');
        CHECK(base(&debug_row[7]) == '1');
    }
    CHECK(tt_session_cycle_debug_mode(debug));
    tt_session_free(debug);

    /* A round trip through a file, including a key nothing here knows about:
     * a TERATERM.INI shared with a real Tera Term has to survive being
     * written back. */
    const char *path = "/tmp/tt-ffi-abi-settings.ini";
    FILE *f2 = fopen(path, "wb");
    CHECK(f2 != NULL);
    if (f2) {
        fputs("; a comment\r\n[Tera Term]\r\nTerminalSize=100,40\r\n"
              "VTPos='12,34'\r\n"
              "SomethingElse=kept\r\n",
              f2);
        fclose(f2);
    }
    CHECK_OK(tt_session_settings_load(s, path));
    CHECK(tt_session_cols(s) == 100);
    CHECK(tt_session_rows(s) == 40);
    CHECK(strcmp(tt_session_setting(s, "terminal.id"), "VT100") == 0);

    CHECK_OK(tt_session_set_setting(s, "terminal.title", "sterna"));
    /* The grid is live state and the Settings struct is the last load. A save
     * must take the first or resizing the window writes the old size back. */
    CHECK_OK(tt_session_resize(s, 90, 30));
    CHECK_OK(tt_session_settings_save(s, path));
    char buf[65536] = {0};
    CHECK(read_file(path, buf, sizeof buf) > 0);
    CHECK(strstr(buf, "; a comment") != NULL);
    CHECK(strstr(buf, "SomethingElse=kept") != NULL);
    CHECK(strstr(buf, "Title=sterna") != NULL);
    CHECK(strstr(buf, "TerminalSize=90,30") != NULL);
    /* SaveVTWinPos is off, so even the quotes on a line we did not own stay. */
    CHECK(strstr(buf, "VTPos='12,34'") != NULL);

    CHECK_OK(tt_session_set_setting(s, "window.save_position", "on"));
    CHECK_OK(tt_session_settings_save_for_window(s, path, 56, 78, true));
    CHECK(read_file(path, buf, sizeof buf) > 0);
    CHECK(strstr(buf, "VTPos=56,78") != NULL);
    /* Wayland has no client-owned position. Its confident-looking (0,0) must
     * not replace the last useful coordinates. */
    CHECK_OK(tt_session_settings_save_for_window(s, path, 0, 0, false));
    CHECK(read_file(path, buf, sizeof buf) > 0);
    CHECK(strstr(buf, "VTPos=56,78") != NULL);

    /* Closing writes geometry and nothing else. A setting changed only in
     * memory must not be pinned into the file by this smaller save. */
    CHECK_OK(tt_session_set_setting(s, "terminal.title", "not-written-on-close"));
    CHECK_OK(tt_session_resize(s, 91, 31));
    CHECK_OK(tt_session_window_geometry_save(s, path, 90, 91, true));
    CHECK(read_file(path, buf, sizeof buf) > 0);
    CHECK(strstr(buf, "VTPos=90,91") != NULL);
    CHECK(strstr(buf, "TerminalSize=91,31") != NULL);
    CHECK(strstr(buf, "Title=sterna") != NULL);
    CHECK(strstr(buf, "not-written-on-close") == NULL);
    remove(path);

    /* A file that is not there is a first run, not a failure. */
    CHECK_OK(tt_session_settings_load(s, "/tmp/tt-ffi-abi-no-such.ini"));
    CHECK(tt_session_cols(s) == 80);

    tt_session_free(s);
}

/* What Sterna remembers about the last connection, which Tera Term does not —
 * see docs/deviations.md. The frontend's whole part in it is these two calls,
 * so this is the test that says what they promise. */
static void test_remembered_connection(void)
{
    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    CHECK(s != NULL);

    const char *path = "/tmp/tt-ffi-abi-recent.ini";
    remove(path);
    FILE *f = fopen(path, "wb");
    CHECK(f != NULL);
    if (f) {
        fputs("; a comment\r\n[Tera Term]\r\nBaudRate=9600\r\n"
              "SomethingElse=kept\r\n",
              f);
        fclose(f);
    }
    CHECK_OK(tt_session_settings_load(s, path));

    /* The line settings a connect dialog opens at are the file's, not the
     * shipped ones — the deviation's other half. */
    TtSerialParams shipped;
    tt_serial_params_default(&shipped);
    CHECK(shipped.baud == 115200);
    TtSerialParams configured;
    tt_session_serial_params(s, &configured);
    CHECK(configured.baud == 9600);
    CHECK(configured.data_bits == 8 && configured.stop_bits == 1);
    CHECK(configured.parity == TT_PARITY_NONE);
    CHECK(configured.flow == TT_FLOW_CONTROL_NONE);

    /* A setting changed in memory only, to prove the remember does not sweep
     * it up the way a full save would. */
    CHECK_OK(tt_session_set_setting(s, "terminal.title", "not-remembered"));

    const TtSettingValue values[] = {
        {"recent.serial_port", "/dev/serial/by-id/usb-FTDI-if00-port0"},
        {"serial.baud", "57600"},
        {"serial.flow", "hard"},
    };
    CHECK_OK(tt_session_settings_remember(s, path, values, 3));

    char buf[65536] = {0};
    CHECK(read_file(path, buf, sizeof buf) > 0);
    CHECK(strstr(buf, "[Sterna]") != NULL);
    CHECK(strstr(buf, "SerialPort=/dev/serial/by-id/usb-FTDI-if00-port0") != NULL);
    CHECK(strstr(buf, "BaudRate=57600") != NULL);
    CHECK(strstr(buf, "FlowCtrl=hard") != NULL);
    CHECK(strstr(buf, "; a comment") != NULL);
    CHECK(strstr(buf, "SomethingElse=kept") != NULL);
    CHECK(strstr(buf, "not-remembered") == NULL);

    /* Applied as well as written, so the settings dialog and a macro's
     * getsetting report the speed the port is running at. */
    CHECK(strcmp(tt_session_setting(s, "serial.baud"), "57600") == 0);
    tt_session_serial_params(s, &configured);
    CHECK(configured.baud == 57600);
    CHECK(configured.flow == TT_FLOW_CONTROL_RTS_CTS);

    /* And it must not resize the terminal. Applying settings takes the grid
     * from `TerminalSize`, whose schema copy is a snapshot from the last load —
     * so a window the user has dragged, or a host's `CSI 8 t`, must survive a
     * connection remembering its own speed. The file must not learn the new
     * size either: this save owns the named keys and nothing else. */
    CHECK_OK(tt_session_resize(s, 132, 50));
    const TtSettingValue resized[] = {{"serial.baud", "38400"}};
    CHECK_OK(tt_session_settings_remember(s, path, resized, 1));
    CHECK(tt_session_cols(s) == 132);
    CHECK(tt_session_rows(s) == 50);
    CHECK(read_file(path, buf, sizeof buf) > 0);
    CHECK(strstr(buf, "BaudRate=38400") != NULL);
    CHECK(strstr(buf, "TerminalSize") == NULL);
    CHECK_OK(tt_session_settings_remember(s, path, values, 3));

    /* Reconnecting to the same port must not rewrite the file. The inode is
     * the check rather than the bytes: a rewrite renames a temporary over the
     * old file, so an unchanged *content* would still be a new inode. */
    struct stat before;
    CHECK(stat(path, &before) == 0);
    CHECK_OK(tt_session_settings_remember(s, path, values, 3));
    struct stat after;
    CHECK(stat(path, &after) == 0);
    CHECK(before.st_ino == after.st_ino);

    /* And one name the schema does not have refuses the whole call: neither
     * the file nor the session takes the half that was valid. */
    const TtSettingValue bad[] = {
        {"serial.baud", "19200"},
        {"recent.no_such_setting", "x"},
    };
    CHECK(tt_session_settings_remember(s, path, bad, 2) == TT_ERR_INVALID);
    CHECK(strcmp(tt_session_setting(s, "serial.baud"), "57600") == 0);
    CHECK(stat(path, &after) == 0);
    CHECK(before.st_ino == after.st_ino);

    /* A second session reading the file back is the next launch: the dialog
     * opens where the last connection left off. */
    TtSession *next = tt_session_new(&cfg);
    CHECK(next != NULL);
    CHECK_OK(tt_session_settings_load(next, path));
    CHECK(strcmp(tt_session_setting(next, "recent.serial_port"),
                 "/dev/serial/by-id/usb-FTDI-if00-port0")
          == 0);
    tt_session_serial_params(next, &configured);
    CHECK(configured.baud == 57600);
    CHECK(configured.flow == TT_FLOW_CONTROL_RTS_CTS);
    /* Nothing was remembered about SSH, and an empty host is how that is
     * spelled — not a zero-length host name. */
    CHECK(strcmp(tt_session_setting(next, "recent.ssh_host"), "") == 0);
    tt_session_free(next);

    /* Nothing at all is a no-op rather than an error: a frontend with an empty
     * record should not have to special-case the call. */
    CHECK_OK(tt_session_settings_remember(s, path, NULL, 0));
    CHECK(tt_session_settings_remember(NULL, path, values, 3) == TT_ERR_INVALID);
    CHECK(tt_session_settings_remember(s, path, NULL, 1) == TT_ERR_INVALID);

    remove(path);
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
     * negotiates and anything else frames without offering a word, because a
     * terminal server's per-line port is not a telnet server. */
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
    CHECK(st.telnet.mode == TT_TELNET_FRAMED);
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
    const char *opts[] = {"/F=other.ini", "/K=keys.cnf", "/L=out.log",
                          "/M=setup.ttl", "/I",          "/X=100",
                          "/DS"};
    TtCmdLine *cmd = tt_cmdline_parse(opts, 7, 0);
    CHECK(cmd != NULL);
    TtCmdLineInfo info;
    CHECK(tt_cmdline_info(cmd, &info));
    CHECK(info.setup_file && strcmp(info.setup_file, "other.ini") == 0);
    CHECK(info.key_cnf_file && strcmp(info.key_cnf_file, "keys.cnf") == 0);
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
    uint8_t signature[64] = {0};
    CHECK(!tt_update_verify(NULL, 1, signature, sizeof signature));
    CHECK(!tt_update_verify(NULL, 0, NULL, 0));
    CHECK(tt_i18n_load(NULL) == NULL);
    CHECK(tt_i18n_text(NULL, "Tera Term", "MENU_FILE", "File", NULL) == NULL);
    tt_i18n_free(NULL);
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
    CHECK(tt_session_wait_handle(NULL) == NULL);
    CHECK(tt_session_key_map_load(NULL, "/tmp/x.cnf") == TT_ERR_INVALID);
    CHECK(tt_session_key_map_duplicate_count(NULL) == 0);
    CHECK(tt_session_key_map_duplicate(NULL, 0) == 0);
    CHECK(tt_session_send_key_code(NULL, 1, NULL) == TT_ERR_INVALID);
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
    CHECK(tt_session_url_at(NULL, 0, 0) == NULL);
    CHECK(tt_session_pending_out(NULL) == 0);
    /* Null answers false, which happens to be the non-default — a frontend
     * that lost its session sends DEL rather than reading through a null. */
    CHECK(!tt_session_backspace_sends_bs(NULL));
    CHECK(tt_session_send_text(NULL, "x", 1) == TT_ERR_INVALID);
    CHECK(tt_session_send_bytes(NULL, NULL, 0) == TT_ERR_INVALID);
    CHECK(tt_session_focus(NULL, true) == TT_ERR_INVALID);
    CHECK(tt_session_send_break(NULL) == TT_ERR_INVALID);
    tt_session_feed(NULL, NULL, 0);
    CHECK(tt_session_sixel_images(NULL, NULL) == 0);
    CHECK(!tt_session_cycle_debug_mode(NULL));
    tt_session_disconnect(NULL);
    CHECK(!tt_session_is_connected(NULL));
    CHECK(tt_session_describe(NULL) == NULL);
    CHECK(tt_port_list_len(NULL) == 0);
    CHECK(tt_port_list_at(NULL, 0) == NULL);
    tt_port_list_free(NULL);
    tt_ssh_params_default(NULL);
    CHECK(tt_ssh_connect(NULL) == NULL);
    CHECK(tt_ssh_connect_for_session(NULL, NULL) == NULL);
    CHECK(tt_ssh_connect_poll_fd(NULL) == -1);
    CHECK(tt_ssh_connect_wait_handle(NULL) == NULL);
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
    CHECK(tt_session_copy_settings(NULL, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_settings_save(NULL, "/tmp/x.ini") == TT_ERR_INVALID);
    CHECK(tt_session_settings_save_for_window(NULL, "/tmp/x.ini", 0, 0, true)
          == TT_ERR_INVALID);
    CHECK(tt_session_window_geometry_save(NULL, "/tmp/x.ini", 0, 0, true)
          == TT_ERR_INVALID);

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
    CHECK(tt_macro_wait_handle(NULL) == NULL);
    CHECK(tt_ctl_wait_handle(NULL) == NULL);
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
    CHECK(tt_session_copy_settings(s, NULL) == TT_ERR_INVALID);
    CHECK(tt_session_copy_settings(NULL, s) == TT_ERR_INVALID);
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
    TtConfig probe_cfg;
    tt_config_default(&probe_cfg);
    TtSession *session = tt_session_new(&probe_cfg);
    CHECK(session != NULL);
    TtSshConnect *c = tt_ssh_connect_for_session(&p, session);
    CHECK(c != NULL);
    if (c) {
        CHECK(tt_ssh_connect_poll_fd(c) >= 0);
        CHECK(tt_ssh_connect_wait_handle(c) == NULL);
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
    tt_session_free(session);

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

    c = tt_ssh_connect_for_session(&p, s);
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
     * mode comes from the port, and the answer away from 23 is FRAMED rather
     * than AUTO — the framing is on from the first byte, it is only the
     * opening burst that is held back. */
    tt_telnet_params_default(&p, 23);
    CHECK(p.mode == TT_TELNET_NEGOTIATE);
    tt_telnet_params_default(&p, 2001);
    CHECK(p.mode == TT_TELNET_FRAMED);
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

/* A persistent Lua plugin set over the same worker/frontend seam as macros.
 * The callback prints locally so servicing it proves both directions: the
 * worker asked for a host call, and the frontend applied it to this session. */
static void test_plugins(void)
{
    char dir[] = "/tmp/sterna-abi-plugin-XXXXXX";
    if (mkdtemp(dir) == NULL) {
        CHECK(0);
        return;
    }

    char path[512];
    snprintf(path, sizeof path, "%s/10-counter.lua", dir);
    FILE *file = fopen(path, "wb");
    CHECK(file != NULL);
    if (!file) {
        rmdir(dir);
        return;
    }
    const char source[] =
        "local count = 0\n"
        "local preferences = sterna.settings { title = 'ABI plugin', "
        "section = 'Lua ABI', fields = {\n"
        "  { name = 'enabled', label = 'Enabled', kind = 'bool', default = true },\n"
        "  { name = 'retries', label = 'Retries', kind = 'int', min = 1, max = 9, default = 3 },\n"
        "  { name = 'prefix', key = 'PromptPrefix', label = 'Prefix', "
        "description = 'Input prefix', kind = 'string', default = 'default:' },\n"
        "  { name = 'mode', label = 'Mode', kind = 'enum', "
        "choices = {'fast', 'safe'}, default = 'fast' },\n"
        "} }\n"
        "local loaded_prefix = preferences.prefix\n"
        "local inbound\n"
        "inbound = sterna.filter('input', function(bytes)\n"
        "  if bytes == 'probe' then return inbound.replacement end\n"
        "  if bytes == 'setting' then return loaded_prefix .. preferences.prefix end\n"
        "  if bytes == 'boom' then error('broken filter') end\n"
        "  return bytes\n"
        "end)\n"
        "inbound.replacement = 'before'\n"
        "sterna.menu { menu = 'Control/ABI', label = 'Count', "
        "shortcut = 'Ctrl+Alt+C', action = function()\n"
        "  count = count + 1; inbound.replacement = 'after'; print('menu ' .. count)\n"
        "end }\n"
        "sterna.key('Ctrl+Alt+K', function() print('key') end)\n"
        "sterna.on('connect', function(event) print(event) end)\n";
    CHECK(fwrite(source, 1, sizeof source - 1, file) == sizeof source - 1);
    fclose(file);

    char settings_path[512];
    snprintf(settings_path, sizeof settings_path, "%s/sterna.ini", dir);
    file = fopen(settings_path, "wb");
    CHECK(file != NULL);
    if (!file) {
        unlink(path);
        rmdir(dir);
        return;
    }
    const char saved_settings[] =
        "[Lua ABI]\nEnabled=off\nRetries=7\nPromptPrefix=saved:\nMode=safe\n";
    CHECK(fwrite(saved_settings, 1, sizeof saved_settings - 1, file)
          == sizeof saved_settings - 1);
    fclose(file);

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    struct ui_log log = {0};
    TtMacroUi ui = {0};
    ui.user = &log;
    ui.error = on_error;

    TtPlugins *plugins =
        tt_plugins_load_with_settings(s, dir, settings_path, &ui);
    CHECK(plugins != NULL);
    if (!plugins) {
        fprintf(stderr, "  %s\n", tt_last_error());
        tt_session_free(s);
        unlink(path);
        rmdir(dir);
        return;
    }
    CHECK(tt_plugins_action_count(plugins) == 2);
    CHECK(tt_plugins_setting_count(plugins) == 4);
    CHECK(tt_plugins_poll_fd(plugins) >= 0);
    CHECK(tt_plugins_wait_handle(plugins) == NULL);

    /* The filter state is live before any asynchronous callback. Clear the
     * screen afterwards so the existing callback row assertions stay exact. */
    tt_session_feed(s, (const uint8_t *)"probe", 5);
    expect_row(s, 0, "before");
    tt_session_feed(s, (const uint8_t *)"\033[2J\033[H", 7);
    const TtEvent *discard = NULL;
    tt_session_drain_events(s, &discard);

    TtPluginAction menu = {0};
    TtPluginAction key = {0};
    CHECK(tt_plugins_action(plugins, 0, &menu));
    CHECK(menu.id == 0 && menu.kind == TT_PLUGIN_ACTION_MENU);
    CHECK(strcmp(menu.plugin, "10-counter.lua") == 0);
    CHECK(strcmp(menu.menu, "Control/ABI") == 0);
    CHECK(strcmp(menu.label, "Count") == 0);
    CHECK(strcmp(menu.shortcut, "Ctrl+Alt+C") == 0);
    CHECK(tt_plugins_action(plugins, 1, &key));
    CHECK(key.kind == TT_PLUGIN_ACTION_KEY);
    CHECK(key.menu == NULL && key.label == NULL);
    CHECK(strcmp(key.shortcut, "Ctrl+Alt+K") == 0);
    CHECK(!tt_plugins_action(plugins, 2, &key));

    TtPluginSetting enabled = {0};
    TtPluginSetting retries = {0};
    TtPluginSetting prefix = {0};
    TtPluginSetting mode = {0};
    CHECK(tt_plugins_setting(plugins, 0, &enabled));
    CHECK(tt_plugins_setting(plugins, 1, &retries));
    CHECK(tt_plugins_setting(plugins, 2, &prefix));
    CHECK(tt_plugins_setting(plugins, 3, &mode));
    CHECK(!tt_plugins_setting(plugins, 4, &mode));
    CHECK(enabled.id == 0 && enabled.page_id == 0);
    CHECK(strcmp(enabled.plugin, "10-counter.lua") == 0);
    CHECK(strcmp(enabled.page, "ABI plugin") == 0);
    CHECK(strcmp(enabled.section, "Lua ABI") == 0);
    CHECK(strcmp(enabled.name, "enabled") == 0);
    CHECK(strcmp(enabled.key, "enabled") == 0);
    CHECK(strcmp(enabled.label, "Enabled") == 0);
    CHECK(enabled.kind == TT_SETTING_KIND_BOOL);
    CHECK(strcmp(tt_plugins_setting_value(plugins, enabled.id), "off") == 0);
    CHECK(retries.kind == TT_SETTING_KIND_INT_RANGE);
    CHECK(retries.min == 1 && retries.max == 9);
    CHECK(strcmp(tt_plugins_setting_value(plugins, retries.id), "7") == 0);
    CHECK(prefix.kind == TT_SETTING_KIND_STR);
    CHECK(strcmp(prefix.key, "PromptPrefix") == 0);
    CHECK(strcmp(prefix.description, "Input prefix") == 0);
    CHECK(strcmp(prefix.default_value, "default:") == 0);
    CHECK(strcmp(tt_plugins_setting_value(plugins, prefix.id), "saved:") == 0);
    CHECK(mode.kind == TT_SETTING_KIND_ENUM && mode.choices == 2);
    CHECK(strcmp(tt_plugins_setting_choice(plugins, mode.id, 0), "fast") == 0);
    CHECK(strcmp(tt_plugins_setting_choice(plugins, mode.id, 1), "safe") == 0);
    CHECK(tt_plugins_setting_choice(plugins, mode.id, 2) == NULL);
    CHECK(strcmp(tt_plugins_setting_value(plugins, mode.id), "safe") == 0);
    CHECK(tt_plugins_set_setting(plugins, retries.id, "10") == TT_ERR_INVALID);
    CHECK(tt_plugins_set_setting(plugins, prefix.id, "live:") == TT_OK);

    /* The saved value was visible before the rest of the top level copied it,
     * while the stream VM sees the live value through the shared proxy. */
    tt_session_feed(s, (const uint8_t *)"setting", 7);
    expect_row(s, 0, "saved:live:");
    tt_session_feed(s, (const uint8_t *)"\033[2J\033[H", 7);

    const int fd = tt_plugins_poll_fd(plugins);
    CHECK(tt_plugins_invoke(plugins, menu.id) == TT_OK);
    /* The first callback blocks on `print` until this thread services it, so
     * the immediate second invocation is deterministically busy. */
    CHECK(tt_plugins_invoke(plugins, menu.id) == TT_ERR_BUSY);
    /* Lifecycle edges are not user actions: losing one while a callback is
     * active would leave the plugin's connection state wrong. It queues. */
    CHECK(tt_plugins_emit(plugins, TT_PLUGIN_HOOK_CONNECT) == TT_OK);
    long deadline = now_ms() + 10000;
    while (tt_plugins_busy(plugins) && now_ms() < deadline) {
        wait_readable(fd, 10);
        tt_plugins_service(plugins, s);
    }
    tt_plugins_service(plugins, s);
    CHECK(!tt_plugins_busy(plugins));
    expect_row(s, 0, "menu 1");
    expect_row(s, 1, "connect");

    CHECK(tt_plugins_invoke(plugins, menu.id) == TT_OK);
    deadline = now_ms() + 10000;
    while (tt_plugins_busy(plugins) && now_ms() < deadline) {
        wait_readable(fd, 10);
        tt_plugins_service(plugins, s);
    }
    tt_plugins_service(plugins, s);
    /* The Lua VM survived the first action, including its closure state. */
    expect_row(s, 2, "menu 2");
    CHECK(tt_plugins_emit(plugins, TT_PLUGIN_HOOK_DISCONNECT) == TT_OK);
    CHECK(!tt_plugins_busy(plugins));
    CHECK(log.errors == 0);

    /* The menu callback changed only the ordinary VM's control proxy. The
     * isolated stream VM sees the shared scalar without sharing a Lua state. */
    tt_session_feed(s, (const uint8_t *)"\033[2J\033[H", 7);
    tt_session_feed(s, (const uint8_t *)"probe", 5);
    expect_row(s, 0, "after");

    /* A filter failure is visible but fail-open: `boom` reaches the grid and
     * the callback is disabled for subsequent chunks. */
    tt_session_feed(s, (const uint8_t *)"boom", 4);
    const TtEvent *events = NULL;
    const size_t event_count = tt_session_drain_events(s, &events);
    bool saw_filter_error = false;
    for (size_t i = 0; i < event_count; i++) {
        if (events[i].kind == TT_EVENT_KIND_STREAM_FILTER_FAILED) {
            saw_filter_error = events[i].text != NULL
                               && strstr(events[i].text, "broken filter") != NULL;
        }
    }
    CHECK(saw_filter_error);
    expect_row(s, 0, "afterboom");

    CHECK(tt_plugins_settings_save(plugins, settings_path) == TT_OK);
    TtSession *copy_session = tt_session_new(&cfg);
    TtPlugins *copy = tt_plugins_load(copy_session, dir, &ui);
    CHECK(copy != NULL);
    if (copy) {
        CHECK(strcmp(tt_plugins_setting_value(copy, prefix.id), "default:") == 0);
        CHECK(tt_plugins_copy_settings(copy, plugins) == TT_OK);
        CHECK(strcmp(tt_plugins_setting_value(copy, prefix.id), "live:") == 0);
        CHECK(strcmp(tt_plugins_setting_value(copy, enabled.id), "off") == 0);
        tt_plugins_free(copy);
        tt_session_unlink_plugins(copy_session);
    }
    tt_session_free(copy_session);

    TtSession *reloaded_session = tt_session_new(&cfg);
    TtPlugins *reloaded =
        tt_plugins_load_with_settings(reloaded_session, dir, settings_path, &ui);
    CHECK(reloaded != NULL);
    if (reloaded) {
        CHECK(strcmp(tt_plugins_setting_value(reloaded, prefix.id), "live:") == 0);
        tt_plugins_free(reloaded);
        tt_session_unlink_plugins(reloaded_session);
    }
    tt_session_free(reloaded_session);

    tt_plugins_free(plugins);
    tt_session_unlink_plugins(s);
    tt_session_free(s);
    unlink(settings_path);
    unlink(path);
    rmdir(dir);
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
    CHECK(tt_macro_wait_handle(m) == NULL);
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


/* --- the control socket -------------------------------------------------
 *
 * The half of the ABI that DDE used to be. A frontend fills in TtCtlHost,
 * waits on tt_ctl_poll_fd and calls tt_ctl_service; this drives the other end
 * too, with a raw socket and sprintf, because "a shell script can do this" is
 * the claim the whole design rests on and a C test is the closest thing to it
 * in this suite.
 */

struct ctl_log {
    int macros;
    char last_macro[512];
    char last_param[128];
    int connects;
    char last_line[256];
    int closed;
    int running_left;
};

static TtStatus on_ctl_run_macro(void *user, const char *const *argv,
                                 const char **error)
{
    struct ctl_log *log = user;
    if (!argv || !argv[0]) {
        *error = "no macro named";
        return TT_ERR_INVALID;
    }
    if (log->running_left > 0) {
        /* Upstream raises the running macro's window instead; a socket has no
         * window to raise, so the client is told to try again. */
        *error = "a macro is already running";
        return TT_ERR_BUSY;
    }
    log->macros++;
    snprintf(log->last_macro, sizeof log->last_macro, "%s", argv[0]);
    snprintf(log->last_param, sizeof log->last_param, "%s",
             argv[1] ? argv[1] : "");
    log->running_left = 1;
    return TT_OK;
}

static bool on_ctl_macro_running(void *user)
{
    struct ctl_log *log = user;
    bool running = log->running_left > 0;
    if (log->running_left > 0)
        log->running_left--;
    return running;
}

static int32_t on_ctl_macro_exit_code(void *user)
{
    (void)user;
    return 5;
}

static TtStatus on_ctl_connect(void *user, const char *line,
                               const char **error)
{
    struct ctl_log *log = user;
    if (!line || !*line) {
        *error = "nothing to connect to";
        return TT_ERR_INVALID;
    }
    log->connects++;
    snprintf(log->last_line, sizeof log->last_line, "%s", line);
    return TT_OK;
}

static bool on_ctl_close(void *user)
{
    struct ctl_log *log = user;
    log->closed++;
    return true;
}

static const char *on_ctl_title(void *user)
{
    (void)user;
    return "a window";
}

/* Send one request and pump until its answer arrives, which is what a
 * frontend's event loop does for it. Returns 0 on success. */
static int ctl_call(TtCtl *c, TtSession *s, int fd, const char *req, char *out,
                    size_t out_len)
{
    if (write(fd, req, strlen(req)) < 0)
        return -1;
    size_t got = 0;
    long deadline = now_ms() + 10000;
    for (;;) {
        tt_ctl_service(c, s);
        struct pollfd pfd = {fd, POLLIN, 0};
        if (poll(&pfd, 1, 10) > 0) {
            ssize_t n = read(fd, out + got, out_len - got - 1);
            if (n <= 0)
                return -1;
            got += (size_t)n;
            out[got] = 0;
            if (memchr(out, '\n', got))
                return 0;
        }
        if (now_ms() > deadline)
            return -1;
    }
}

static void test_ctl(void)
{
    /* Its own runtime directory, so this cannot find — or prune — a window the
     * developer has open. */
    char dir[] = "/tmp/sterna-abi-XXXXXX";
    CHECK(mkdtemp(dir) != NULL);
    setenv("XDG_RUNTIME_DIR", dir, 1);

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);
    tt_session_feed(s, (const uint8_t *)"hello", 5);

    struct ctl_log log = {0};
    TtCtlHost host = {0};
    host.user = &log;
    host.run_macro = on_ctl_run_macro;
    host.macro_running = on_ctl_macro_running;
    host.macro_exit_code = on_ctl_macro_exit_code;
    host.connect = on_ctl_connect;
    host.close_window = on_ctl_close;
    host.title = on_ctl_title;

    TtCtl *c = tt_ctl_start("abitest", &host);
    CHECK(c != NULL);
    if (!c) {
        fprintf(stderr, "  %s\n", tt_last_error());
        tt_session_free(s);
        return;
    }
    const char *path = tt_ctl_path(c);
    CHECK(path != NULL && strstr(path, "abitest.sock") != NULL);
    CHECK(tt_ctl_poll_fd(c) >= 0);
    CHECK(tt_ctl_wait_handle(c) == NULL);

    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    CHECK(fd >= 0);
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof addr);
    addr.sun_family = AF_UNIX;
    snprintf(addr.sun_path, sizeof addr.sun_path, "%s", path);
    CHECK(connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0);

    char buf[8192];
    /* The window's own title comes from the callback rather than from the
     * terminal, which is what `title` is for. */
    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"status\"}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "\"title\":\"a window\"") != NULL);
    CHECK(strstr(buf, "\"connected\":false") != NULL);

    /* The terminal itself, read back as text. */
    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"screen\"}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "\"hello\"") != NULL);

    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"connect\","
                   "\"params\":{\"line\":\"myhost /ssh\"}}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "\"started\":true") != NULL);
    CHECK(log.connects == 1);
    CHECK(strcmp(log.last_line, "myhost /ssh") == 0);

    /* A macro, waited for: the callback reports it running once and then
     * finished, and the exit code comes back to the client. */
    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"macro.run\","
                   "\"params\":{\"path\":\"/tmp/x.ttl\","
                   "\"params\":[\"one\"],\"wait\":true}}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "\"exit\":5") != NULL);
    CHECK(log.macros == 1);
    CHECK(strcmp(log.last_macro, "/tmp/x.ttl") == 0);
    CHECK(strcmp(log.last_param, "one") == 0);

    /* And a second one while the first is up, which is its own error code
     * because a client retries that one and not the others. */
    log.running_left = 1;
    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"macro.run\","
                   "\"params\":{\"path\":\"/tmp/x.ttl\"}}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "-32002") != NULL);
    CHECK(strstr(buf, "already running") != NULL);

    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"close\"}\n",
                   buf, sizeof buf) == 0);
    CHECK(log.closed == 1);

    close(fd);
    /* Copied before the free, because the path is borrowed from the handle. */
    char socket_path[256];
    snprintf(socket_path, sizeof socket_path, "%s", path);
    tt_ctl_free(c);
    /* Freeing unlinks it, so the directory does not fill with the names of
     * windows that have closed. */
    struct stat st;
    CHECK(stat(socket_path, &st) != 0);
    tt_session_free(s);
    rmdir(dir);
}

/* A window with no callbacks at all: every method that needs one is refused
 * with -32003, and the ones that only need the session still work. */
static void test_ctl_without_a_frontend(void)
{
    char dir[] = "/tmp/sterna-abi-XXXXXX";
    CHECK(mkdtemp(dir) != NULL);
    setenv("XDG_RUNTIME_DIR", dir, 1);

    TtConfig cfg;
    tt_config_default(&cfg);
    TtSession *s = tt_session_new(&cfg);

    TtCtl *c = tt_ctl_start(NULL, NULL);
    CHECK(c != NULL);
    if (!c) {
        tt_session_free(s);
        return;
    }
    /* A null name is this process's pid, which is what a window with no `/D=`
     * uses. */
    char want[64];
    snprintf(want, sizeof want, "%d.sock", (int)getpid());
    CHECK(strstr(tt_ctl_path(c), want) != NULL);

    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof addr);
    addr.sun_family = AF_UNIX;
    snprintf(addr.sun_path, sizeof addr.sun_path, "%s", tt_ctl_path(c));
    CHECK(connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0);

    char buf[4096];
    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"close\"}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "-32003") != NULL);

    /* ...and `status` still answers, because it needs nothing of the window. */
    CHECK(ctl_call(c, s, fd,
                   "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"status\"}\n",
                   buf, sizeof buf) == 0);
    CHECK(strstr(buf, "\"cols\":80") != NULL);

    close(fd);
    tt_ctl_free(c);
    tt_session_free(s);
    rmdir(dir);
}

int main(void)
{
    printf("Sterna core %s\n", tt_version());
    test_i18n();
    test_screen();
    test_sixel();
    test_remote_clipboard();
    test_attributes();
    test_scrollback_viewport();
    test_absolute_lines();
    test_url_lookup();
    test_logging();
    test_log_name();
    test_settings();
    test_remembered_connection();
    test_cmdline();
    test_input();
    test_palette();
    test_window_ops();
    test_printer();
    test_serial();
    test_ssh();
    test_telnet();
    test_pty();
    test_transfer();
    test_plugins();
    test_macro();
    test_macro_without_a_frontend();
    test_macro_ends_quietly();
    test_macro_cancelled();
    test_ctl();
    test_ctl_without_a_frontend();
    test_null_safety();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("ABI ok\n");
    return 0;
}
