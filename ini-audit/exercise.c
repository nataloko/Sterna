/* What `GetPrivateProfile*` really does, asked of an implementation rather
 * than of the documentation.
 *
 * Tera Term reads and writes `TERATERM.INI` through the Win32 profile API
 * (`common/inifile_com.cpp` and `ttpset/ttset.c` — there is no portable
 * implementation anywhere in the tree), so "read TERATERM.INI natively,
 * bug-compatible" means reproducing that API's behaviour, quirks included.
 * Several of those quirks are undocumented, and at least one thing the
 * documentation *does* say turns out to be worth checking.
 *
 * Compiled with mingw-w64 and run under Wine. Wine is not Windows, and this
 * file does not pretend otherwise — see README.md for what that buys and what
 * it does not.
 *
 * Reads a case battery on stdin and writes one result line per case, so that
 * the Rust implementation can be fed the *same* battery and the two diffed.
 * The battery is data, not code, precisely so there is one copy of it.
 *
 * Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.
 */

#include <windows.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_LINE 4096
#define MAX_BYTES 4096

static wchar_t g_fixture[MAX_PATH];

/* --- the escaping the battery is written in ------------------------------ */

/* Decode `\r \n \t \\ \0 \xHH` into raw bytes. Returns the length. */
static size_t unescape(const char *in, unsigned char *out, size_t max)
{
    size_t n = 0;
    for (const char *p = in; *p && n < max; p++) {
        if (*p != '\\') {
            out[n++] = (unsigned char)*p;
            continue;
        }
        p++;
        switch (*p) {
        case 'r': out[n++] = '\r'; break;
        case 'n': out[n++] = '\n'; break;
        case 't': out[n++] = '\t'; break;
        case '0': out[n++] = '\0'; break;
        case '\\': out[n++] = '\\'; break;
        case 'x': {
            char hex[3] = {p[1], p[2], 0};
            out[n++] = (unsigned char)strtoul(hex, NULL, 16);
            p += 2;
            break;
        }
        case '\0': p--; break;
        default: out[n++] = (unsigned char)*p; break;
        }
    }
    return n;
}

/* The inverse, so a result with a NUL or a CR in it survives being printed. */
static void print_escaped(const unsigned char *b, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        unsigned char c = b[i];
        if (c == '\\') {
            fputs("\\\\", stdout);
        } else if (c == '\r') {
            fputs("\\r", stdout);
        } else if (c == '\n') {
            fputs("\\n", stdout);
        } else if (c == '\t') {
            fputs("\\t", stdout);
        } else if (c == 0) {
            fputs("\\0", stdout);
        } else if (c < 0x20 || c >= 0x7f) {
            printf("\\x%02x", c);
        } else {
            fputc(c, stdout);
        }
    }
}

/* --- narrow/wide, in the one encoding the battery is written in ---------- */

static wchar_t *to_wide(const char *utf8)
{
    int n = MultiByteToWideChar(CP_UTF8, 0, utf8, -1, NULL, 0);
    wchar_t *w = (wchar_t *)malloc(sizeof(wchar_t) * (size_t)n);
    MultiByteToWideChar(CP_UTF8, 0, utf8, -1, w, n);
    return w;
}

/* A wide result, printed escaped as UTF-8. `n` is in wide characters and may
 * cover embedded NULs, which is how a key list comes back. */
static void print_wide(const wchar_t *w, DWORD n)
{
    unsigned char buf[MAX_BYTES * 4];
    int len = WideCharToMultiByte(CP_UTF8, 0, w, (int)n, (char *)buf, sizeof buf, NULL, NULL);
    if (len < 0) {
        len = 0;
    }
    print_escaped(buf, (size_t)len);
}

/* --- the fixture --------------------------------------------------------- */

static void write_fixture(const char *escaped)
{
    unsigned char bytes[MAX_BYTES];
    /* `@empty` is how the battery spells a zero-length file, which is a case
     * in its own right: the API has to create one. */
    size_t n = strcmp(escaped, "@empty") == 0 ? 0 : unescape(escaped, bytes, sizeof bytes);

    HANDLE h = CreateFileW(g_fixture, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS,
                           FILE_ATTRIBUTE_NORMAL, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        fprintf(stderr, "cannot create fixture: %lu\n", GetLastError());
        exit(1);
    }
    DWORD written = 0;
    WriteFile(h, bytes, (DWORD)n, &written, NULL);
    CloseHandle(h);
}

static void print_fixture(void)
{
    HANDLE h = CreateFileW(g_fixture, GENERIC_READ, FILE_SHARE_READ, NULL,
                           OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        fputs("<no file>", stdout);
        return;
    }
    unsigned char bytes[MAX_BYTES];
    DWORD n = 0;
    ReadFile(h, bytes, sizeof bytes, &n, NULL);
    CloseHandle(h);
    print_escaped(bytes, n);
}

/* --- the battery --------------------------------------------------------- */

