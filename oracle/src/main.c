/*
 * termitta oracle — runner.
 *
 * Reads a byte stream, drives Tera Term's real VT state machine over it, and
 * writes a stable textual dump of the resulting screen. The dump is the
 * differential-test contract between Tera Term and the Rust reimplementation,
 * so the format is deliberately line-oriented and diff-friendly.
 *
 *   oracle [--cols N] [--rows N] [--term ID] [--attrs] [--scrollback] [FILE]
 *
 * With no FILE, reads stdin. Output goes to stdout.
 */
#include <windows.h>

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "teraterm.h"
#include "tttypes.h"
#include "tttypes_termid.h"
#include "ttcommon.h"
#include "buffer.h"
#include "vtdisp.h"
#include "vtterm.h"
#include "ttlib_charset.h"
#include "makeoutputstring.h"
#include "unicode.h"

#include "oracle.h"

extern TTTSet ts;
extern TComVar cv;

/* ---- settings ----------------------------------------------------------- */

/*
 * Tera Term populates TTTSet while parsing TERATERM.INI, with defaults applied
 * per-key as it goes; there is no standalone defaults function to borrow. The
 * values below mirror ttpset/ttset.c's fallbacks for the 67 fields vtterm.c
 * and buffer.c actually read. Everything else stays zero, which is also
 * ttset.c's behaviour for the flag words.
 */
/*
 * Resolve --term.
 *
 * NOT TermIDGetID(): that is a case-sensitive strcmp against an UPPERCASE table
 * and it returns IdVT100 for anything it does not recognise rather than an
 * error, so `--term vt220` silently ran as a VT100 and the guard below could
 * never fire. Caught by the differential suite in 2026-08: the Rust engine
 * answered DA as a VT220 and the oracle as a VT100, and the oracle was wrong.
 *
 * Walk the same table case-insensitively and refuse what is not in it, so a
 * typo is a failed run rather than a quietly downgraded terminal.
 */
static int resolve_term_id(const char *name)
{
	int i;

	for (i = 0; ; i++) {
		const TermIDList *e = TermIDGetList(i);
		const char *a, *b;

		if (e == NULL) {
			break;
		}
		for (a = name, b = e->TermIDStr; *a != '\0' && *b != '\0'; a++, b++) {
			if (tolower((unsigned char)*a) != tolower((unsigned char)*b)) {
				break;
			}
		}
		if (*a == '\0' && *b == '\0') {
			return e->TermID;
		}
	}
	return 0;
}

