/*
 * qtterm oracle — MSVC Secure CRT / locale shim for POSIX.
 *
 * vtterm.c and buffer.c reach for exactly five MSVC-only CRT functions.
 * These reimplementations follow Microsoft's documented semantics closely,
 * because the oracle's output is ground truth for the Rust port — a sloppy
 * return value here would silently skew every differential test.
 *
 * The subtle one is _snprintf_s / _snprintf_s_l: on truncation with
 * _TRUNCATE it returns -1, NOT the would-be length that vsnprintf returns.
 * vtterm.c assigns that result to `len` and accumulates it (see the DECRQSS
 * SGR report around vtterm.c:4319), so the distinction is load-bearing.
 */
#pragma once

#include <locale.h>
#include <stddef.h>
#include <stdio.h>
#include <time.h>

#ifdef __cplusplus
extern "C" {
#endif

/* MSVC spells the opaque locale handle _locale_t; POSIX 2008 spells it locale_t. */
typedef locale_t _locale_t;

#ifndef _TRUNCATE
#define _TRUNCATE ((size_t)-1)
#endif
#ifndef STRUNCATE
#define STRUNCATE 80
#endif

/* Category is accepted for source compatibility; only LC_ALL is ever passed. */
_locale_t _create_locale(int category, const char *locale);
void      _free_locale(_locale_t locale);

/* Return 0 on success, STRUNCATE when _TRUNCATE truncated, EINVAL on bad args. */
/* Secure-CRT fopen. Wide paths are converted with wcstombs: the protocol log
 * is opened by name from a wchar_t path, and POSIX filesystems take bytes. */
int _wfopen_s(FILE **pFile, const wchar_t *filename, const wchar_t *mode);

/*
 * NOTE: MSVC's sscanf_s takes an extra buffer-size argument after every %s,
 * %c and %[ conversion; this forwards to vsscanf and therefore does NOT.
 * Safe because every call site we compile (kermit.c, zmodem.c, ymodem.c)
 * scans numbers only — verified. A future %s here would read garbage, so
 * check the format string before adding call sites.
 */
int sscanf_s(const char *buffer, const char *format, ...);

int ctime_s(char *buf, size_t size, const time_t *t);
int localtime_s(struct tm *tm, const time_t *t);
int memmove_s(void *dest, size_t destsz, const void *src, size_t count);
long long _atoi64(const char *s);

int strncpy_s(char *dest, size_t destsz, const char *src, size_t count);
int strncat_s(char *dest, size_t destsz, const char *src, size_t count);

/* Return chars written excluding NUL, or -1 on truncation/error. */
int _snprintf_s(char *buffer, size_t sizeOfBuffer, size_t count,
                const char *format, ...);
int _snprintf_s_l(char *buffer, size_t sizeOfBuffer, size_t count,
                  const char *format, _locale_t locale, ...);

#ifdef __cplusplus
}
#endif
