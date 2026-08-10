/*
 * Platform seam for the vendored Tera Term protocols.
 *
 * The sources include <windows.h> because that is where their scalar types
 * live.  On POSIX our minimal winshim supplies it; on Windows this is the real
 * SDK header.  The three window-shaped services below are different: the
 * library has no HWND or message loop, so its host implements them per
 * transfer on both platforms.  Remapping the names here also stops a Windows
 * build from accidentally opening a native MessageBox behind Qt's back.
 */
#pragma once

#include <windows.h>

#ifndef _WIN32
#include "msvc_compat.h"
#endif

/* MSVC's <windows.h> happens to make these visible to upstream source files;
 * the POSIX shim does the same.  Make that dependency explicit here. */
#include <stdlib.h>
#include <string.h>

#ifdef MessageBox
#undef MessageBox
#endif
#ifdef SetTimer
#undef SetTimer
#endif
#ifdef KillTimer
#undef KillTimer
#endif

#define MessageBox  tt_xfer_MessageBoxA
#define SetTimer    tt_xfer_SetTimer
#define KillTimer   tt_xfer_KillTimer

#ifdef __cplusplus
extern "C" {
#endif

int tt_xfer_MessageBoxA(HWND hWnd, const char *text, const char *caption,
                        UINT type);
UINT_PTR tt_xfer_SetTimer(HWND hWnd, UINT_PTR id, UINT elapsed,
                         void *callback);
BOOL tt_xfer_KillTimer(HWND hWnd, UINT_PTR id);

#ifdef __cplusplus
}
#endif