static void settings_defaults(int cols, int rows, const char *term_id, int cr_receive)
{
	memset(&ts, 0, sizeof(ts));
	memset(&cv, 0, sizeof(cv));

	ts.TerminalWidth = cols;
	ts.TerminalHeight = rows;
	ts.TerminalID = resolve_term_id(term_id);
	if (ts.TerminalID == 0) {
		fprintf(stderr, "oracle: unknown terminal id '%s'\n", term_id);
		exit(2);
	}

	ts.KanjiCode = IdUTF8;
	ts.KanjiCodeSend = IdUTF8;
	/* ttset.c:629-644 — with no INI key present, both fall to the else
	 * branch and become IdCR. Do not "improve" these to IdCRLF: the oracle's
	 * job is to reproduce shipped Tera Term, and newline handling silently
	 * shifts every row in the dump. Override with --crreceive. */
	ts.CRReceive = cr_receive;
	ts.CRSend = IdCR;
	ts.ScrollBuffSize = 100;        /* ttset.c:750 */
	ts.ScrollBuffMax = 10000;       /* ttset.c:1213 */
	ts.MaxOSCBufferSize = 4096;     /* ttset.c:1789 */
	ts.TabStopFlag = TABF_ALL;      /* ttset.c:1719, key default "on" */
	ts.Beep = IdBeepOff;            /* silence, and keeps runs deterministic */
	ts.CursorShape = IdBlkCur;      /* ttset.c:725, the else branch */
	ts.BSKey = IdBS;                /* ttset.c:882, the else branch */
	ts.AutoWinResize = FALSE;
	ts.EnableScrollBuff = 1;
	ts.SelectStartDelay = 0;
	ts.ScrollWindowClearScreen = TRUE;      /* ttset.c:1444 */
	/* ttset.c:1568 defaults this to "overwrite", not off. */
	ts.AcceptTitleChangeRequest = IdTitleChangeRequestOverwrite;

	/*
	 * THE FLAG WORDS ARE NOT ZERO.
	 *
	 * Every one of these is initialised to 0 near the top of ttset.c and then
	 * built up, key by key, from per-key defaults further down. Reading only
	 * the initialiser — which is what this function used to do — gives a
	 * terminal with 256-colour off, ISO-2022 shifts off, 8-bit controls off
	 * and the alternate screen off, none of which is how Tera Term ships.
	 * The oracle then reports that as ground truth and the port copies it.
	 *
	 * Corrected 2026-08 after the ISO-2022 gap turned up while porting
	 * character sets. Each bit below is a key whose GetOnOff default is TRUE;
	 * keys defaulting to FALSE are deliberately absent, not forgotten.
	 */

	/* ttset.c:743 Xterm256Color=on, :857 EnableANSIColor=on, :759 :764 :777
	 * :785 the attribute-colour keys. PcBoldColor and Aixterm16Color default
	 * off, so CF_FULLCOLOR is NOT the answer here. */
	ts.ColorFlag = CF_XTERM256 | CF_ANSICOLOR | CF_BOLDCOLOR | CF_BLINKCOLOR |
	               CF_URLCOLOR | CF_UNDERLINE;

	/* ttset.c:1875 — the key default string is "on", which means
	 * ISO2022_SHIFT_ALL. SO/SI/SS2/SS3 and every locking shift are live. */
	ts.ISO2022Flag = ISO2022_SHIFT_ALL;

	/* ttset.c:1075 Accept8BitCtrl=on, :1159 CtrlInKanji=on, :1188
	 * EnableStatusLine=on, :1681 AlternateScreenBuffer=on, :1711 LockTUID=on,
	 * :1950 ClearScrollBufferFromRemote=on. */
	ts.TermFlag = TF_ACCEPT8BITCTRL | TF_CTRLINKANJI | TF_ENABLESLINE |
	              TF_ALTSCR | TF_LOCKTUID | TF_REMOTECLEARSBUFF;

	/* ttset.c:1653 WindowCtrlSequence=on, :1661 WindowReportSequence=on.
	 * CursorCtrlSequence defaults off; TitleReportSequence defaults "Empty",
	 * which sets no WF_TITLEREPORT bit. */
	ts.WindowFlag = WF_WINDOWCHANGE | WF_WINDOWREPORT;

	/* ttset.c:1537 DecSpMappingDir defaults to IdDecSpecialDoNot, not the
	 * IdDecSpecialUniToDec that a zeroed struct produces. :1546
	 * UnicodeToDecSpMapping defaults to 3. The pair matters: with DoNot the
	 * mapping is inert, so DEC special characters keep their raw byte value
	 * and carry AttrSpecial instead of becoming U+25xx box drawing. */
	ts.Dec2Unicode = IdDecSpecialDoNot;
	ts.UnicodeDecSpMapping = 3;

	cv.Ready = TRUE;
	cv.CRSend = ts.CRSend;
	cv.KanjiCodeEcho = ts.KanjiCode;
	cv.KanjiCodeSend = ts.KanjiCodeSend;
	cv.PortType = IdTCPIP;

	/* vtterm.c calls MakeOutputStringInit() on both of these and asserts they
	 * are non-NULL. The real app allocates them in vtwin.cpp:737. */
	cv.StateSend = MakeOutputStringCreate();
	cv.StateEcho = MakeOutputStringCreate();
}

