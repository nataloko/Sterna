/*
 * Minimal codeconv.
 *
 * Tera Term's common/codeconv.cpp does not compile off Windows: it leans on
 * GetACP() and the Win32 codepage converters. But vtterm.c and buffer.c only
 * need eight of its entry points, and most are pure Unicode transforms.
 * ttpfile/protolog.cpp needs two of the same ones for its path handling,
 * which is why this sits beside the Win32 shim rather than inside the oracle.
 *
 * UTF32ToUTF16 in particular is NOT optional. buffer.c:234 calls it to fill
 * buff_char_t::wc2, and expand_wchar() reads back from wc2 — so a stub that
 * returns 0 yields a screen that holds the right codepoints in `u32` and
 * renders as entirely blank. That failure looks exactly like a broken parser
 * and is worth remembering.
 *
 * The legacy CJK codepage paths (UTF32ToMBCP, CP932ToUTF32, ToWcharA) still
 * need Tera Term's vendored .map/.tbl tables. They are marked below and are
 * Stage 4 work; the oracle runs in UTF-8 and never reaches them.
 */
#include <windows.h>

#include <stdlib.h>
#include <string.h>
#include <wchar.h>

/* Replacement character for anything we cannot represent. */
#define REPLACEMENT 0xFFFD

size_t UTF32ToUTF16(unsigned int u32, wchar_t *wstr_ptr, size_t wstr_len)
{
	/* wchar_t is 32-bit on Linux, but buff_char_t::wc2 is a 2-element array
	 * that buffer.c treats as a UTF-16 pair, so keep the surrogate encoding
	 * exactly as the Windows build produces it. */
	if (u32 > 0x10FFFF || (u32 >= 0xD800 && u32 <= 0xDFFF)) {
		u32 = REPLACEMENT;
	}

	if (u32 < 0x10000) {
		if (wstr_ptr == NULL) {
			return 1;
		}
		if (wstr_len < 1) {
			return 0;
		}
		wstr_ptr[0] = (wchar_t)u32;
		if (wstr_len >= 2) {
			wstr_ptr[1] = 0;
		}
		return 1;
	}

	if (wstr_ptr == NULL) {
		return 2;
	}
	if (wstr_len < 2) {
		return 0;
	}
	u32 -= 0x10000;
	wstr_ptr[0] = (wchar_t)(0xD800 + (u32 >> 10));
	wstr_ptr[1] = (wchar_t)(0xDC00 + (u32 & 0x3FF));
	return 2;
}

size_t UTF32ToUTF8(unsigned int u32, char *u8_ptr, size_t u8_len)
{
	unsigned char b[4];
	size_t n;

	if (u32 > 0x10FFFF || (u32 >= 0xD800 && u32 <= 0xDFFF)) {
		u32 = REPLACEMENT;
	}

	if (u32 < 0x80) {
		b[0] = (unsigned char)u32;
		n = 1;
	} else if (u32 < 0x800) {
		b[0] = (unsigned char)(0xC0 | (u32 >> 6));
		b[1] = (unsigned char)(0x80 | (u32 & 0x3F));
		n = 2;
	} else if (u32 < 0x10000) {
		b[0] = (unsigned char)(0xE0 | (u32 >> 12));
		b[1] = (unsigned char)(0x80 | ((u32 >> 6) & 0x3F));
		b[2] = (unsigned char)(0x80 | (u32 & 0x3F));
		n = 3;
	} else {
		b[0] = (unsigned char)(0xF0 | (u32 >> 18));
		b[1] = (unsigned char)(0x80 | ((u32 >> 12) & 0x3F));
		b[2] = (unsigned char)(0x80 | ((u32 >> 6) & 0x3F));
		b[3] = (unsigned char)(0x80 | (u32 & 0x3F));
		n = 4;
	}

	if (u8_ptr == NULL) {
		return n;
	}
	if (u8_len < n) {
		return 0;
	}
	memcpy(u8_ptr, b, n);
	return n;
}

