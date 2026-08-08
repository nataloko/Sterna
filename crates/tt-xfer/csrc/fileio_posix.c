/*
 * TFileIO for POSIX — the replacement for ttpfile/filesys_win32.cpp.
 *
 * Semantics are mirrored from the Win32 original rather than invented, because
 * the protocols depend on them on the wire: GetSendFilename must yield a
 * BASENAME (send a full path and the peer writes a file with slashes in the
 * name), and the space/upper flags exist because XMODEM peers of a certain
 * vintage cannot cope otherwise.
 */
#include "tt_xfer.h"

#include "tttypes.h"
#include "filesys_io.h"

#include <ctype.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <utime.h>

typedef struct {
	FILE *fp;
} FileIOPosix;

static BOOL fio_open_read(TFileIO *fv, const char *nameU8)
{
	FileIOPosix *d = fv->data;
	if (d->fp)
		fclose(d->fp);
	d->fp = fopen(nameU8, "rb");
	return d->fp != NULL;
}

static BOOL fio_open_write(TFileIO *fv, const char *nameU8)
{
	FileIOPosix *d = fv->data;
	if (d->fp)
		fclose(d->fp);
	d->fp = fopen(nameU8, "wb");
	return d->fp != NULL;
}

static size_t fio_read(TFileIO *fv, void *buf, size_t bytes)
{
	FileIOPosix *d = fv->data;
	return d->fp ? fread(buf, 1, bytes, d->fp) : 0;
}

static size_t fio_write(TFileIO *fv, const void *buf, size_t bytes)
{
	FileIOPosix *d = fv->data;
	return d->fp ? fwrite(buf, 1, bytes, d->fp) : 0;
}

static void fio_close(TFileIO *fv)
{
	FileIOPosix *d = fv->data;
	if (d->fp) {
		fclose(d->fp);
		d->fp = NULL;
	}
}

static int fio_seek(TFileIO *fv, size_t offset)
{
	FileIOPosix *d = fv->data;
	return d->fp ? fseek(d->fp, (long)offset, SEEK_SET) : -1;
}

static void fio_destroy(TFileIO *fv)
{
	if (fv == NULL)
		return;
	fio_close(fv);
	free(fv->data);
	free(fv);
}

static size_t fio_getfsize(TFileIO *fv, const char *nameU8)
{
	struct stat st;
	(void)fv;
	return stat(nameU8, &st) == 0 ? (size_t)st.st_size : 0;
}

static int fio_utime(TFileIO *fv, const char *nameU8, struct _utimbuf *const t)
{
	struct utimbuf u;
	(void)fv;
	u.actime = t->actime;
	u.modtime = t->modtime;
	return utime(nameU8, &u);
}

static BOOL fio_setfmtime(TFileIO *fv, const char *nameU8, DWORD mtime)
{
	struct utimbuf u;
	(void)fv;
	u.actime = (time_t)mtime;
	u.modtime = (time_t)mtime;
	return utime(nameU8, &u) == 0;
}

static int fio_stat(TFileIO *fv, const char *nameU8, struct _stati64 *out)
{
	(void)fv;
	return stat(nameU8, out);
}

static long fio_getfmtime(TFileIO *fv, const char *nameU8)
{
	struct stat st;
	(void)fv;
	return stat(nameU8, &st) == 0 ? (long)st.st_mtime : 0;
}

static const char *basename_of(const char *path)
{
	const char *slash = strrchr(path, '/');
	return slash ? slash + 1 : path;
}

static char *fio_get_send_filename(TFileIO *fv, const char *fullnameU8,
                                   BOOL utf8, BOOL space, BOOL upper)
{
	char *name;
	(void)fv;
	(void)utf8;   /* We are UTF-8 throughout; there is no ANSI codepage here. */

	name = strdup(basename_of(fullnameU8));
	if (name == NULL)
		return NULL;
	if (space) {
		for (char *p = name; *p; p++)
			if (*p == ' ')
				*p = '_';
	}
	if (upper) {
		for (char *p = name; *p; p++)
			*p = (char)toupper((unsigned char)*p);
	}
	return name;
}

static char *fio_get_receive_filename(TFileIO *fv, const char *filename,
                                      BOOL utf8, const char *path, BOOL unique)
{
	char *out;
	size_t need;
	const char *base;
	(void)fv;
	(void)utf8;

	/* Never honour a path from the wire: a peer that sends "../../x" or an
	 * absolute path must not escape the receive directory. Win32 leans on
	 * GetFileNamePos for this; on POSIX taking the basename is the whole job. */
	base = basename_of(filename);
	if (base[0] == '\0' || strcmp(base, ".") == 0 || strcmp(base, "..") == 0)
		base = "noname";

	need = strlen(path ? path : "") + strlen(base) + 32;
	out = malloc(need);
	if (out == NULL)
		return NULL;
	snprintf(out, need, "%s%s", path ? path : "", base);

	if (unique) {
		struct stat st;
		int n = 1;
		while (stat(out, &st) == 0 && n < 1000)
			snprintf(out, need, "%s%s.%d", path ? path : "", base, n++);
	}
	return out;
}

TFileIO *tt_xfer_fileio_create(void)
{
	TFileIO *fv = calloc(1, sizeof(*fv));
	FileIOPosix *d = calloc(1, sizeof(*d));

	if (fv == NULL || d == NULL) {
		free(fv);
		free(d);
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
	fv->data = d;
	return fv;
}
