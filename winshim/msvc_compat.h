/*
 * Sterna oracle — force-included (-include) into every Tera Term translation
 * unit we build.
 *
 * Some of the files we compile in (asprintf.cpp, tttypes_termid.cpp,
 * charset.cpp) never include <windows.h>, so they don't pick up the shim's
 * MSVC definitions. This header carries the small set they need.
 */
#pragma once

#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/stat.h>

#include "msvc_crt.h"

/*
 * MSVC's 64-bit stat. `_stati64` appears only as a struct tag in the sources
 * we build (ttpfile/filesys_io.h's vtable and kermit.c), never as the function
 * of the same name, so mapping the tag onto the POSIX struct is exact rather
 * than approximate: st_mode, st_size and st_mtime are all present and mean the
 * same thing. `_S_IFREG` likewise has the same value as `S_IFREG` — zmodem.c
 * ORs it into the file mode it transmits, so the wire format depends on it.
 */
#ifndef _stati64
#define _stati64 stat
#endif
#ifndef _S_IFREG
#define _S_IFREG S_IFREG
#endif

#ifndef _countof
#define _countof(a) (sizeof(a) / sizeof((a)[0]))
#endif

/* MSVC SAL annotations — pure documentation on Windows, nothing off it. */
#define _Printf_format_string_
#define _In_
#define _In_opt_
#define _In_z_
#define _Out_
#define _Out_opt_
#define _Inout_
#define _Inout_opt_
#define _Ret_maybenull_

/*
 * NOTE: MSVC's _snprintf does NOT NUL-terminate on truncation; C99 snprintf
 * does. The mapping is therefore safe-but-not-identical. Every call site we
 * compile writes short fixed-format strings into buffers sized with room to
 * spare, so the divergence is unreachable — but if a differential test ever
 * disagrees on a truncated string, look here first.
 */
#ifndef _snprintf
#define _snprintf snprintf
#endif
#ifndef _vsnprintf
#define _vsnprintf vsnprintf
#endif

#ifdef __cplusplus
extern "C" {
#endif

int _vsnprintf_s(char *buffer, size_t sizeOfBuffer, size_t count,
                 const char *format, va_list ap);
int _vsnwprintf_s(wchar_t *buffer, size_t sizeOfBuffer, size_t count,
                  const wchar_t *format, va_list ap);
wchar_t *_wcsdup(const wchar_t *s);
int swscanf_s(const wchar_t *src, const wchar_t *fmt, ...);

/* Config reads always fall through to the default: the oracle has no INI. */
unsigned int GetPrivateProfileIntW(const wchar_t *sec, const wchar_t *key,
                                   int nDefault, const wchar_t *file);

#ifdef __cplusplus
}
#endif