/* Split `line` on `|`, trimming each field. Returns how many were found. */
static int split(char *line, char **out, int max)
{
    int n = 0;
    char *p = line;
    while (n < max) {
        while (*p == ' ') {
            p++;
        }
        out[n++] = p;
        char *bar = strchr(p, '|');
        if (!bar) {
            break;
        }
        char *end = bar;
        while (end > p && end[-1] == ' ') {
            end--;
        }
        *end = '\0';
        p = bar + 1;
    }
    /* The last field keeps its trailing spaces off too. */
    char *last = out[n - 1];
    size_t len = strlen(last);
    while (len > 0 && (last[len - 1] == ' ' || last[len - 1] == '\r')) {
        last[--len] = '\0';
    }
    return n;
}

/* `@` is how the battery spells things a literal cannot: an absent argument
 * and an empty string. Everything else goes through the same escapes the
 * fixture does, so a value with a high byte in it can be written down. */
static const char *arg(const char *s)
{
    static char bufs[4][MAX_LINE];
    static int next;
    if (strcmp(s, "@empty") == 0) {
        return "";
    }
    if (!strchr(s, '\\')) {
        return s;
    }
    char *out = bufs[next++ & 3];
    size_t n = unescape(s, (unsigned char *)out, MAX_LINE - 1);
    out[n] = '\0';
    return out;
}

static int is_null(const char *s) { return strcmp(s, "@null") == 0; }

int main(int argc, char **argv)
{
    if (argc < 2) {
        fprintf(stderr, "usage: exercise.exe <dir-in-windows-form> < cases\n");
        return 2;
    }
    _snwprintf(g_fixture, MAX_PATH, L"%hs\\fixture.ini", argv[1]);

    char line[MAX_LINE];
    while (fgets(line, sizeof line, stdin)) {
        size_t len = strlen(line);
        while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r')) {
            line[--len] = '\0';
        }
        if (line[0] == '\0' || line[0] == '#') {
            continue;
        }

        char *f[8];
        int n = split(line, f, 8);
        if (n < 3) {
            fprintf(stderr, "malformed case: %s\n", line);
            return 2;
        }
        const char *name = f[0];
        const char *op = f[2];
        write_fixture(f[1]);

        printf("%s\t", name);

        if (strcmp(op, "get") == 0 && n >= 6) {
            wchar_t *sec = to_wide(arg(f[3]));
            wchar_t *key = to_wide(arg(f[4]));
            wchar_t *def = to_wide(arg(f[5]));
            wchar_t out[MAX_BYTES];
            /* Deliberately not memset: the length is the interesting half of
             * the answer, and a caller cannot tell "empty" from "untouched"
             * without it. */
            DWORD got = GetPrivateProfileStringW(sec, key, def, out, MAX_BYTES, g_fixture);
            printf("len=%lu str=", got);
            print_wide(out, got);
            free(sec);
            free(key);
            free(def);
        } else if (strcmp(op, "int") == 0 && n >= 6) {
            wchar_t *sec = to_wide(arg(f[3]));
            wchar_t *key = to_wide(arg(f[4]));
            wchar_t *def = to_wide(arg(f[5]));
            UINT got = GetPrivateProfileIntW(sec, key, (INT)wcstol(def, NULL, 10), g_fixture);
            printf("%u", got);
            free(sec);
            free(key);
            free(def);
        } else if (strcmp(op, "keys") == 0 && n >= 4) {
            /* A null key name asks for every key in the section, as one
             * double-NUL-terminated block. */
            wchar_t *sec = to_wide(arg(f[3]));
            wchar_t out[MAX_BYTES];
            DWORD got = GetPrivateProfileStringW(sec, NULL, L"", out, MAX_BYTES, g_fixture);
            printf("len=%lu str=", got);
            print_wide(out, got);
            free(sec);
        } else if (strcmp(op, "sections") == 0) {
            wchar_t out[MAX_BYTES];
            DWORD got = GetPrivateProfileStringW(NULL, NULL, L"", out, MAX_BYTES, g_fixture);
            printf("len=%lu str=", got);
            print_wide(out, got);
        } else if (strcmp(op, "write") == 0 && n >= 6) {
            wchar_t *sec = to_wide(arg(f[3]));
            wchar_t *key = is_null(f[4]) ? NULL : to_wide(arg(f[4]));
            wchar_t *val = is_null(f[5]) ? NULL : to_wide(arg(f[5]));
            BOOL ok = WritePrivateProfileStringW(sec, key, val, g_fixture);
            printf("ok=%d file=", ok ? 1 : 0);
            print_fixture();
            free(sec);
            free(key);
            free(val);
        } else if (strcmp(op, "truncate") == 0 && n >= 7) {
            /* What a caller sees when its buffer is too small, which decides
             * whether a long value comes back cut or empty. */
            wchar_t *sec = to_wide(arg(f[3]));
            wchar_t *key = to_wide(arg(f[4]));
            wchar_t *def = to_wide(arg(f[5]));
            DWORD size = (DWORD)strtoul(f[6], NULL, 10);
            wchar_t out[MAX_BYTES];
            for (DWORD i = 0; i < size + 2 && i < MAX_BYTES; i++) {
                out[i] = L'#';
            }
            DWORD got = GetPrivateProfileStringW(sec, key, def, out, size, g_fixture);
            printf("len=%lu str=", got);
            print_wide(out, got);
            free(sec);
            free(key);
            free(def);
        } else {
            printf("<unknown op %s>", op);
        }
        printf("\n");
        fflush(stdout);
    }
    return 0;
}
