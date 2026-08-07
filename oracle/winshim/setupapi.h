/*
 * qtterm oracle: SetupAPI is pulled in transitively by win32helper.h, which
 * declares COM-port enumeration helpers. The oracle never enumerates ports —
 * these types exist only so those declarations parse.
 */
#pragma once

#include <windows.h>

typedef void *HDEVINFO;

typedef struct _SP_DEVINFO_DATA {
	DWORD     cbSize;
	BYTE      ClassGuid[16];
	DWORD     DevInst;
	ULONG_PTR Reserved;
} SP_DEVINFO_DATA, *PSP_DEVINFO_DATA;

typedef struct _DEVPROPKEY {
	BYTE  fmtid[16];
	ULONG pid;
} DEVPROPKEY;

typedef ULONG DEVPROPTYPE, *PDEVPROPTYPE;