wchar_t *ToWcharU8(const char *strU8)
{
	/* UTF-8 -> wchar_t. Malformed sequences become U+FFFD rather than
	 * failing, matching how the terminal treats junk on the wire. */
	size_t n, i = 0, o = 0;
	wchar_t *out;

	if (strU8 == NULL) {
		return NULL;
	}
	n = strlen(strU8);
	out = (wchar_t *)malloc((n + 1) * sizeof(wchar_t));
	if (out == NULL) {
		return NULL;
	}

	while (i < n) {
		unsigned char c = (unsigned char)strU8[i];
		unsigned int cp;
		size_t extra;

		if (c < 0x80) {
			cp = c;
			extra = 0;
		} else if ((c & 0xE0) == 0xC0) {
			cp = c & 0x1F;
			extra = 1;
		} else if ((c & 0xF0) == 0xE0) {
			cp = c & 0x0F;
			extra = 2;
		} else if ((c & 0xF8) == 0xF0) {
			cp = c & 0x07;
			extra = 3;
		} else {
			out[o++] = REPLACEMENT;
			i++;
			continue;
		}

		if (i + extra >= n + 1 && extra > 0) {
			out[o++] = REPLACEMENT;
			break;
		}
		i++;
		while (extra-- > 0) {
			unsigned char cc = (unsigned char)strU8[i];
			if ((cc & 0xC0) != 0x80) {
				cp = REPLACEMENT;
				break;
			}
			cp = (cp << 6) | (cc & 0x3F);
			i++;
		}
		out[o++] = (wchar_t)cp;
	}
	out[o] = L'\0';
	return out;
}

char *ToCharW(const wchar_t *strW)
{
	size_t i, n, cap, o = 0;
	char *out;

	if (strW == NULL) {
		return NULL;
	}
	n = wcslen(strW);
	cap = n * 4 + 1;
	out = (char *)malloc(cap);
	if (out == NULL) {
		return NULL;
	}
	for (i = 0; i < n; i++) {
		o += UTF32ToUTF8((unsigned int)strW[i], out + o, cap - o);
	}
	out[o] = '\0';
	return out;
}

/*
 * buffer.c:3076 (ConvertACPChar) dereferences the result immediately, with no
 * NULL check — so this must always return an allocated, NUL-terminated buffer.
 * A stub returning NULL segfaults on the first combining character.
 */
char *_WideCharToMultiByte(const wchar_t *wstr_ptr, size_t wstr_len,
                           int code_page, size_t *mb_len_)
{
	size_t i, o = 0, cap;
	char *out;

	cap = (wstr_len + 1) * 4 + 1;
	out = (char *)malloc(cap);
	if (out == NULL) {
		if (mb_len_ != NULL) {
			*mb_len_ = 0;
		}
		return NULL;
	}

	if (wstr_ptr != NULL && code_page == CP_UTF8) {
		for (i = 0; i < wstr_len && wstr_ptr[i] != L'\0'; i++) {
			unsigned int cp = (unsigned int)wstr_ptr[i];
			if (cp >= 0xD800 && cp <= 0xDBFF && i + 1 < wstr_len) {
				unsigned int lo = (unsigned int)wstr_ptr[i + 1];
				if (lo >= 0xDC00 && lo <= 0xDFFF) {
					cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
					i++;
				}
			}
			o += UTF32ToUTF8(cp, out + o, cap - o - 1);
		}
	} else if (wstr_ptr != NULL) {
		/* Legacy codepage: '?' per character, matching Win32's default
		 * unmappable behaviour. Real fidelity needs the vendored tables. */
		for (i = 0; i < wstr_len && wstr_ptr[i] != L'\0'; i++) {
			out[o++] = '?';
		}
	}

	out[o] = '\0';
	if (mb_len_ != NULL) {
		*mb_len_ = o;
	}
	return out;
}

wchar_t *_MultiByteToWideChar(const char *str_ptr, size_t str_len,
                              int code_page, size_t *w_len_)
{
	wchar_t *out;
	size_t n;

	(void)code_page;
	if (str_ptr == NULL) {
		if (w_len_ != NULL) {
			*w_len_ = 0;
		}
		return NULL;
	}
	out = (wchar_t *)malloc((str_len + 1) * sizeof(wchar_t));
	if (out == NULL) {
		return NULL;
	}
	for (n = 0; n < str_len && str_ptr[n] != '\0'; n++) {
		out[n] = (wchar_t)(unsigned char)str_ptr[n];
	}
	out[n] = L'\0';
	if (w_len_ != NULL) {
		*w_len_ = n;
	}
	return out;
}

