/*
 * Sterna oracle — hand-written stubs.
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
/*
 * AutoRepeatMode, AppliKeyMode, AppliCursorMode, AppliEscapeMode and
 * Send8BitMode are NOT here: keyboard.c:54 owns them, and it is compiled now
 * (see keys.c). They used to be stood in for, which meant vtterm.c set a mode
 * and the real key table would never have seen it -- and AppliEscapeMode was
 * declared BOOL here against upstream's int, a type mismatch that links
 * silently in C and would have truncated on a big-endian target.
 *
 * Their initial values come from ResetTerminal(), which the runner calls at
 * startup; that is also where AutoRepeatMode gets its TRUE.
 */
BOOL KeybEnabled = TRUE;
BOOL DDELog = FALSE;

/*
 * ShiftKey()/ControlKey()/AltKey() are NOT defined here.
 *
 * They used to be, as BOOL *variables* while keyboard.h declares them as
 * functions -- which links, C having no cross-TU type check for it, and then
 * jumps into the data section the first time anything calls one. Nothing did,
 * because no headless run reached the mouse path.
 *
 * They now come from keyboard.c itself (compiled in keys.c), which is
 * strictly better: upstream's own definitions, including MetaKey()'s
 * left/right variants, all resting on one Win32 primitive. So the settable
 * thing here is GetAsyncKeyState, and everything above it is real code.
 */
static BYTE g_key_state[256];

SHORT GetAsyncKeyState(int vk)
{
	/* Bit 15 is "down now". ShiftKey() and friends test & 0xFFFFFF80, so
	 * anything above bit 6 does; the high bit is the honest one. */
	if (vk < 0 || vk > 255) {
		return 0;
	}
	return g_key_state[vk] ? (SHORT)0x8000 : 0;
}

BOOL SetKeyboardState(PBYTE state)
{
	if (state != NULL) {
		memcpy(g_key_state, state, sizeof(g_key_state));
	}
	return TRUE;
}

void oracle_set_modifiers(int shift, int control, int alt)
{
	memset(g_key_state, 0, sizeof(g_key_state));
	g_key_state[VK_SHIFT] = shift ? 1 : 0;
	g_key_state[VK_CONTROL] = control ? 1 : 0;
	/* VK_MENU is Alt. Set the left variant too, so MetaKey(IdMetaLeft)
	 * answers consistently with AltKey(). */
	g_key_state[VK_MENU] = alt ? 1 : 0;
	g_key_state[VK_LMENU] = alt ? 1 : 0;
}


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

/*
 * The TEXT output path, as opposed to the binary one.
 *
 * The difference is not cosmetic: text goes through MakeOutputString and its
 * OutControl callback (ttcmn.c:800), which expands a CR according to
 * cv.CRSend -- bare CR, CR LF, or a lone LF. That is what makes LNM
 * (`CSI 20 h`, which sets cv.CRSend = IdCRLF) reach the keypad Enter key, the
 * one key whose numeric form upstream marks IdText for exactly this reason.
 *
 * ttcmn.c is a whole DLL and is not compiled here, so this reproduces the CR
 * arm of OutControl and nothing else. The other two arms it has -- BS and
 * ctrl-U -- only differ under TelLineMode, which needs a telnet session.
 */