/* ---- dumping ------------------------------------------------------------ */

static void put_utf8(FILE *out, unsigned int cp)
{
	if (cp < 0x80) {
		fputc((int)cp, out);
	} else if (cp < 0x800) {
		fputc((int)(0xC0 | (cp >> 6)), out);
		fputc((int)(0x80 | (cp & 0x3F)), out);
	} else if (cp < 0x10000) {
		fputc((int)(0xE0 | (cp >> 12)), out);
		fputc((int)(0x80 | ((cp >> 6) & 0x3F)), out);
		fputc((int)(0x80 | (cp & 0x3F)), out);
	} else {
		fputc((int)(0xF0 | (cp >> 18)), out);
		fputc((int)(0x80 | ((cp >> 12) & 0x3F)), out);
		fputc((int)(0x80 | ((cp >> 6) & 0x3F)), out);
		fputc((int)(0x80 | (cp & 0x3F)), out);
	}
}

/* One char per cell of SGR state, so attribute diffs read at a glance. */
static char attr_char(const TCharAttr *a)
{
	if (a->Attr & AttrReverse) return 'R';
	if (a->Attr & AttrBold)    return 'B';
	if (a->Attr & AttrUnder)   return 'U';
	if (a->Attr & AttrBlink)   return 'K';
	if (a->Attr & AttrSpecial) return 'S';
	if (a->Attr2 & Attr2Fore)  return 'f';
	if (a->Attr2 & Attr2Back)  return 'b';
	return '.';
}

/*
 * Display columns consumed by a codepoint. Uses Tera Term's own east-asian
 * width tables (unicode.cpp) rather than a private copy, so the dump agrees
 * with the layout the real terminal produced.
 */
static int disp_width(unsigned int cp)
{
	char p;

	if (UnicodeIsCombiningCharacter(cp)) {
		return 0;
	}
	p = UnicodeGetWidthProperty(cp);
	/*
	 * BOTH 'W' (Wide) and 'F' (Fullwidth) are full-width -- that is what
	 * buffer.c:BuffIsHalfWidthFromPropery() says, and the buffer lays cells
	 * out accordingly. Testing only for 'W' here made the dump count every
	 * fullwidth form (U+FF01 onward) as one column while the buffer had
	 * given it two, so the row was padded past its own width and every
	 * comparison against it was wrong. 'A' (Ambiguous) is narrow because
	 * ts.UnicodeAmbiguousWidth is not 2.
	 */
	return (p == 'W' || p == 'F') ? 2 : 1;
}

