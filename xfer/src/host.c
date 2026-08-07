/*
 * The host side of the protocol contract: FileVarProto services, the InfoOp
 * progress vtable, and the handful of Tera Term / Win32 symbols the protocol
 * sources reference but that live outside ttpfile.
 *
 * There are only six of those. That number is the real finding of this spike:
 * the protocols are almost free-standing.
 */
#include "xfer.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "ttlib.h"

int xfer_timeout_secs = 0;
int xfer_timer_ms = 0;
int xfer_verbose = 0;

/* ---------------------------------------------------------------- services */

typedef struct {
	char **send_paths;
	int send_count;
	int send_index;
	char *recv_dir;
} HostData;

static HostData host;

static char *svc_get_next_fname(TFileVarProto *fv)
{
	(void)fv;
	if (host.send_index >= host.send_count)
		return NULL;
	/* Ownership passes to the protocol, which frees it in Destroy — same as
	 * filesys_proto.cpp's ToU8W() result. */
	return strdup(host.send_paths[host.send_index++]);
}

static char *svc_get_receive_path(TFileVarProto *fv)
{
	(void)fv;
	return strdup(host.recv_dir ? host.recv_dir : "./");
}

static void svc_set_timeout(TFileVarProto *fv, int t)
{
	(void)fv;
	xfer_timeout_secs = t;
}

static void svc_set_dialog_caption(TFileVarProto *fv, const char *key,
                                   const wchar_t *default_caption)
{
	(void)fv; (void)key; (void)default_caption;
}

/* ------------------------------------------------------------------ InfoOp */

static void info_init_progress(TFileVarProto *fv, int *CurProgStat)
{
	(void)fv;
	if (CurProgStat)
		*CurProgStat = 0;
}

static void info_set_time(TFileVarProto *fv, DWORD elapsed, int bytes)
{
	(void)fv; (void)elapsed; (void)bytes;
}

static void info_set_packet_num(TFileVarProto *fv, LONG num)
{
	(void)fv;
	if (xfer_verbose)
		fprintf(stderr, "  packet %ld\n", (long)num);
}

static void info_set_byte_count(TFileVarProto *fv, LONG num)
{
	(void)fv;
	if (xfer_verbose)
		fprintf(stderr, "  bytes %ld\n", (long)num);
}

static void info_set_percent(TFileVarProto *fv, LONG a, LONG b, int *p)
{
	(void)fv; (void)a; (void)b; (void)p;
}

static void info_set_proto_text(TFileVarProto *fv, const char *text)
{
	(void)fv;
	if (xfer_verbose && text && *text)
		fprintf(stderr, "  [proto] %s\n", text);
}

static void info_set_proto_filename(TFileVarProto *fv, const char *text)
{
	(void)fv;
	if (text && *text)
		fprintf(stderr, "  file: %s\n", text);
}

static const TInfoOp info_op = {
	info_init_progress,
	info_set_time,
	info_set_packet_num,
	info_set_byte_count,
	info_set_percent,
	info_set_proto_text,
	info_set_proto_filename,
};

/* ----------------------------------------------------------------- factory */

TFileVarProto *filevar_create(TComm *comm, TFileIO *file)
{
	TFileVarProto *fv = calloc(1, sizeof(*fv));
	if (fv == NULL)
		return NULL;

	fv->GetNextFname = svc_get_next_fname;
	fv->GetRecievePath = svc_get_receive_path;   /* sic — upstream spelling */
	fv->FTSetTimeOut = svc_set_timeout;
	fv->SetDialogCation = svc_set_dialog_caption;
	fv->InfoOp = &info_op;
	fv->Comm = comm;
	fv->file_fv = file;
	fv->OverWrite = TRUE;
	fv->NoMsg = FALSE;
	return fv;
}

void filevar_destroy(TFileVarProto *fv)
{
	free(fv);
}

void filevar_set_send_files(TFileVarProto *fv, char *const *paths, int count)
{
	(void)fv;
	host.send_paths = (char **)paths;
	host.send_count = count;
	host.send_index = 0;
}

void filevar_set_receive_dir(TFileVarProto *fv, const char *dir)
{
	(void)fv;
	free(host.recv_dir);
	host.recv_dir = malloc(strlen(dir) + 2);
	if (host.recv_dir == NULL)
		return;
	strcpy(host.recv_dir, dir);
	/* The protocols concatenate path + filename with no separator, so the
	 * trailing one is part of the contract (see RecievePath in
	 * filesys_proto.h: "終端にパスセパレータが付加されている"). */
	if (host.recv_dir[0] && host.recv_dir[strlen(host.recv_dir) - 1] != '/')
		strcat(host.recv_dir, "/");
}

/* -------------------------------------------- symbols from outside ttpfile */

/*
 * Tera Term drives protocol timeouts through Win32 window timers. Headless,
 * the driver loop owns timing, so these just record the request. Only zmodem
 * uses them (zmodem.c:1586).
 */
UINT_PTR SetTimer(HWND hWnd, UINT_PTR nIDEvent, UINT uElapse, void *lpTimerFunc)
{
	(void)hWnd; (void)nIDEvent; (void)lpTimerFunc;
	xfer_timer_ms = (int)uElapse;
	return nIDEvent;
}

BOOL KillTimer(HWND hWnd, UINT_PTR uIDEvent)
{
	(void)hWnd; (void)uIDEvent;
	xfer_timer_ms = 0;
	return TRUE;
}

/* filesys_proto.cpp's teardown. The driver loop owns the lifecycle here. */
void ProtoEnd(void)
{
}

int TTMessageBoxW(HWND hWnd, const TTMessageBoxInfoW *info,
                  const wchar_t *UILanguageFile, ...)
{
	(void)hWnd; (void)UILanguageFile;
	if (info != NULL && info->message_default != NULL)
		fprintf(stderr, "[TTMessageBox] %ls\n", info->message_default);
	return IDOK;
}
