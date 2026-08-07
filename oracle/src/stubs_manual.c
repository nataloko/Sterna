/*
 * termitta oracle — hand-written stubs.
 *
 * These are the symbols the grid model actually observes, so unlike
 * stubs_generated.c they carry real behaviour:
 *
 *   - the global settings (ts) and comm state (cv) structs
 *   - CommRead1Byte, which is where the oracle injects input bytes
 *   - the Comm*Out family, which records what the terminal replies
 *   - screen-geometry globals and the resize path into buffer.c
 *   - title / colour requests, recorded as side-channel events
 *
 * Everything else no-ops. See oracle.h for the recorder API.
 */
#include <windows.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "teraterm.h"
#include "tttypes.h"
#include "ttcommon.h"
#include "buffer.h"
#include "vtdisp.h"
#include "vtterm.h"

#include "oracle.h"

/* ---- global state vtterm.c/buffer.c expect ------------------------------ */

TTTSet ts;
TComVar cv;
HWND HVTWin;

int WinWidth = 80, WinHeight = 24;
int CursorX = 0, CursorY = 0;
int WinOrgX = 0, WinOrgY = 0, NewOrgX = 0, NewOrgY = 0;
int NumOfLines = 24, NumOfColumns = 80;
int PageStart = 0, BuffEnd = 0;
TCharAttr DefCharAttr;

BOOL AdjustSize = FALSE, DontChangeSize = FALSE;
BOOL IMEstat = FALSE, IMECompositionState = FALSE;
BOOL AppliKeyMode = FALSE, AppliCursorMode = FALSE, AppliEscapeMode = FALSE;
BOOL AutoRepeatMode = TRUE;
BOOL Send8BitMode = FALSE;
BOOL KeybEnabled = TRUE;
BOOL AltKey = FALSE, ControlKey = FALSE, ShiftKey = FALSE;
BOOL DDELog = FALSE;


/* ---- input feed --------------------------------------------------------- */

static const unsigned char *g_feed;
static size_t g_feed_len;
static size_t g_feed_pos;

#define PUSHBACK_MAX 64
static unsigned char g_pushback[PUSHBACK_MAX];
static int g_pushback_n;

void oracle_feed(const void *bytes, size_t len)
{
	g_feed = (const unsigned char *)bytes;
	g_feed_len = len;
	g_feed_pos = 0;
}

int oracle_feed_remaining(void)
{
	return (int)(g_feed_len - g_feed_pos) + g_pushback_n;
}

/* Notification balloons are pure UI. */
void WINAPI NotifyMessageW(PComVar pcv, const wchar_t *msg, const wchar_t *title, DWORD flag)
{
	(void)pcv; (void)msg; (void)title; (void)flag;
}

/*
 * vtterm.c re-injects bytes into the receive stream via CommInsert1Byte —
 * used when an escape sequence turns out not to be one and its bytes have to
 * be reprocessed as text. Those bytes must come back out of CommRead1Byte
 * ahead of the remaining feed, so this is a real LIFO pushback, not a stub.
 */

void WINAPI CommInsert1Byte(PComVar pcv, BYTE b)
{
	(void)pcv;
	if (g_pushback_n < PUSHBACK_MAX) {
		g_pushback[g_pushback_n++] = b;
	}
}

int WINAPI CommRead1Byte(PComVar pcv, LPBYTE b)
{
	(void)pcv;
	if (g_pushback_n > 0) {
		*b = g_pushback[--g_pushback_n];
		return 1;
	}
	if (g_feed_pos >= g_feed_len) {
		return 0;
	}
	*b = g_feed[g_feed_pos++];
	return 1;
}

/* ---- reply transcript --------------------------------------------------- */
/*
 * Everything the terminal sends back to the host (DA/DSR responses, DECRQSS
 * replies, mouse reports) lands here. This is half the value of the oracle:
 * escape-sequence conformance is as much about what you answer as what you
 * paint.
 */

static unsigned char *g_reply;
static size_t g_reply_len, g_reply_cap;

static void reply_push(const void *p, size_t n)
{
	if (g_reply_len + n > g_reply_cap) {
		size_t cap = g_reply_cap ? g_reply_cap * 2 : 256;
		while (cap < g_reply_len + n) {
			cap *= 2;
		}
		g_reply = (unsigned char *)realloc(g_reply, cap);
		g_reply_cap = cap;
	}
	memcpy(g_reply + g_reply_len, p, n);
	g_reply_len += n;
}

const unsigned char *oracle_reply(size_t *len)
{
	*len = g_reply_len;
	return g_reply;
}

void oracle_reply_reset(void)
{
	g_reply_len = 0;
}

int WINAPI CommBinaryOut(PComVar pcv, PCHAR B, int C)
{
	(void)pcv;
	if (C > 0) {
		reply_push(B, (size_t)C);
	}
	return C;
}

int WINAPI CommTextOutW(PComVar pcv, const wchar_t *B, int C)
{
	/* The oracle transcript is UTF-8; vtterm hands us UTF-16-ish wchar_t. */
	int i;
	(void)pcv;
	for (i = 0; i < C; i++) {
		char buf[8];
		int n = WideCharToMultiByte(CP_UTF8, 0, &B[i], 1, buf, sizeof(buf), NULL, NULL);
		if (n > 0) {
			reply_push(buf, (size_t)n);
		}
	}
	return C;
}

