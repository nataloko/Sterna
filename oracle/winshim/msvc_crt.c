/*
 * termitta oracle — MSVC Secure CRT / locale shim for POSIX. See msvc_crt.h.
 */
#define _GNU_SOURCE
#include <time.h>
#include <stdlib.h>
#include "msvc_crt.h"

#include <errno.h>
#include <locale.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

_locale_t _create_locale(int category, const char *locale)
{
	/* MSVC takes a category constant (LC_ALL); newlocale takes a mask.
	 * vtterm.c:308 is the only call site and passes LC_ALL, so map that
	 * and fall back to LC_ALL_MASK for anything else. */
	int mask = LC_ALL_MASK;
	(void)category;
	return newlocale(mask, locale, (locale_t)0);
}

void _free_locale(_locale_t locale)
{
	if (locale != (locale_t)0) {
		freelocale(locale);
	}
}

int strncpy_s(char *dest, size_t destsz, const char *src, size_t count)
{
	size_t n;

	if (dest == NULL || destsz == 0) {
		return EINVAL;
	}
	if (src == NULL) {
		dest[0] = '\0';
		return EINVAL;
	}

	if (count == _TRUNCATE) {
		n = strnlen(src, destsz - 1);
		memcpy(dest, src, n);
		dest[n] = '\0';
		return src[n] != '\0' ? STRUNCATE : 0;
	}

	n = strnlen(src, count);
	if (n >= destsz) {
		/* MSVC treats overflow without _TRUNCATE as an invalid parameter
		 * and empties the destination. */
		dest[0] = '\0';
		return ERANGE;
	}
	memcpy(dest, src, n);
	dest[n] = '\0';
	return 0;
}

int strncat_s(char *dest, size_t destsz, const char *src, size_t count)
{
	size_t dlen, avail, n;

	if (dest == NULL || destsz == 0) {
		return EINVAL;
	}
	if (src == NULL) {
		return EINVAL;
	}

	dlen = strnlen(dest, destsz);
	if (dlen == destsz) {
		/* Unterminated destination. */
		dest[0] = '\0';
		return EINVAL;
	}
	avail = destsz - dlen - 1;

	if (count == _TRUNCATE) {
		n = strnlen(src, avail);
		memcpy(dest + dlen, src, n);
		dest[dlen + n] = '\0';
		return src[n] != '\0' ? STRUNCATE : 0;
	}

	n = strnlen(src, count);
	if (n > avail) {
		dest[0] = '\0';
		return ERANGE;
	}
	memcpy(dest + dlen, src, n);
	dest[dlen + n] = '\0';
	return 0;
}

/*
 * Shared back end. Mirrors _snprintf_s: on _TRUNCATE overflow, write what
 * fits, NUL-terminate, and return -1 (not the would-be length).
 */
static int snprintf_s_v(char *buffer, size_t sizeOfBuffer, size_t count,
                        const char *format, _locale_t locale, va_list ap)
{
	locale_t saved = (locale_t)0;
	int r;

	if (buffer == NULL || sizeOfBuffer == 0) {
		return -1;
	}

	if (locale != (locale_t)0) {
		saved = uselocale(locale);
	}
	r = vsnprintf(buffer, sizeOfBuffer, format, ap);
	if (locale != (locale_t)0 && saved != (locale_t)0) {
		uselocale(saved);
	}

	if (r < 0) {
		buffer[0] = '\0';
		return -1;
	}

	if ((size_t)r >= sizeOfBuffer) {
		/* vsnprintf already truncated and NUL-terminated. */
		if (count == _TRUNCATE) {
			return -1;
		}
		buffer[0] = '\0';
		return -1;
	}

	if (count != _TRUNCATE && (size_t)r > count) {
		buffer[count] = '\0';
		return (int)count;
	}

	return r;
}

int _vsnprintf_s(char *buffer, size_t sizeOfBuffer, size_t count,
                 const char *format, va_list ap)
{
	return snprintf_s_v(buffer, sizeOfBuffer, count, format, (locale_t)0, ap);
}

int _vsnwprintf_s(wchar_t *buffer, size_t sizeOfBuffer, size_t count,
                  const wchar_t *format, va_list ap)
{
	int r;

	if (buffer == NULL || sizeOfBuffer == 0) {
		return -1;
	}
	r = vswprintf(buffer, sizeOfBuffer, format, ap);
	if (r < 0) {
		/* glibc's vswprintf returns -1 on overflow rather than the would-be
		 * length, which matches _TRUNCATE's contract closely enough. */
		buffer[sizeOfBuffer - 1] = L'\0';
		return -1;
	}
	if (count != _TRUNCATE && (size_t)r > count) {
		buffer[count] = L'\0';
		return (int)count;
	}
	return r;
}

wchar_t *_wcsdup(const wchar_t *s)
{
	return wcsdup(s);
}

int _snprintf_s(char *buffer, size_t sizeOfBuffer, size_t count,
                const char *format, ...)
{
	va_list ap;
	int r;

	va_start(ap, format);
	r = snprintf_s_v(buffer, sizeOfBuffer, count, format, (locale_t)0, ap);
	va_end(ap);
	return r;
}

int _snprintf_s_l(char *buffer, size_t sizeOfBuffer, size_t count,
                  const char *format, _locale_t locale, ...)
{
	va_list ap;
	int r;

	/* MSVC puts the locale after the format string, before the varargs. */
	va_start(ap, locale);
	r = snprintf_s_v(buffer, sizeOfBuffer, count, format, locale, ap);
	va_end(ap);
	return r;
}

int _wfopen_s(FILE **pFile, const wchar_t *filename, const wchar_t *mode)
{
	char path[4096], m[16];
	if (pFile == NULL)
		return 22;              /* EINVAL, as the Secure CRT reports it. */
	*pFile = NULL;
	if (wcstombs(path, filename, sizeof(path)) == (size_t)-1)
		return 22;
	if (wcstombs(m, mode, sizeof(m)) == (size_t)-1)
		return 22;
	*pFile = fopen(path, m);
	return (*pFile != NULL) ? 0 : errno;
}

int sscanf_s(const char *buffer, const char *format, ...)
{
	/* See the note in msvc_crt.h: numeric conversions only. */
	va_list ap;
	int n;
	va_start(ap, format);
	n = vsscanf(buffer, format, ap);
	va_end(ap);
	return n;
}

int ctime_s(char *buf, size_t size, const time_t *t)
{
	char tmp[32];
	if (buf == NULL || size < 26 || t == NULL)
		return 22;                      /* EINVAL */
	if (ctime_r(t, tmp) == NULL)
		return 22;
	strncpy(buf, tmp, size - 1);
	buf[size - 1] = '\0';
	return 0;
}

int localtime_s(struct tm *tm, const time_t *t)
{
	/* Argument order is MSVC's (out, in) — the reverse of localtime_r. */
	if (tm == NULL || t == NULL)
		return 22;
	return localtime_r(t, tm) != NULL ? 0 : 22;
}

int memmove_s(void *dest, size_t destsz, const void *src, size_t count)
{
	if (dest == NULL || src == NULL)
		return 22;
	if (count > destsz)
		return 34;                      /* ERANGE */
	memmove(dest, src, count);
	return 0;
}

long long _atoi64(const char *s)
{
	return strtoll(s, NULL, 10);
}