static void dump(FILE *out, int cols, int rows, const char *term_id, int want_attrs)
{
	/*
	 * BuffGetAnyLineDataW returns a STRING, not one wchar_t per column: a
	 * surrogate pair is two units, combining marks add more, and the padding
	 * cell behind a wide character is skipped entirely. So the buffer must be
	 * sized for the worst case, and the dump must pad by DISPLAY COLUMN
	 * rather than by array index. Getting this wrong truncates the line and
	 * overruns the buffer.
	 */
	size_t line_cap = (size_t)cols * 8 + 8;
	wchar_t *line = (wchar_t *)malloc(line_cap * sizeof(wchar_t));
	size_t reply_len;
	const unsigned char *reply;
	const char *title;
	int y;

	fprintf(out, "# termitta-oracle 1\n");
	fprintf(out, "# term %s %dx%d\n", term_id, cols, rows);
	fprintf(out, "# cursor %d,%d\n", CursorX, CursorY);

	title = oracle_title();
	if (title[0] != '\0') {
		fprintf(out, "# title %s\n", title);
	}

	for (y = 0; y < rows; y++) {
		int n, x, col;

		memset(line, 0, line_cap * sizeof(wchar_t));
		/* BuffGetAnyLineDataW takes an ABSOLUTE buffer index (it calls
		 * GetLinePtr(offset_y) directly), unlike BuffGetCursorCharAttr which
		 * is screen-relative. PageStart maps one to the other. */
		n = BuffGetAnyLineDataW(PageStart + y, line, line_cap);
		if (n < 0) {
			n = 0;
		}
		fprintf(out, "%3d |", y);
		col = 0;
		for (x = 0; x < n && col < cols; x++) {
			unsigned int cp = (unsigned int)line[x];
			int w;

			if (cp == 0) {
				break;
			}
			/* Recombine surrogates: wchar_t is 32-bit here, but buffer.c
			 * stores the UTF-16 form, so pairs arrive as two units. */
			if (cp >= 0xD800 && cp <= 0xDBFF && x + 1 < n) {
				unsigned int lo = (unsigned int)line[x + 1];
				if (lo >= 0xDC00 && lo <= 0xDFFF) {
					cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
					x++;
				}
			}
			w = disp_width(cp);
			if (col + w > cols) {
				break;
			}
			put_utf8(out, cp);
			col += w;
		}
		for (; col < cols; col++) {
			fputc(' ', out);
		}
		fprintf(out, "|\n");
	}

	if (want_attrs) {
		fprintf(out, "# attrs\n");
		for (y = 0; y < rows; y++) {
			int x;
			fprintf(out, "%3d |", y);
			for (x = 0; x < cols; x++) {
				TCharAttr a = BuffGetCursorCharAttr(x, y);
				fputc(attr_char(&a), out);
			}
			fprintf(out, "|\n");
		}
		fprintf(out, "# colors\n");
		for (y = 0; y < rows; y++) {
			int x;
			fprintf(out, "%3d |", y);
			for (x = 0; x < cols; x++) {
				TCharAttr a = BuffGetCursorCharAttr(x, y);
				if ((a.Attr2 & (Attr2Fore | Attr2Back)) == 0) {
					fputc('.', out);
				} else {
					fprintf(out, "%x", (unsigned)(a.Fore & 0xf));
				}
			}
			fprintf(out, "|\n");
		}

		/*
		 * DECSCA's protect bit gets a section of its own, and only when
		 * something is actually protected. It is orthogonal to the rest, so
		 * folding it into attr_char() would hide a protected bold cell behind
		 * its 'B'; emitting it unconditionally would churn every golden for a
		 * bit almost no case sets.
		 */
		{
			int protected_any = 0;
			for (y = 0; y < rows && !protected_any; y++) {
				int x;
				for (x = 0; x < cols; x++) {
					if (BuffGetCursorCharAttr(x, y).Attr2 & Attr2Protect) {
						protected_any = 1;
						break;
					}
				}
			}
			if (protected_any) {
				fprintf(out, "# protect\n");
				for (y = 0; y < rows; y++) {
					int x;
					fprintf(out, "%3d |", y);
					for (x = 0; x < cols; x++) {
						TCharAttr a = BuffGetCursorCharAttr(x, y);
						fputc((a.Attr2 & Attr2Protect) ? 'P' : '.', out);
					}
					fprintf(out, "|\n");
				}
			}
		}
	}

	reply = oracle_reply(&reply_len);
	if (reply_len > 0) {
		size_t i;
		fprintf(out, "# reply ");
		for (i = 0; i < reply_len; i++) {
			unsigned char c = reply[i];
			if (c == 0x1b) {
				fprintf(out, "<ESC>");
			} else if (c < 0x20 || c >= 0x7f) {
				fprintf(out, "<%02x>", c);
			} else {
				fputc(c, out);
			}
		}
		fprintf(out, "\n");
	}

	free(line);
}

/* ---- main --------------------------------------------------------------- */