int WINAPI CommTextOutW(PComVar pcv, const wchar_t *B, int C)
{
	/* The oracle transcript is UTF-8; vtterm hands us UTF-16-ish wchar_t. */
	int i;
	for (i = 0; i < C; i++) {
		char buf[8];
		int n;
		if (B[i] == 0x0d) {
			if (pcv != NULL && pcv->CRSend == IdCRLF) {
				reply_push("\r\n", 2);
			} else if (pcv != NULL && pcv->CRSend == IdLF) {
				reply_push("\n", 1);
			} else {
				reply_push("\r", 1);
			}
			continue;
		}
		n = WideCharToMultiByte(CP_UTF8, 0, &B[i], 1, buf, sizeof(buf), NULL, NULL);
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

/*
 * vtdisp.c:2057 / :2066. The real pair is a single static BOOL, initialised
 * TRUE (`vtdisp.c:139`) and only ever assigned; the drawing it gates is
 * elsewhere. DECRQM reports it for DECTCEM, so a stub returning 0 would have
 * made `CSI ? 25 h` answer "reset" forever -- and a differential suite would
 * then have taught the port to agree with the stub.
 */
static BOOL CaretEnabled = TRUE;

void DispEnableCaret(vtdraw_t *vt, BOOL On)
{
	(void)vt;
	CaretEnabled = On;
}

BOOL IsCaretEnabled(void)
{
	return CaretEnabled;
}

void DispGetCellSize(vtdraw_t *vt, int *width, int *height)
{
	/* Nominal 8x16 cell. Only pixel-mode mouse reports observe this, and
	 * fixing it makes those reports reproducible. */
	(void)vt;
	if (width != NULL) {
		*width = ORACLE_CELL_W;
	}
	if (height != NULL) {
		*height = ORACLE_CELL_H;
	}
}

/*
 * vtdisp.c:1719 / :1735, with the nominal cell above in place of the real
 * font metrics. The generated stubs left both of these empty -- they did not
 * even store through their out-parameters, so MouseReport read an
 * uninitialised x and y off the stack. That is invisible until a mouse event
 * arrives, which is exactly the shape of stub AGENTS.md warns about.
 *
 * The window origin is taken as (0,0) rather than (WinOrgX,WinOrgY), and that
 * is deliberate. WinOrgY is a *scrollback viewport* offset: buffer.c:3865
 * drives it negative on every scroll so the visible rows stay put, and
 * vtdisp.c then restores it -- but vtdisp.c is not compiled here, so it only
 * ever drifts. Adding it back made a click six rows above the screen after six
 * lines of scrolling. The oracle's dump is always the current page, which is
 * exactly the state a real Tera Term reports with WinOrgY == 0.
 */
void DispConvWinToScreen(vtdraw_t *vt, int Xw, int Yw, int *Xs, int *Ys, PBOOL Right)
{
	(void)vt;
	if (Xs != NULL) {
		*Xs = Xw / ORACLE_CELL_W;
	}
	if (Ys != NULL) {
		*Ys = Yw / ORACLE_CELL_H;
	}
	if (Xs != NULL && Right != NULL) {
		*Right = (Xw - *Xs * ORACLE_CELL_W) >= ORACLE_CELL_W / 2;
	}
}

void DispConvScreenToWin(vtdraw_t *vt, int Xs, int Ys, int *Xw, int *Yw)
{
	(void)vt;
	if (Xw != NULL) {
		*Xw = Xs * ORACLE_CELL_W;
	}
	if (Yw != NULL) {
		*Yw = Ys * ORACLE_CELL_H;
	}
}

/*
 * The notional window: no chrome, at the origin, on a 1920x1080 work area,
 * with the nominal cell above. XTWINOPS' reports (CSI 11/13/14/15/16/19 t) are
 * answered from it, and there is no honest alternative -- a headless build has
 * no window, and GetWindowRect has nothing to measure.
 *
 * What it does buy is an adjudicable answer: tt_vt::WindowMetrics defaults to
 * exactly these numbers, so esctest/run_diff.sh compares the two engines on
 * the *logic* -- which reports are gated by which flag, which sub-parameters
 * mean the frame and which the text area, and which of the two axes each
 * report prints first -- rather than on a desktop neither of them has.
 *
 * Note that DispShowWindow, DispMoveWindow and DispResizeWin are generated
 * no-op stubs, so the window never actually moves. That is faithful to a
 * headless run and not a gap: a CSI 3;x;y t followed by CSI 13 t reports the
 * origin in both engines, and on Wayland it would in a real one too.
 */
#define ORACLE_SCREEN_W 1920
#define ORACLE_SCREEN_H 1080

void DispGetWindowSize(vtdraw_t *vt, int *width, int *height, BOOL client)
{
	/* No chrome, so the frame and the text area are the same rectangle. */
	(void)vt; (void)client;
	if (width != NULL) {
		*width = NumOfColumns * ORACLE_CELL_W;
	}
	if (height != NULL) {
		*height = NumOfLines * ORACLE_CELL_H;
	}
}

void DispGetRootWinSize(vtdraw_t *vt, int *x, int *y, BOOL inPixels)
{
	/* vtdisp.c:3713 subtracts this window's own chrome from the work area
	 * before dividing by the cell. Written out rather than folded into a
	 * constant: the chrome is zero here, and hardcoding the quotient would
	 * hide the day it stops being. */
	int win_w, win_h, client_w, client_h;

	DispGetWindowSize(vt, &win_w, &win_h, FALSE);
	DispGetWindowSize(vt, &client_w, &client_h, TRUE);

	if (x != NULL) {
		*x = inPixels ? ORACLE_SCREEN_W
		              : (ORACLE_SCREEN_W - (win_w - client_w)) / ORACLE_CELL_W;
	}
	if (y != NULL) {
		*y = inPixels ? ORACLE_SCREEN_H
		              : (ORACLE_SCREEN_H - (win_h - client_h)) / ORACLE_CELL_H;
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
 * vtdisp.c's DispSetColor / DispGetColor / DispResetColor, transcribed.
 *
 * These live here because vtdisp.c is not compiled into the oracle, and they
 * are a transcription rather than something convenient because that file is
 * the only specification for OSC 4/5/10-19 and their resets — vtterm.c does
 * the parsing and then hands every decision to these three.
 *
 * The version this replaces was convenient, and it lied in three ways at once:
 * one flat array indexed by the CS_ number, so a dynamic colour could be read
 * back after it was set; no eight-colour permutation; and a DispResetColor
 * that ignored its argument and put the whole table back. Same trap as
 * DispFindClosestColor below, which held xterm's palette until it was caught.
 *
 * The split upstream keeps is the whole point of them. `vt->` is what the
 * window paints with and `ts.` is what the settings asked for; the setter
 * writes the first, the reset copies the second over the first, and the getter
 * reads — for every colour except the palette and Tek — the *second*. So a
 * host cannot read back a dynamic colour it just set. That is upstream's
 * behaviour and this reproduces it; see PLAN.md.
 */

/*
 * `vtdraw_t`'s live colours. Upstream's names, upstream's [fg, bg] order.
 */
static COLORREF g_ansi[256];
static COLORREF g_vt[2], g_bold[2], g_blink[2], g_reverse[2], g_url[2], g_under[2];
static int g_colors_init;

/* vtdisp.c:1400. Its own inverse, which is why GetIndex16From256 calls it. */
static int oracle_index256_from16(int index16)
{
	static const int index256[] = { 0, 9, 10, 11, 12, 13, 14, 15, 8, 1, 2, 3, 4, 5, 6, 7 };
	return index16 < (int)(sizeof(index256) / sizeof(index256[0])) ? index256[index16] : index16;
}

/*
 * vtdisp.c:1429 InitColorTable, over `DefaultColorTable` above 15.
 *
 * ts.ANSIColor is the legacy sixteen, ordered dim-then-bright, so it is
 * permuted on the way in. Entries 16.. are the xterm cube and greyscale ramp
 * and are not configurable — ttset.c:797 masks a colour id to four bits, so
 * the file cannot reach past 15.
 */
static void oracle_init_color_table(void)
{
	int i;

	for (i = 0; i < 16; i++) {
		g_ansi[oracle_index256_from16(i)] = ts.ANSIColor[i];
	}
	for (i = 16; i < 232; i++) {
		int n = i - 16;
		int r = (n / 36) % 6, g = (n / 6) % 6, b = n % 6;
		g_ansi[i] = RGB(r ? r * 40 + 55 : 0, g ? g * 40 + 55 : 0, b ? b * 40 + 55 : 0);
	}
	for (i = 232; i < 256; i++) {
		int v = (i - 232) * 10 + 8;
		g_ansi[i] = RGB(v, v, v);
	}
}

/* vtdisp.c:1154 BGInitialize, which is InitColorTable plus BGSetDefaultColor. */
static void colors_init_once(void)
{
	int i;

	if (g_colors_init) {
		return;
	}
	g_colors_init = 1;
	oracle_init_color_table();
	for (i = 0; i < 2; i++) {
		g_vt[i] = ts.VTColor[i];
		g_bold[i] = ts.VTBoldColor[i];
		g_blink[i] = ts.VTBlinkColor[i];
		g_reverse[i] = ts.VTReverseColor[i];
		g_url[i] = ts.URLColor[i];
		g_under[i] = ts.VTUnderlineColor[i];
	}
}

/* vtdisp.c:3376. */
void DispSetColor(vtdraw_t *vt, unsigned int num, COLORREF color)
{
	(void)vt;
	colors_init_once();
	switch (num) {
	case CS_VT_NORMALFG:  g_vt[0] = color; break;
	case CS_VT_NORMALBG:  g_vt[1] = color; break;
	case CS_VT_BOLDFG:    g_bold[0] = color; break;
	case CS_VT_BOLDBG:    g_bold[1] = color; break;
	case CS_VT_BLINKFG:   g_blink[0] = color; break;
	case CS_VT_BLINKBG:   g_blink[1] = color; break;
	case CS_VT_REVERSEFG: g_reverse[0] = color; break;
	case CS_VT_REVERSEBG: g_reverse[1] = color; break;
	case CS_VT_URLFG:     g_url[0] = color; break;
	case CS_VT_URLBG:     g_url[1] = color; break;
	case CS_VT_UNDERFG:   g_under[0] = color; break;
	case CS_VT_UNDERBG:   g_under[1] = color; break;
	/* Tek has no live copy: the setter writes the *setting*, which is why a
	 * Tek colour is the one special colour a query can read back. */
	case CS_TEK_FG:       ts.TEKColor[0] = color; break;
	case CS_TEK_BG:       ts.TEKColor[1] = color; break;
	default:
		if (num <= 255) {
			if ((ts.ColorFlag & CF_FULLCOLOR) == 0) {
				g_ansi[oracle_index256_from16((int)num)] = color;
			}
			else {
				g_ansi[num] = color;
			}
		}
		break;
	}
}

/* vtdisp.c:3561 — and every special colour comes out of `ts`, not `vt`. */
COLORREF DispGetColor(vtdraw_t *vt, unsigned int num)
{
	(void)vt;
	colors_init_once();
	switch (num) {
	case CS_VT_NORMALFG:  return ts.VTColor[0];
	case CS_VT_NORMALBG:  return ts.VTColor[1];
	case CS_VT_BOLDFG:    return ts.VTBoldColor[0];
	case CS_VT_BOLDBG:    return ts.VTBoldColor[1];
	case CS_VT_BLINKFG:   return ts.VTBlinkColor[0];
	case CS_VT_BLINKBG:   return ts.VTBlinkColor[1];
	case CS_VT_REVERSEFG: return ts.VTReverseColor[0];
	case CS_VT_REVERSEBG: return ts.VTReverseColor[1];
	case CS_VT_URLFG:     return ts.URLColor[0];
	case CS_VT_URLBG:     return ts.URLColor[1];
	case CS_VT_UNDERFG:   return ts.VTUnderlineColor[0];
	case CS_VT_UNDERBG:   return ts.VTUnderlineColor[1];
	case CS_TEK_FG:       return ts.TEKColor[0];
	case CS_TEK_BG:       return ts.TEKColor[1];
	default:
		if (num <= 255) {
			if ((ts.ColorFlag & CF_FULLCOLOR) == 0) {
				return g_ansi[oracle_index256_from16((int)num)];
			}
			return g_ansi[num];
		}
		return g_ansi[0];
	}
}

/* vtdisp.c:3456. Both Tek arms are empty upstream, and so are they here. */
void DispResetColor(vtdraw_t *vt, unsigned int num)
{
	int i;

	(void)vt;
	colors_init_once();
	if (num == CS_UNSPEC) {
		return;
	}
	switch (num) {
	case CS_VT_NORMALFG:  g_vt[0] = ts.VTColor[0]; break;
	case CS_VT_NORMALBG:  g_vt[1] = ts.VTColor[1]; break;
	case CS_VT_BOLDFG:    g_bold[0] = ts.VTBoldColor[0]; break;
	case CS_VT_BOLDBG:    g_bold[1] = ts.VTBoldColor[1]; break;
	case CS_VT_BLINKFG:   g_blink[0] = ts.VTBlinkColor[0]; break;
	case CS_VT_BLINKBG:   g_blink[1] = ts.VTBlinkColor[1]; break;
	case CS_VT_REVERSEFG: g_reverse[0] = ts.VTReverseColor[0]; break;
	case CS_VT_REVERSEBG: g_reverse[1] = ts.VTReverseColor[1]; break;
	case CS_VT_URLFG:     g_url[0] = ts.URLColor[0]; break;
	case CS_VT_URLBG:     g_url[1] = ts.URLColor[1]; break;
	case CS_VT_UNDERFG:   g_under[0] = ts.VTUnderlineColor[0]; break;
	case CS_VT_UNDERBG:   g_under[1] = ts.VTUnderlineColor[1]; break;
	case CS_TEK_FG:
	case CS_TEK_BG:
		break;
	case CS_ANSICOLOR_ALL:
		oracle_init_color_table();
		break;
	/* Three colours, not the special set — and not the underline, which
	 * OSC 5 can set and OSC 105 therefore cannot put back. */
	case CS_SP_ALL:
		g_bold[0] = ts.VTBoldColor[0];
		g_blink[0] = ts.VTBlinkColor[0];
		g_reverse[1] = ts.VTReverseColor[1];
		break;
	case CS_ALL:
		for (i = 0; i < 2; i++) {
			g_vt[i] = ts.VTColor[i];
			g_bold[i] = ts.VTBoldColor[i];
			g_blink[i] = ts.VTBlinkColor[i];
			g_reverse[i] = ts.VTReverseColor[i];
			g_url[i] = ts.URLColor[i];
			g_under[i] = ts.VTUnderlineColor[i];
		}
		oracle_init_color_table();
		break;
	default:
		if (num <= 15) {
			if ((ts.ColorFlag & CF_FULLCOLOR) == 0) {
				g_ansi[oracle_index256_from16((int)num)] = ts.ANSIColor[num];
			}
			else {
				g_ansi[num] = ts.ANSIColor[oracle_index256_from16((int)num)];
			}
		}
		else if (num <= 255) {
			int n = (int)num - 16;
			if (num <= 231) {
				int r = (n / 36) % 6, g = (n / 6) % 6, b = n % 6;
				g_ansi[num] = RGB(r ? r * 40 + 55 : 0, g ? g * 40 + 55 : 0,
				                  b ? b * 40 + 55 : 0);
			}
			else {
				int v = ((int)num - 232) * 10 + 8;
				g_ansi[num] = RGB(v, v, v);
			}
		}
		break;
	}
}

/*
 * vtdisp.c:DispFindClosestColor, reproduced rather than approximated.
 *
 * Two details the earlier version dropped, both of which change the answer:
 * the out-of-range guard returns -1 (which the SGR parser treats as "no
 * colour"), and the result is flipped between the bright and dim halves of the
 * base 16 when any full-colour mode is on. That flip looks like a bug — it maps
 * pure red to index 1, "dark red" — but the drawing path applies the inverse
 * when it turns a sequence index back into a palette index, so the round trip
 * is what matters, and this is the half the buffer stores.
 */
int DispFindClosestColor(vtdraw_t *vt, int red, int green, int blue)
{
	int i, best = 0;
	long bestd = -1;

	(void)vt;

	if (red < 0 || red > 255 || green < 0 || green > 255 || blue < 0 || blue > 255) {
		return -1;
	}

	colors_init_once();
	for (i = 0; i < 256; i++) {
		long dr = (long)GetRValue(g_ansi[i]) - red;
		long dg = (long)GetGValue(g_ansi[i]) - green;
		long db = (long)GetBValue(g_ansi[i]) - blue;
		long d = dr * dr + dg * dg + db * db;
		if (bestd < 0 || d < bestd) {
			bestd = d;
			best = i;
		}
	}

	if ((ts.ColorFlag & CF_FULLCOLOR) != 0 && best < 16 && (best & 7) != 0) {
		best ^= 8;
	}
	return best;
}
