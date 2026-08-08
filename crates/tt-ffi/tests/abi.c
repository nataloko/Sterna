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

#include <termitta.h>

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

int main(void)
{
    printf("termitta core %s\n", tt_version());
    test_screen();
    test_attributes();
    test_scrollback_viewport();
    test_logging();
    test_input();
    test_palette();
    test_serial();
    test_ssh();
    test_null_safety();

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("ABI ok\n");
    return 0;
}
