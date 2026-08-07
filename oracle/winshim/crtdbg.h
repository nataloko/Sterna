/* qtterm oracle: MSVC debug-heap header shim (no-op on POSIX). */
#pragma once
#include <assert.h>
#define _ASSERT(e)      assert(e)
#define _ASSERTE(e)     assert(e)
#define _CrtCheckMemory() 1
