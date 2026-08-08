/*
 * termitta oracle — minimal <windows.h> shim for POSIX.
 *
 * Purpose: let Tera Term's vtterm.c (VT state machine) and buffer.c (grid +
 * scrollback) compile and run on Linux unmodified, so they can serve as a
 * differential-test oracle for the Rust reimplementation.
 *
 * This is deliberately NOT a Win32 emulation. It provides only the types and
 * the three functions those two translation units actually reach for:
 * Sleep, GetTickCount, WideCharToMultiByte.
 */
#pragma once

#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <wchar.h>

#include "msvc_crt.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ---- calling conventions and MSVC decorations (no-ops off Windows) ---- */
#define WINAPI
#define APIENTRY
#define CALLBACK
#define PASCAL
#define WINAPIV
#define __cdecl
#define far
#define near
#define __declspec(x)

/* ---- scalar types ---- */
typedef int                 BOOL;
typedef unsigned char       BYTE;
typedef unsigned short      WORD;
typedef uint32_t            DWORD;
typedef int32_t             LONG;
typedef uint32_t            ULONG;
typedef unsigned int        UINT;
typedef int                 INT;
typedef short               SHORT;
typedef unsigned short      USHORT;
typedef float               FLOAT;
typedef uint64_t            ULONGLONG;
typedef int64_t             LONGLONG;

typedef char                CHAR;
typedef wchar_t             WCHAR;
typedef char                TCHAR;      /* ANSI build; vtterm.c is char-based */

typedef char               *PCHAR;
typedef unsigned char      *PUCHAR;
typedef char               *LPSTR;
typedef const char         *LPCSTR;
typedef wchar_t            *LPWSTR;
typedef const wchar_t      *LPCWSTR;
typedef char               *LPTSTR;
typedef const char         *LPCTSTR;
typedef void               *LPVOID;
typedef const void         *LPCVOID;
typedef BOOL               *PBOOL;
typedef DWORD              *LPDWORD;
typedef WORD               *LPWORD;
typedef BYTE               *LPBYTE;
typedef int                *LPINT;

typedef intptr_t            INT_PTR;
typedef uintptr_t           UINT_PTR;
typedef intptr_t            LONG_PTR;
typedef uintptr_t           ULONG_PTR;
typedef ULONG_PTR           DWORD_PTR;
typedef UINT_PTR            WPARAM;
typedef LONG_PTR            LPARAM;
typedef LONG_PTR            LRESULT;
typedef LONG                HRESULT;

typedef DWORD               COLORREF;
typedef DWORD               ATOM;
typedef LONG                LSTATUS;

/* ---- opaque handles ----
 * Distinct struct pointers so the compiler still type-checks assignments,
 * matching Win32's STRICT mode. None of these are ever dereferenced here.
 */
#define DECLARE_HANDLE(name) typedef struct name##__ { int unused; } *name
typedef void *HANDLE;
DECLARE_HANDLE(HWND);
DECLARE_HANDLE(HDC);
DECLARE_HANDLE(HFONT);
DECLARE_HANDLE(HBITMAP);
DECLARE_HANDLE(HMENU);
DECLARE_HANDLE(HICON);
DECLARE_HANDLE(HBRUSH);
DECLARE_HANDLE(HPEN);
DECLARE_HANDLE(HRGN);
DECLARE_HANDLE(HKEY);
DECLARE_HANDLE(HINSTANCE);
DECLARE_HANDLE(HGLOBAL);
DECLARE_HANDLE(HLOCAL);
DECLARE_HANDLE(HGDIOBJ);
DECLARE_HANDLE(HDROP);
typedef HINSTANCE HMODULE;
typedef HICON     HCURSOR;

typedef UINT_PTR SOCKET;

/* ---- structs ---- */
typedef struct tagPOINT { LONG x, y; } POINT, *LPPOINT, *PPOINT;
typedef struct tagSIZE  { LONG cx, cy; } SIZE, *LPSIZE, *PSIZE;
typedef struct tagRECT  { LONG left, top, right, bottom; } RECT, *LPRECT, *PRECT;

typedef struct _FILETIME {
	DWORD dwLowDateTime;
	DWORD dwHighDateTime;
} FILETIME, *LPFILETIME, *PFILETIME;

typedef struct _SYSTEMTIME {
	WORD wYear, wMonth, wDayOfWeek, wDay;
	WORD wHour, wMinute, wSecond, wMilliseconds;
} SYSTEMTIME, *LPSYSTEMTIME, *PSYSTEMTIME;

/* LOGFONT — referenced in i18n.h signatures the oracle never calls, but the
 * declarations still have to parse. */
