/*
 * termitta oracle — swscanf_s shim.
 *
 * MSVC's swscanf_s takes an extra (unsigned) buffer-size argument after every
 * %s / %c / %[...] pointer, so it CANNOT be macro-mapped onto plain swscanf:
 * the size would be consumed as the next pointer.
 *
 * Only one call site reaches this — unicode.cpp's Unicode-mapping-list config
 * loader, which the oracle never executes (it has no INI file). Rather than
 * pull in a full scanf, this implements the exact subset that appears there:
 *
 *     L"%[^,] , %s"   and   L"%s"
 *
 * Supported directives: %s, %[^<set>], literal characters, and whitespace
 * (which matches any run of whitespace). Anything else returns EOF rather
 * than guessing — if this ever fires, the config loader needs a real parser,
 * not a silently wrong one.
 *
 * Note unicode.cpp:900 already carries a __MINGW32__ branch using plain
 * swscanf, so upstream anticipated non-MSVC builds here.
 */
#define _GNU_SOURCE
#include <stdarg.h>
#include <stdio.h>
#include <wchar.h>
#include <wctype.h>

int swscanf_s(const wchar_t *src, const wchar_t *fmt, ...);

int swscanf_s(const wchar_t *src, const wchar_t *fmt, ...)
{
	va_list ap;
	int assigned = 0;
	const wchar_t *s = src;
	const wchar_t *f = fmt;

	va_start(ap, fmt);

	while (*f != L'\0') {
		if (iswspace(*f)) {
			while (iswspace(*f)) {
				f++;
			}
			while (iswspace(*s)) {
				s++;
			}
			continue;
		}

		if (*f != L'%') {
			if (*s != *f) {
				goto done;
			}
			f++;
			s++;
			continue;
		}

		f++;   /* past '%' */

		if (*f == L's') {
			wchar_t *out = va_arg(ap, wchar_t *);
			unsigned int cap = va_arg(ap, unsigned int);
			unsigned int n = 0;

			f++;
			while (iswspace(*s)) {
				s++;
			}
			if (*s == L'\0') {
				goto done;
			}
			while (*s != L'\0' && !iswspace(*s)) {
				if (n + 1 >= cap) {
					goto done;   /* would overflow: MSVC fails the field */
				}
				out[n++] = *s++;
			}
			out[n] = L'\0';
			assigned++;
			continue;
		}

		if (*f == L'[') {
			wchar_t *out = va_arg(ap, wchar_t *);
			unsigned int cap = va_arg(ap, unsigned int);
			unsigned int n = 0;
			int negate = 0;
			const wchar_t *set_start, *set_end;

			f++;
			if (*f == L'^') {
				negate = 1;
				f++;
			}
			set_start = f;
			while (*f != L']' && *f != L'\0') {
				f++;
			}
			if (*f != L']') {
				goto done;   /* malformed */
			}
			set_end = f;
			f++;

			for (;;) {
				int in_set = 0;
				const wchar_t *p;

				if (*s == L'\0') {
					break;
				}
				for (p = set_start; p < set_end; p++) {
					if (*p == *s) {
						in_set = 1;
						break;
					}
				}
				if (negate ? in_set : !in_set) {
					break;
				}
				if (n + 1 >= cap) {
					goto done;
				}
				out[n++] = *s++;
			}
			if (n == 0) {
				goto done;   /* empty match fails the field */
			}
			out[n] = L'\0';
			assigned++;
			continue;
		}

		/* Unsupported directive — refuse rather than guess. */
		va_end(ap);
		return EOF;
	}

done:
	va_end(ap);
	return assigned;
}
