/*
 * TFileIO for Windows.
 *
 * The protocol surface is UTF-8 even on Windows.  Keep it that way until the
 * filesystem boundary, then use the wide CRT calls; opening the bytes through
 * fopen would reinterpret them in the process ANSI code page and make an
 * otherwise successful transfer fail solely because its path is non-ASCII.
 */
#include "tt_xfer.h"

#include "tttypes.h"
#include "filesys_io.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
	FILE *fp;
} FileIOWindows;

static wchar_t *wide_path(const char *path)
{
	int n;
	wchar_t *wide;

	if (path == NULL)
		return NULL;
	n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
	if (n <= 0)
		return NULL;
	wide = malloc((size_t)n * sizeof(*wide));
	if (wide == NULL)
		return NULL;
	if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, n) == 0) {
		free(wide);
		return NULL;
	}
	return wide;
}

static BOOL open_file(TFileIO *fv, const char *name, const wchar_t *mode)
{
	FileIOWindows *data = fv->data;
	wchar_t *wide = wide_path(name);
	FILE *fp = NULL;

	if (data->fp != NULL)
		fclose(data->fp);
	data->fp = NULL;
	if (wide != NULL)
		_wfopen_s(&fp, wide, mode);
	free(wide);
	data->fp = fp;
	return fp != NULL;
}

static BOOL fio_open_read(TFileIO *fv, const char *name)
{
	return open_file(fv, name, L"rb");
}

static BOOL fio_open_write(TFileIO *fv, const char *name)
{
	return open_file(fv, name, L"wb");
}

static size_t fio_read(TFileIO *fv, void *buf, size_t bytes)
{
	FileIOWindows *data = fv->data;
	return data->fp != NULL ? fread(buf, 1, bytes, data->fp) : 0;
}

static size_t fio_write(TFileIO *fv, const void *buf, size_t bytes)
{
	FileIOWindows *data = fv->data;
	return data->fp != NULL ? fwrite(buf, 1, bytes, data->fp) : 0;
}

static void fio_close(TFileIO *fv)
{
	FileIOWindows *data = fv->data;
	if (data->fp != NULL) {
		fclose(data->fp);
		data->fp = NULL;
	}
}

static int fio_seek(TFileIO *fv, size_t offset)
{
	FileIOWindows *data = fv->data;
	return data->fp != NULL ? _fseeki64(data->fp, (__int64)offset, SEEK_SET) : -1;
}

static void fio_destroy(TFileIO *fv)
{
	if (fv == NULL)
		return;
	fio_close(fv);
	free(fv->data);
	free(fv);
}

static int stat_path(const char *name, struct _stati64 *out)
{
	wchar_t *wide = wide_path(name);
	int result = wide != NULL ? _wstati64(wide, out) : -1;
	free(wide);
	return result;
}

static size_t fio_getfsize(TFileIO *fv, const char *name)
{
	struct _stati64 st;
	(void)fv;
	return stat_path(name, &st) == 0 ? (size_t)st.st_size : 0;
}

static int fio_utime(TFileIO *fv, const char *name, struct _utimbuf *const when)
{
	wchar_t *wide = wide_path(name);
	int result;
	(void)fv;
	result = wide != NULL ? _wutime(wide, when) : -1;
	free(wide);
	return result;
}

static BOOL fio_setfmtime(TFileIO *fv, const char *name, DWORD mtime)
{
	struct _utimbuf when;
	when.actime = (time_t)mtime;
	when.modtime = (time_t)mtime;
	return fio_utime(fv, name, &when) == 0;
}

static int fio_stat(TFileIO *fv, const char *name, struct _stati64 *out)
{
	(void)fv;
	return stat_path(name, out);
}

static long fio_getfmtime(TFileIO *fv, const char *name)
{
	struct _stati64 st;
	(void)fv;
	return stat_path(name, &st) == 0 ? (long)st.st_mtime : 0;
}

static const char *basename_of(const char *path)
{
	const char *slash = strrchr(path, '/');
	const char *backslash = strrchr(path, '\\');
	const char *last = slash;
	if (last == NULL || (backslash != NULL && backslash > last))
		last = backslash;
	return last != NULL ? last + 1 : path;
}

static char *copy_string(const char *text)
{
	size_t n = strlen(text) + 1;
	char *copy = malloc(n);
	if (copy != NULL)
		memcpy(copy, text, n);
	return copy;
}

static char *fio_get_send_filename(TFileIO *fv, const char *fullname,
                                   BOOL utf8, BOOL space, BOOL upper)
{
	char *name;
	char *p;
	(void)fv;
	(void)utf8;

	name = copy_string(basename_of(fullname));
	if (name == NULL)
		return NULL;
	for (p = name; *p != '\0'; ++p) {
		if (space && *p == ' ')
			*p = '_';
		if (upper)
			*p = (char)toupper((unsigned char)*p);
	}
	return name;
}

static char *fio_get_receive_filename(TFileIO *fv, const char *filename,
                                      BOOL utf8, const char *path, BOOL unique)
{
	const char *base = basename_of(filename);
	const char *dir = path != NULL ? path : "";
	size_t need;
	char *out;
	int n = 1;
	struct _stati64 st;
	(void)fv;
	(void)utf8;

	/* Never honour a directory received from the wire. */
	if (*base == '\0' || strcmp(base, ".") == 0 || strcmp(base, "..") == 0)
		base = "noname";
	need = strlen(dir) + strlen(base) + 32;
	out = malloc(need);
	if (out == NULL)
		return NULL;
	snprintf(out, need, "%s%s", dir, base);
	while (unique && stat_path(out, &st) == 0 && n < 1000)
		snprintf(out, need, "%s%s.%d", dir, base, n++);
	return out;
}

TFileIO *tt_xfer_fileio_create(void)
{
	TFileIO *fv = calloc(1, sizeof(*fv));
	FileIOWindows *data = calloc(1, sizeof(*data));

	if (fv == NULL || data == NULL) {
		free(fv);
		free(data);
		return NULL;
	}
	fv->OpenRead = fio_open_read;
	fv->OpenWrite = fio_open_write;
	fv->ReadFile = fio_read;
	fv->WriteFile = fio_write;
	fv->Close = fio_close;
	fv->Seek = fio_seek;
	fv->FileSysDestroy = fio_destroy;
	fv->GetFSize = fio_getfsize;
	fv->utime = fio_utime;
	fv->SetFMtime = fio_setfmtime;
	fv->stat = fio_stat;
	fv->GetFMtime = fio_getfmtime;
	fv->GetSendFilename = fio_get_send_filename;
	fv->GetReceiveFilename = fio_get_receive_filename;
	fv->data = data;
	return fv;
}