#define LF_FACESIZE 32
typedef struct tagLOGFONTA {
	LONG lfHeight, lfWidth, lfEscapement, lfOrientation, lfWeight;
	BYTE lfItalic, lfUnderline, lfStrikeOut, lfCharSet;
	BYTE lfOutPrecision, lfClipPrecision, lfQuality, lfPitchAndFamily;
	CHAR lfFaceName[LF_FACESIZE];
} LOGFONTA, *PLOGFONTA, *LPLOGFONTA;
typedef struct tagLOGFONTW {
	LONG lfHeight, lfWidth, lfEscapement, lfOrientation, lfWeight;
	BYTE lfItalic, lfUnderline, lfStrikeOut, lfCharSet;
	BYTE lfOutPrecision, lfClipPrecision, lfQuality, lfPitchAndFamily;
	WCHAR lfFaceName[LF_FACESIZE];
} LOGFONTW, *PLOGFONTW, *LPLOGFONTW;
typedef LOGFONTA LOGFONT;
typedef PLOGFONTA PLOGFONT;

/* ---- constants ---- */
#ifndef TRUE
#define TRUE  1
#endif
#ifndef FALSE
#define FALSE 0
#endif
#ifndef NULL
#define NULL ((void *)0)
#endif

#define MAX_PATH        260
#define INVALID_HANDLE_VALUE ((HANDLE)(LONG_PTR)-1)
#define INFINITE        0xFFFFFFFFu
#define CP_ACP          0
#define CP_UTF8         65001

/* ---- macros ---- */
#define LOWORD(l)   ((WORD)(((DWORD_PTR)(l)) & 0xffff))
#define HIWORD(l)   ((WORD)((((DWORD_PTR)(l)) >> 16) & 0xffff))
#define LOBYTE(w)   ((BYTE)(((DWORD_PTR)(w)) & 0xff))
#define HIBYTE(w)   ((BYTE)((((DWORD_PTR)(w)) >> 8) & 0xff))
#define MAKEWORD(a,b) ((WORD)(((BYTE)(a)) | ((WORD)((BYTE)(b))) << 8))
#define MAKELONG(a,b) ((LONG)(((WORD)(a)) | ((DWORD)((WORD)(b))) << 16))
#define RGB(r,g,b)  ((COLORREF)(((BYTE)(r)) | ((WORD)((BYTE)(g))<<8) | (((DWORD)(BYTE)(b))<<16)))
#define GetRValue(c) ((BYTE)(c))
#define GetGValue(c) ((BYTE)(((WORD)(c)) >> 8))
#define GetBValue(c) ((BYTE)((c) >> 16))

#ifndef _countof
#define _countof(a) (sizeof(a)/sizeof((a)[0]))
#endif

#ifndef min
#define min(a,b) (((a) < (b)) ? (a) : (b))
#endif
#ifndef max
#define max(a,b) (((a) > (b)) ? (a) : (b))
#endif

/* Win32 entry points reached only from code paths the oracle never drives
 * (printing, message loop, caret). Defined in winshim.c as no-ops. */
BOOL MessageBeep(UINT uType);
void PostQuitMessage(int nExitCode);
BOOL UpdateWindow(HWND hWnd);
int  StartPage(HDC hdc);
int  EndPage(HDC hdc);
BOOL IsDBCSLeadByte(BYTE TestChar);

/* MSVC-isms used in the Tera Term sources */
#define _stricmp   strcasecmp
#define _strnicmp  strncasecmp
#define _wcsicmp   wcscasecmp
#define _wcsnicmp  wcsncasecmp

/* ShellExecuteW + SW_* — reached only by buffer.c's invokeBrowserWithUrl(),
 * which opens a clicked URL. Irrelevant to the grid model; stubbed so the
 * translation unit links. The oracle records the request instead. */
#define SW_HIDE            0
#define SW_SHOWNORMAL      1
#define SW_NORMAL          1
#define SW_SHOWMINIMIZED   2
#define SW_SHOWMAXIMIZED   3
#define SW_MAXIMIZE        3
#define SW_SHOW            5
#define SW_MINIMIZE        6
#define SW_RESTORE         9

HINSTANCE ShellExecuteW(HWND hwnd, const wchar_t *lpOperation,
                        const wchar_t *lpFile, const wchar_t *lpParameters,
                        const wchar_t *lpDirectory, int nShowCmd);

/* ---- MessageBox, for ttpfile's error paths ----
 *
 * The file-transfer protocols report failures ("Cannot create file") through
 * MessageBox. Headless those are diagnostics, not dialogs, so the shim writes
 * them to stderr. Without a declaration these compile via C89 implicit
 * declaration and only fail at link, which is a confusing way to find out.
 */
#define MB_OK                0x0000
#define MB_OKCANCEL          0x0001
#define MB_YESNO             0x0004
#define MB_ICONERROR         0x0010
#define MB_ICONQUESTION      0x0020
#define MB_ICONEXCLAMATION   0x0030
#define MB_ICONINFORMATION   0x0040
#define MB_TASKMODAL         0x2000

#define IDOK      1
#define IDCANCEL  2
#define IDYES     6
#define IDNO      7

int MessageBoxA(HWND hWnd, const char *text, const char *caption, UINT type);
#define MessageBox MessageBoxA

