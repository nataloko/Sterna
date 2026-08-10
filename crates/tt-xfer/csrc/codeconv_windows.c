/* The two codeconv entry points used by protolog.cpp on a real Windows build. */
#include <windows.h>

#include <stdlib.h>

static wchar_t *to_wide(const char *text, UINT code_page)
{
	DWORD flags = code_page == CP_UTF8 ? MB_ERR_INVALID_CHARS : 0;
	int n;
	wchar_t *wide;

	if (text == NULL)
		return NULL;
	n = MultiByteToWideChar(code_page, flags, text, -1, NULL, 0);
	if (n <= 0)
		return NULL;
	wide = malloc((size_t)n * sizeof(*wide));
	if (wide == NULL)
		return NULL;
	if (MultiByteToWideChar(code_page, flags, text, -1, wide, n) == 0) {
		free(wide);
		return NULL;
	}
	return wide;
}

wchar_t *ToWcharU8(const char *text)
{
	return to_wide(text, CP_UTF8);
}

wchar_t *ToWcharA(const char *text)
{
	return to_wide(text, CP_ACP);
}
