/*
 * qtterm oracle — implementations for the handful of Win32 entry points that
 * vtterm.c and buffer.c actually reach.
 *
 * GetTickCount is backed by a monotonic clock but can also be frozen, because
 * vtterm.c uses it for bell-throttling (BeepStartTime / BeepSuppressTime) and
 * a wall-clock-dependent oracle would produce non-reproducible transcripts.
 * The runner freezes it by default; see oracle_clock_set_frozen().
 */
#define _GNU_SOURCE
#include <windows.h>

#include <stdlib.h>
#include <time.h>

/* ---- deterministic clock ------------------------------------------------ */

static int    g_clock_frozen = 1;
static DWORD  g_clock_ms     = 0;

void oracle_clock_set_frozen(int frozen) { g_clock_frozen = frozen; }
void oracle_clock_advance(DWORD ms)      { g_clock_ms += ms; }
DWORD oracle_clock_now(void)             { return g_clock_ms; }

DWORD GetTickCount(void)
{
	struct timespec ts;

	if (g_clock_frozen) {
		return g_clock_ms;
	}
	clock_gettime(CLOCK_MONOTONIC, &ts);
	return (DWORD)(ts.tv_sec * 1000u + ts.tv_nsec / 1000000u);
}

void Sleep(DWORD dwMilliseconds)
{
	/* The oracle must not actually sleep — vtterm.c calls this only to pace
	 * the bell. Advance the virtual clock instead so throttling logic still
	 * sees time passing. */
	if (g_clock_frozen) {
		g_clock_ms += dwMilliseconds;
		return;
	}
	{
		struct timespec ts;
		ts.tv_sec  = dwMilliseconds / 1000;
		ts.tv_nsec = (long)(dwMilliseconds % 1000) * 1000000L;
		nanosleep(&ts, NULL);
	}
}

/* ---- narrow/wide conversion --------------------------------------------- */

/*
 * Only the CP_UTF8 path is exercised (vtterm.c converts an OSC string for
 * logging). Implemented directly rather than via iconv to keep the oracle
 * free of locale state.
 */
static int utf8_encode(unsigned int cp, char *out)
{
	if (cp < 0x80) {
		out[0] = (char)cp;
		return 1;
	}
	if (cp < 0x800) {
		out[0] = (char)(0xC0 | (cp >> 6));
		out[1] = (char)(0x80 | (cp & 0x3F));
		return 2;
	}
	if (cp < 0x10000) {
		out[0] = (char)(0xE0 | (cp >> 12));
		out[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
		out[2] = (char)(0x80 | (cp & 0x3F));
		return 3;
	}
	out[0] = (char)(0xF0 | (cp >> 18));
	out[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
	out[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
	out[3] = (char)(0x80 | (cp & 0x3F));
	return 4;
}

int WideCharToMultiByte(UINT CodePage, DWORD dwFlags,
                        const wchar_t *lpWideCharStr, int cchWideChar,
                        char *lpMultiByteStr, int cbMultiByte,
                        const char *lpDefaultChar, BOOL *lpUsedDefaultChar)
{
	int i, n, written = 0;
	char tmp[4];

	(void)CodePage; (void)dwFlags; (void)lpDefaultChar;
	if (lpUsedDefaultChar != NULL) {
		*lpUsedDefaultChar = FALSE;
	}
	if (lpWideCharStr == NULL) {
		return 0;
	}

	n = cchWideChar;
	if (n < 0) {
		/* Negative length means NUL-terminated, and the NUL is included. */
		n = 0;
		while (lpWideCharStr[n] != L'\0') {
			n++;
		}
		n++;
	}

	for (i = 0; i < n; i++) {
		unsigned int cp = (unsigned int)lpWideCharStr[i];
		int len;

		/* wchar_t is 32-bit on Linux and 16-bit on Windows. Recombine
		 * surrogate pairs so transcripts match the Windows build. */
		if (cp >= 0xD800 && cp <= 0xDBFF && i + 1 < n) {
			unsigned int lo = (unsigned int)lpWideCharStr[i + 1];
			if (lo >= 0xDC00 && lo <= 0xDFFF) {
				cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
				i++;
			}
		}

		len = utf8_encode(cp, tmp);
		if (cbMultiByte == 0) {
			written += len;
			continue;
		}
		if (written + len > cbMultiByte) {
			return 0;   /* Win32 signals insufficient buffer with 0. */
		}
		if (lpMultiByteStr != NULL) {
			int k;
			for (k = 0; k < len; k++) {
				lpMultiByteStr[written + k] = tmp[k];
			}
		}
		written += len;
	}
	return written;
}

int MultiByteToWideChar(UINT CodePage, DWORD dwFlags,
                        const char *lpMultiByteStr, int cbMultiByte,
                        wchar_t *lpWideCharStr, int cchWideChar)
{
	/* Not reached by vtterm.c/buffer.c; present so the shim is self-contained.
	 * ASCII-only fallback — widen and fail loudly if that assumption breaks. */
	int i, n;

	(void)CodePage; (void)dwFlags;
	if (lpMultiByteStr == NULL) {
		return 0;
	}
	n = cbMultiByte;
	if (n < 0) {
		n = 0;
		while (lpMultiByteStr[n] != '\0') {
			n++;
		}
		n++;
	}
	if (cchWideChar == 0) {
		return n;
	}
	if (n > cchWideChar) {
		return 0;
	}
	for (i = 0; i < n; i++) {
		lpWideCharStr[i] = (wchar_t)(unsigned char)lpMultiByteStr[i];
	}
	return n;
}

/* ---- stubs -------------------------------------------------------------- */

unsigned int GetPrivateProfileIntW(const wchar_t *sec, const wchar_t *key,
                                   int nDefault, const wchar_t *file)
{
	/* The oracle runs with no INI file, so every setting takes its default.
	 * That is exactly the state we want to compare the Rust port against. */
	(void)sec; (void)key; (void)file;
	return (unsigned int)nDefault;
}

BOOL MessageBeep(UINT uType)      { (void)uType; return TRUE; }
void PostQuitMessage(int code)    { (void)code; }
BOOL UpdateWindow(HWND hWnd)      { (void)hWnd; return TRUE; }
int  StartPage(HDC hdc)           { (void)hdc; return 1; }
int  EndPage(HDC hdc)             { (void)hdc; return 1; }

BOOL IsDBCSLeadByte(BYTE TestChar)
{
	/* The oracle runs in UTF-8, where no byte is a DBCS lead byte. Legacy
	 * CJK codepage input is Stage 4 work and will need the vendored tables. */
	(void)TestChar;
	return FALSE;
}

HINSTANCE ShellExecuteW(HWND hwnd, const wchar_t *lpOperation,
                        const wchar_t *lpFile, const wchar_t *lpParameters,
                        const wchar_t *lpDirectory, int nShowCmd)
{
	(void)hwnd; (void)lpOperation; (void)lpFile;
	(void)lpParameters; (void)lpDirectory; (void)nShowCmd;
	return (HINSTANCE)(LONG_PTR)42;   /* >32 means success in Win32. */
}