int main(int argc, char **argv)
{
	int cols = 80, rows = 24, want_attrs = 0, cr_receive = IdCR;
	const char *term_id = "vt100";
	const char *path = NULL;
	unsigned char *input;
	size_t len = 0, cap = 1 << 16;
	FILE *in;
	int i, guard;

	for (i = 1; i < argc; i++) {
		if (strcmp(argv[i], "--cols") == 0 && i + 1 < argc) {
			cols = atoi(argv[++i]);
		} else if (strcmp(argv[i], "--rows") == 0 && i + 1 < argc) {
			rows = atoi(argv[++i]);
		} else if (strcmp(argv[i], "--term") == 0 && i + 1 < argc) {
			term_id = argv[++i];
		} else if (strcmp(argv[i], "--attrs") == 0) {
			want_attrs = 1;
		} else if (strcmp(argv[i], "--crreceive") == 0 && i + 1 < argc) {
			const char *v = argv[++i];
			if (strcmp(v, "cr") == 0)        cr_receive = IdCR;
			else if (strcmp(v, "lf") == 0)   cr_receive = IdLF;
			else if (strcmp(v, "crlf") == 0) cr_receive = IdCRLF;
			else if (strcmp(v, "auto") == 0) cr_receive = IdAUTO;
			else { fprintf(stderr, "oracle: bad --crreceive '%s'\n", v); return 2; }
		} else if (strcmp(argv[i], "--help") == 0) {
			fprintf(stderr,
			        "usage: oracle [--cols N] [--rows N] [--term ID] [--attrs]\n"
			        "              [--crreceive cr|lf|crlf|auto] [FILE]\n");
			return 0;
		} else if (argv[i][0] != '-') {
			path = argv[i];
		} else {
			fprintf(stderr, "oracle: unknown option '%s'\n", argv[i]);
			return 2;
		}
	}

	if (cols < 1 || cols > TermWidthMax || rows < 1 || rows > TermHeightMax) {
		fprintf(stderr, "oracle: size %dx%d out of range\n", cols, rows);
		return 2;
	}

	in = path ? fopen(path, "rb") : stdin;
	if (in == NULL) {
		fprintf(stderr, "oracle: cannot open %s\n", path);
		return 2;
	}
	input = (unsigned char *)malloc(cap);
	for (;;) {
		size_t n;
		if (len == cap) {
			cap *= 2;
			input = (unsigned char *)realloc(input, cap);
		}
		n = fread(input + len, 1, cap - len, in);
		if (n == 0) {
			break;
		}
		len += n;
	}
	if (path) {
		fclose(in);
	}

	settings_defaults(cols, rows, term_id, cr_receive);
	oracle_clock_set_frozen(1);

	NumOfColumns = cols;
	NumOfLines = rows;
	WinWidth = cols;
	WinHeight = rows;

	InitBuffer(IdVtDrawAPIUnicode);
	/* buffer.c:134 hardcodes CodePage = 932 (Shift-JIS). The oracle is
	 * UTF-8, and this drives the per-cell ansi_char shadow. */
	BuffSetDispCodePage(CP_UTF8);
	ResetTerminal();
	BuffChangeTerminalSize(cols, rows);

	oracle_feed(input, len);

	/* VTParse consumes what it can per call and returns 0 when the input is
	 * exhausted; the guard is a backstop against a state machine that stops
	 * making progress rather than a normal exit path. */
	guard = 0;
	while (oracle_feed_remaining() > 0) {
		if (VTParse() == 0 && oracle_feed_remaining() > 0) {
			if (++guard > 1000) {
				fprintf(stderr, "oracle: parser stalled with %d bytes left\n",
				        oracle_feed_remaining());
				return 3;
			}
		} else {
			guard = 0;
		}
	}

	/*
	 * The LIVE size, not the one from argv: XTWINOPS `CSI 8;h;w t` changes
	 * NumOfColumns/NumOfLines mid-stream, and dumping the startup size after
	 * that pads every row past its own width and reports a header the terminal
	 * has outgrown.
	 */
	dump(stdout, NumOfColumns, NumOfLines, term_id, want_attrs);

	free(input);
	return 0;
}
