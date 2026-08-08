/*
 * termitta oracle — Tera Term's real key table, driven headless.
 *
 * The frontend seam has two halves. Bytes in are the escape parser; bytes out
 * are the keyboard, and this is that half: given a key and the terminal's
 * current modes, what does Tera Term put on the wire?
 *
 * WHY THIS FILE #includes A .c FILE
 *
 * The whole table lives in `GetKeyStr()` — 74 key cases, each spelling out its
 * escape sequence three times over for application-cursor, application-keypad
 * and 8-bit-controls mode. It is `static`, and it is also *exactly* the thing
 * a reimplementation must agree with; a transcription of 74x3 string literals
 * is precisely where a port silently goes wrong. Including the translation
 * unit reaches it without touching upstream, which ground rule 1 forbids, and
 * without retyping the table, which ground rule 2 warns about.
 *
 * The alternative was to drive `KeyCodeSend()`, which is public. It routes
 * through `SendMemBinary`/`SendMemStart` — the delayed-send queue — so it
 * would have dragged an async subsystem and its stubs into the answer for no
 * gain. The bytes are decided before any of that.
 *
 * `keyboard.c` is therefore compiled HERE and must not also appear in the
 * Makefile's TT_C, or every symbol in it is defined twice.
 */
#include <windows.h>

#include "teraterm.h"
#include "tttypes.h"
#include "tttypes_key.h"

#include "oracle.h"

/* keyboard.c reaches these; nothing on the GetKeyStr path calls them. */
extern TTTSet ts;
extern BOOL AppliKeyMode, AppliCursorMode;
extern BOOL Send8BitMode;

#include "keyboard.c"

/*
 * The key map is normally read from KEYBOARD.CNF, which maps a *scan code* to
 * one of the key ids in tttypes_key.h. There is no CNF here and no keyboard,
 * so the oracle addresses keys by id directly and only needs a map that is
 * consistent with itself.
 */
static TKeyMap oracle_keymap;

void oracle_key_send(int key_code)
{
	wchar_t code[MAXPATHLEN];
	size_t len = 0;
	UserKeyType_t type = IdBinary;
	unsigned char out[MAXPATHLEN];
	size_t i;

	if (key_code < 1 || key_code > IdKeyMax) {
		return;
	}

	/*
	 * The three mode arguments are copied from KeyCodeSend() rather than
	 * passed in, so the *byte stream* drives them: `CSI ? 1 h` sets
	 * AppliCursorMode in vtterm.c, and the very next key reads it. Both
	 * translation units share these globals exactly as the real program does,
	 * which is what makes a case like "DECCKM then Up" mean anything.
	 */
	GetKeyStr(NULL, &oracle_keymap, (WORD)key_code,
	          AppliKeyMode && !ts.DisableAppKeypad,
	          AppliCursorMode && !ts.DisableAppCursor,
	          Send8BitMode, code, _countof(code), &len, &type);

	if (len == 0) {
		return;
	}
	if (type == IdText) {
		/* Exactly one built-in key takes this path: keypad Enter in numeric
		 * mode, which upstream marks IdText so the CR goes through newline
		 * conversion. Routing it through CommTextOutW is what applies
		 * cv.CRSend, so LNM changes the answer. */
		CommTextOutW(&cv, code, (int)len);
		return;
	}
	if (type != IdBinary) {
		/* IdMacro and IdCommand belong to user-defined keys, which need a
		 * KEYBOARD.CNF to exist at all. Nothing built in produces them. */
		return;
	}

	/* SendBinary()'s narrowing, reproduced: anything above U+00FF becomes
	 * 0xFF. Every built-in key string is ASCII or a C1 control, so this only
	 * matters for user keys. */
	for (i = 0; i < len && i < sizeof(out); i++) {
		out[i] = code[i] < 256 ? (unsigned char)code[i] : 0xff;
	}
	CommBinaryOut(&cv, (PCHAR)out, (int)i);
}

int oracle_key_id(const char *name)
{
	static const struct {
		const char *name;
		int id;
	} table[] = {
		{ "up", IdUp }, { "down", IdDown }, { "right", IdRight }, { "left", IdLeft },
		{ "kp0", Id0 }, { "kp1", Id1 }, { "kp2", Id2 }, { "kp3", Id3 }, { "kp4", Id4 },
		{ "kp5", Id5 }, { "kp6", Id6 }, { "kp7", Id7 }, { "kp8", Id8 }, { "kp9", Id9 },
		{ "kpminus", IdMinus }, { "kpcomma", IdComma }, { "kpperiod", IdPeriod },
		{ "kpslash", IdSlash }, { "kpasterisk", IdAsterisk }, { "kpplus", IdPlus },
		{ "kpenter", IdEnter },
		{ "pf1", IdPF1 }, { "pf2", IdPF2 }, { "pf3", IdPF3 }, { "pf4", IdPF4 },
		{ "find", IdFind }, { "insert", IdInsert }, { "remove", IdRemove },
		{ "select", IdSelect }, { "prev", IdPrev }, { "next", IdNext },
		{ "f6", IdF6 }, { "f7", IdF7 }, { "f8", IdF8 }, { "f9", IdF9 },
		{ "f10", IdF10 }, { "f11", IdF11 }, { "f12", IdF12 }, { "f13", IdF13 },
		{ "f14", IdF14 }, { "help", IdHelp }, { "do", IdDo }, { "f17", IdF17 },
		{ "f18", IdF18 }, { "f19", IdF19 }, { "f20", IdF20 },
		{ "xf1", IdXF1 }, { "xf2", IdXF2 }, { "xf3", IdXF3 }, { "xf4", IdXF4 },
		{ "xf5", IdXF5 },
		{ "hold", IdHold }, { "print", IdPrint }, { "break", IdBreak },
		{ "backtab", IdXBackTab },
		{ NULL, 0 },
	};
	int i;

	for (i = 0; table[i].name != NULL; i++) {
		if (strcmp(table[i].name, name) == 0) {
			return table[i].id;
		}
	}
	return 0;
}