unsigned short UTF32ToDecSp(unsigned int u32)
{
	/*
	 * Reverse of the DEC Special Graphics mapping: given a Unicode codepoint,
	 * return the 0x5F..0x7E DEC glyph that produces it, or 0 if none does.
	 * Table order matches DEC's charset positions `_` (0x5F) through `~`.
	 */
	static const unsigned int dec_sp[] = {
		0x00A0, /* 5F blank            */
		0x25C6, /* 60 diamond          */
		0x2592, /* 61 checkerboard     */
		0x2409, /* 62 HT               */
		0x240C, /* 63 FF               */
		0x240D, /* 64 CR               */
		0x240A, /* 65 LF               */
		0x00B0, /* 66 degree           */
		0x00B1, /* 67 plus/minus       */
		0x2424, /* 68 NL               */
		0x240B, /* 69 VT               */
		0x2518, /* 6A lower-right      */
		0x2510, /* 6B upper-right      */
		0x250C, /* 6C upper-left       */
		0x2514, /* 6D lower-left       */
		0x253C, /* 6E cross            */
		0x23BA, /* 6F scan line 1      */
		0x23BB, /* 70 scan line 3      */
		0x2500, /* 71 horizontal       */
		0x23BC, /* 72 scan line 7      */
		0x23BD, /* 73 scan line 9      */
		0x251C, /* 74 left tee         */
		0x2524, /* 75 right tee        */
		0x2534, /* 76 bottom tee       */
		0x252C, /* 77 top tee          */
		0x2502, /* 78 vertical         */
		0x2264, /* 79 less-or-equal    */
		0x2265, /* 7A greater-or-equal */
		0x03C0, /* 7B pi               */
		0x2260, /* 7C not-equal        */
		0x00A3, /* 7D sterling         */
		0x00B7, /* 7E middle dot       */
	};
	size_t i;

	for (i = 0; i < sizeof(dec_sp) / sizeof(dec_sp[0]); i++) {
		if (dec_sp[i] == u32) {
			return (unsigned short)(0x5F + i);
		}
	}
	return 0;
}

/* ---- legacy CJK codepages: Stage 4 ------------------------------------- */
/*
 * These need Tera Term's vendored .map/.tbl tables. The oracle runs UTF-8 only
 * and never reaches them; they abort rather than return silently wrong data,
 * because a quietly-wrong charset conversion is exactly the kind of bug a
 * differential test is supposed to catch, not introduce.
 */
#include <stdio.h>

static void unsupported(const char *fn)
{
	fprintf(stderr, "oracle: %s needs the vendored CJK tables (not yet ported)\n", fn);
	abort();
}

size_t UTF32ToMBCP(unsigned int u32, int code_page, char *mb_ptr, size_t mb_len)
{
	static int warned;

	if (code_page == CP_UTF8) {
		return UTF32ToUTF8(u32, mb_ptr, mb_len);
	}

	/*
	 * Returning 0 is the documented "cannot represent" signal: BuffSetChar2
	 * (buffer.c:261) already maps it to '?'. So this degrades exactly the way
	 * Tera Term does for an unmappable character, rather than aborting.
	 *
	 * It is still a divergence from the real build for legacy codepages, so
	 * say so once — a silent wrong answer in a test oracle is worse than a
	 * loud one.
	 */
	(void)u32; (void)mb_ptr; (void)mb_len;
	if (!warned) {
		warned = 1;
		fprintf(stderr,
		        "oracle: codepage %d unsupported (needs the vendored CJK tables); "
		        "affected cells fall back to '?'\n", code_page);
	}
	return 0;
}

unsigned int CP932ToUTF32(unsigned short cp932)
{
	(void)cp932;
	unsupported("CP932ToUTF32");
	return 0;
}

wchar_t *ToWcharA(const char *strA)
{
	(void)strA;
	unsupported("ToWcharA");
	return NULL;
}