/* A consumer that has somewhere better to put the text than stderr says so
 * here. `tt-xfer` does: "Cannot create file" is the only account the user gets
 * of why a transfer failed, and a library that printed it to the terminal's
 * own stderr would be writing it where nobody is looking. Passing NULL
 * restores the default. Not thread-safe by design — set it at startup. */
void winshim_set_message_sink(void (*sink)(void *ctx, const char *caption,
                                           const char *text),
                              void *ctx);

/* ---- the three functions vtterm.c/buffer.c actually call ---- */
void  Sleep(DWORD dwMilliseconds);
DWORD GetTickCount(void);
int   WideCharToMultiByte(UINT CodePage, DWORD dwFlags,
                          const wchar_t *lpWideCharStr, int cchWideChar,
                          char *lpMultiByteStr, int cbMultiByte,
                          const char *lpDefaultChar, BOOL *lpUsedDefaultChar);
int   MultiByteToWideChar(UINT CodePage, DWORD dwFlags,
                          const char *lpMultiByteStr, int cbMultiByte,
                          wchar_t *lpWideCharStr, int cchWideChar);


/* ---- the keyboard's Win32 surface ----
 *
 * keyboard.c is compiled whole so the key table is ground truth rather than a
 * transcription; see oracle/README.md. Almost all of what it needs from Win32
 * is *constants*, which have fixed documented values and cannot be got subtly
 * wrong. The message-pump calls below are genuine stubs, and they are only
 * reached on paths the oracle does not drive: PeekMessage discards a pending
 * WM_CHAR after a key has been handled another way, and PostMessage delivers
 * an accelerator to the window. GetKeyStr, which is the part that matters,
 * touches neither.
 */
#define WM_USER      0x0400
#define WM_COMMAND   0x0111
#define WM_CHAR      0x0102
#define WM_SYSCHAR   0x0106
#define WM_TIMER     0x0113
#define PM_NOREMOVE  0x0000
#define PM_REMOVE    0x0001

typedef struct tagMSG {
	HWND   hwnd;
	UINT   message;
	WPARAM wParam;
	LPARAM lParam;
	DWORD  time;
} MSG, *LPMSG, *PMSG;

typedef BYTE *PBYTE;

BOOL PeekMessageA(LPMSG lpMsg, HWND hWnd, UINT wMsgFilterMin, UINT wMsgFilterMax, UINT wRemoveMsg);
#define PeekMessage PeekMessageA
BOOL PostMessageA(HWND hWnd, UINT Msg, WPARAM wParam, LPARAM lParam);
#define PostMessage PostMessageA
BOOL GetMessageA(LPMSG lpMsg, HWND hWnd, UINT wMsgFilterMin, UINT wMsgFilterMax);
#define GetMessage GetMessageA

UINT  MapVirtualKeyA(UINT uCode, UINT uMapType);
#define MapVirtualKey MapVirtualKeyA
DWORD OemKeyScan(WORD wOemChar);
SHORT VkKeyScanA(char ch);
#define VkKeyScan VkKeyScanA
SHORT GetKeyState(int nVirtKey);
BOOL  GetKeyboardState(PBYTE lpKeyState);

/* Virtual-key codes. Fixed by the Win32 API since 1993. */
#define VK_BACK      0x08
#define VK_TAB       0x09
#define VK_RETURN    0x0D
#define VK_SHIFT     0x10
#define VK_CONTROL   0x11
#define VK_MENU      0x12
#define VK_PAUSE     0x13
#define VK_ESCAPE    0x1B
#define VK_SPACE     0x20
#define VK_PRIOR     0x21
#define VK_NEXT      0x22
#define VK_END       0x23
#define VK_HOME      0x24
#define VK_LEFT      0x25
#define VK_UP        0x26
#define VK_RIGHT     0x27
#define VK_DOWN      0x28
#define VK_INSERT    0x2D
#define VK_DELETE    0x2E
#define VK_NUMLOCK   0x90
#define VK_SCROLL    0x91
#define VK_LSHIFT    0xA0
#define VK_RSHIFT    0xA1
#define VK_LCONTROL  0xA2
#define VK_RCONTROL  0xA3
#define VK_LMENU     0xA4
#define VK_RMENU     0xA5
#define VK_OEM_2     0xBF
#define VK_OEM_102   0xE2
#define VK_F1        0x70
#define VK_F2        0x71
#define VK_F3        0x72
#define VK_F4        0x73
#define VK_F5        0x74
#define VK_F6        0x75
#define VK_F7        0x76
#define VK_F8        0x77
#define VK_F9        0x78
#define VK_F10       0x79
#define VK_F11       0x7A
#define VK_F12       0x7B
#define VK_F13       0x7C
#define VK_F14       0x7D
#define VK_F15       0x7E
#define VK_F16       0x7F
#define VK_F17       0x80
#define VK_F18       0x81
#define VK_F19       0x82
#define VK_F20       0x83

#ifdef __cplusplus
}
#endif