int WINAPI CommTextEchoW(PComVar pcv, const wchar_t *B, int C)
{
	/* Local echo is a display-side effect, not a host reply. Deliberately
	 * dropped so the transcript stays a clean record of outbound bytes. */
	(void)pcv; (void)B;
	return C;
}

/* ---- recorded window events --------------------------------------------- */

static char g_title[512];

void ChangeTitle(void)
{
	if (cv.TitleRemoteW != NULL) {
		WideCharToMultiByte(CP_UTF8, 0, cv.TitleRemoteW, -1,
		                    g_title, sizeof(g_title), NULL, NULL);
	}
}

const char *oracle_title(void)
{
	return g_title;
}

/* ---- geometry ----------------------------------------------------------- */

/*
 * WinWidth/WinHeight are the VISIBLE WINDOW size in cells; NumOfColumns and
 * NumOfLines are the TERMINAL size. They are not the same thing, and only
 * BuffChangeTerminalSize owns the latter.
 *
 * The real vtdisp.c:2082 sets just WinWidth/WinHeight (plus pixel geometry and
 * scrollbars) and never calls back into buffer.c. An earlier version of this
 * stub called BuffChangeWinSize here and recursed infinitely against
 * buffer.c:4956.
 */
void DispChangeWinSize(vtdraw_t *vt, int Nx, int Ny)
{
	(void)vt;
	WinWidth = Nx;
	WinHeight = Ny;
}

void DispGetCellSize(vtdraw_t *vt, int *width, int *height)
{
	/* Nominal 8x16 cell. Only pixel-mode mouse reports observe this, and
	 * fixing it makes those reports reproducible. */
	(void)vt;
	if (width != NULL) {
		*width = 8;
	}
	if (height != NULL) {
		*height = 16;
	}
}

void DispGetWindowSize(vtdraw_t *vt, int *width, int *height, BOOL client)
{
	(void)vt; (void)client;
	if (width != NULL) {
		*width = NumOfColumns * 8;
	}
	if (height != NULL) {
		*height = NumOfLines * 16;
	}
}

void DispGetRootWinSize(vtdraw_t *vt, int *x, int *y, BOOL inPixels)
{
	(void)vt;
	if (x != NULL) {
		*x = inPixels ? 1920 : 240;
	}
	if (y != NULL) {
		*y = inPixels ? 1080 : 67;
	}
}

void DispGetWindowPos(vtdraw_t *vt, int *x, int *y, BOOL client)
{
	(void)vt; (void)client;
	if (x != NULL) {
		*x = 0;
	}
	if (y != NULL) {
		*y = 0;
	}
}

BOOL DispWindowIconified(vtdraw_t *vt)
{
	(void)vt;
	return FALSE;
}

/* ---- colours ------------------------------------------------------------ */
/*
 * Palette state lives here rather than in a stub because OSC 4/10/11 queries
 * read it back, and those replies must be reproducible.
 */

#define ORACLE_NCOLORS 272
static COLORREF g_colors[ORACLE_NCOLORS];
static int g_colors_init;

static void colors_init_once(void)
{
	int i;
	static const unsigned char base[16][3] = {
		{  0,  0,  0}, {205,  0,  0}, {  0,205,  0}, {205,205,  0},
		{  0,  0,238}, {205,  0,205}, {  0,205,205}, {229,229,229},
		{127,127,127}, {255,  0,  0}, {  0,255,  0}, {255,255,  0},
		{ 92, 92,255}, {255,  0,255}, {  0,255,255}, {255,255,255},
	};

	if (g_colors_init) {
		return;
	}
	g_colors_init = 1;
	for (i = 0; i < 16; i++) {
		g_colors[i] = RGB(base[i][0], base[i][1], base[i][2]);
	}
	for (i = 16; i < 232; i++) {
		int n = i - 16;
		int r = (n / 36) % 6, g = (n / 6) % 6, b = n % 6;
		g_colors[i] = RGB(r ? r * 40 + 55 : 0, g ? g * 40 + 55 : 0, b ? b * 40 + 55 : 0);
	}
	for (i = 232; i < 256; i++) {
		int v = (i - 232) * 10 + 8;
		g_colors[i] = RGB(v, v, v);
	}
	g_colors[CS_VT_NORMALFG] = RGB(255, 255, 255);
	g_colors[CS_VT_NORMALBG] = RGB(0, 0, 0);
}

void DispSetColor(vtdraw_t *vt, unsigned int num, COLORREF color)
{
	(void)vt;
	colors_init_once();
	if (num < ORACLE_NCOLORS) {
		g_colors[num] = color;
	}
}

COLORREF DispGetColor(vtdraw_t *vt, unsigned int num)
{
	(void)vt;
	colors_init_once();
	return num < ORACLE_NCOLORS ? g_colors[num] : 0;
}

void DispResetColor(vtdraw_t *vt, unsigned int num)
{
	(void)vt; (void)num;
	g_colors_init = 0;
	colors_init_once();
}

int DispFindClosestColor(vtdraw_t *vt, int red, int green, int blue)
{
	int i, best = 0;
	long bestd = -1;

	(void)vt;
	colors_init_once();
	for (i = 0; i < 256; i++) {
		long dr = (long)GetRValue(g_colors[i]) - red;
		long dg = (long)GetGValue(g_colors[i]) - green;
		long db = (long)GetBValue(g_colors[i]) - blue;
		long d = dr * dr + dg * dg + db * db;
		if (bestd < 0 || d < bestd) {
			bestd = d;
			best = i;
		}
	}
	return best;
}
