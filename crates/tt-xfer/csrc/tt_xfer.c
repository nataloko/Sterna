/*
 * The host half of Tera Term's file-transfer contract.
 *
 * `filesys_proto.cpp` is upstream's version of this file: the same three
 * vtables, plus a modal dialog, a Win32 message pump and a file-scope
 * `FileVar`. What is here is the vtables and nothing else, per instance, with
 * the loop that drives them in Rust.
 *
 * The comm side is written against `ttpcmn/ttcmn.c` rather than invented,
 * because the protocols read `TComVar` directly in three places and would
 * notice: raw.c drains `cv->InBuff` itself (`raw.c:152`), bplus.c waits on
 * `cv->OutBuffCount` reaching zero (`bplus.c:885`), and every protocol tests
 * `cv->Ready` before deciding it cannot finish.
 */
#include "tt_xfer.h"

#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "tttypes.h"
#include "filesys_proto.h"
#include "filesys_io.h"
#include "xmodem.h"
#include "ymodem.h"
#include "zmodem.h"
#include "kermit.h"
#include "bplus.h"
#include "quickvan.h"
#include "raw.h"

TProto *ZCreate(TFileVarProto *fv);   /* zmodem.h omits this declaration */

TFileIO *tt_xfer_fileio_create(void);

struct TtXfer {
	TTTSet ts;
	TComVar cv;
	TComm comm;
	TFileIO *file;
	TFileVarProto fv;
	TProto *proto;

	/* GetNextFname walks this; ownership of each string is ours. */
	char **send_paths;
	int send_count;
	int send_index;
	char *recv_dir;

	/* FTSetTimeOut's deadline and the protocol's own 500 ms cancel timer,
	 * both CLOCK_MONOTONIC seconds; 0 means unarmed. */
	double deadline;
	double timer_at;

	TtXferProgress prog;
	double start;
	char *proto_name;
	char *file_name;
	char *message;
	wchar_t *log_dir_w;

	unsigned state;
};

/*
 * MessageBox, TTMessageBoxW, ProtoEnd, SetTimer and KillTimer are plain
 * globals in Tera Term and stay globals here, so the instance they belong to
 * is threaded through a thread-local set around every call into the protocol.
 * Thread-local rather than global because two windows may each be
 * transferring, and each pumps on its own thread.
 */
static _Thread_local TtXfer *current;

static double now_sec(void)
{
	struct timespec t;
	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec + t.tv_nsec / 1e9;
}

static void set_str(char **slot, const char *value)
{
	free(*slot);
	*slot = (value != NULL) ? strdup(value) : NULL;
}

/* ------------------------------------------------------------------- TComm */

/* `CommReadRawByte` (`ttcmn.c:498`), NOT `CommRead1Byte`.
 *
 * The difference is the telnet decoder. Upstream runs one buffer for the
 * terminal and the transfer both, so `CommRead1Byte` unescapes `IAC IAC` and
 * drops the `NUL` after a `CR` on the way past. Here `tt-conn`'s telnet
 * transport has already done that before the bytes ever arrive, and doing it
 * twice would eat one `0xFF` of every escaped pair — a corruption that only
 * shows up on binary payloads over telnet, which is to say on every zmodem
 * transfer to a terminal server.
 */
static int comm_read1(TComm *comm, BYTE *b)
{
	TtXfer *x = comm->private_data;

	if (!x->cv.Ready)
		return 0;

	if (x->cv.InBuffCount > 0) {
		*b = x->cv.InBuff[x->cv.InPtr];
		x->cv.InPtr++;
		x->cv.InBuffCount--;
		if (x->cv.InBuffCount == 0)
			x->cv.InPtr = 0;
		return 1;
	}
	x->cv.InPtr = 0;
	return 0;
}

/* `CommRawOut` (`ttcmn.c:610`) — again the raw one, for the same reason:
 * `CommBinaryOut` is `CommRawOut` plus the telnet *escaping*, and the
 * transport applies that itself. Short writes are real and the protocols
 * handle them; returning the full length when the buffer is full would lose
 * a packet in silence. */
static int comm_binary_out(TComm *comm, const CHAR *buf, size_t len)
{
	TtXfer *x = comm->private_data;
	int room, n;

	if (!x->cv.Ready)
		return (int)len;   /* upstream's answer: pretend, and drop it */

	room = OutBuffSize - x->cv.OutBuffCount;
	n = ((int)len > room) ? room : (int)len;
	if (n <= 0)
		return 0;

	if (x->cv.OutPtr > 0) {
		memmove(&x->cv.OutBuff[0], &x->cv.OutBuff[x->cv.OutPtr],
		        (size_t)x->cv.OutBuffCount);
		x->cv.OutPtr = 0;
	}
	memcpy(&x->cv.OutBuff[x->cv.OutBuffCount], buf, (size_t)n);
	x->cv.OutBuffCount += n;
	return n;
}

/*
 * `CommInsert1Byte` (`ttcmn.c:532`) — which puts the byte at the front of the
 * **receive** buffer, not the send buffer.
 *
 * `filesys_proto.h:61` comments it as "1byte送信" and it is not; the
 * implementation is authoritative and the header comment is wrong. It exists
 * for auto-start: when the terminal has already swallowed `ZPAD ZDLE B 0 0`
 * out of the stream, `ZInit` pushes those five bytes back so the protocol can
 * read its own trigger. Send them instead and the trigger goes to the peer,
 * which is a zmodem header arriving from the wrong direction. `xfer/`'s
 * spike had it backwards and never noticed, because nothing there ran an
 * auto-start mode.
 */
static void comm_insert1(TComm *comm, BYTE b)
{
	TtXfer *x = comm->private_data;

	/* Upstream overruns by one byte here when the buffer is exactly full: it
	 * memmoves up by one and increments the count unconditionally. Reached
	 * only from ZInit and BPInit, before any data has been pushed, so it
	 * cannot fire in practice — but this buffer is filled from the network,
	 * so the guard stays. See vendor/ttpfile/README.md. */
	if (x->cv.InBuffCount >= InBuffSize)
		return;

	if (x->cv.InPtr == 0)
		memmove(&x->cv.InBuff[1], &x->cv.InBuff[0], (size_t)x->cv.InBuffCount);
	else
		x->cv.InPtr--;
	x->cv.InBuff[x->cv.InPtr] = b;
	x->cv.InBuffCount++;
}

static void comm_flush_recv(TComm *comm)
{
	TtXfer *x = comm->private_data;
	x->cv.InBuffCount = 0;
	x->cv.InPtr = 0;
}

static const CommOp comm_op = {
	comm_binary_out,
	comm_read1,
	comm_insert1,
	comm_flush_recv,
};

/* ---------------------------------------------------------------- services */

static char *svc_get_next_fname(TFileVarProto *fv)
{
	TtXfer *x = fv->Comm->private_data;

	if (x->send_index >= x->send_count)
		return NULL;
	/* Ownership passes to the protocol, which frees it — the same contract
	 * as filesys_proto.cpp's ToU8W() result. */
	return strdup(x->send_paths[x->send_index++]);
}

static char *svc_get_receive_path(TFileVarProto *fv)
{
	TtXfer *x = fv->Comm->private_data;
	return strdup(x->recv_dir != NULL ? x->recv_dir : "./");
}

static void svc_set_timeout(TFileVarProto *fv, int t)
{
	TtXfer *x = fv->Comm->private_data;
	/* Re-arm unconditionally. The protocols call this with an unchanged
	 * value on every packet precisely to reset the clock. */
	x->deadline = (t > 0) ? now_sec() + t : 0;
}

static void svc_set_dialog_caption(TFileVarProto *fv, const char *key,
                                   const wchar_t *default_caption)
{
	(void)fv; (void)key; (void)default_caption;
}

/* ------------------------------------------------------------------ InfoOp */

static void info_init_progress(TFileVarProto *fv, int *CurProgStat)
{
	TtXfer *x = fv->Comm->private_data;
	if (CurProgStat != NULL)
		*CurProgStat = 0;
	x->prog.percent = 0;
	x->start = now_sec();
}

/* `elapsed` is the protocol's *start* tick, not a duration — upstream passes
 * `zv->StartTime` and `dlglib.c:162` subtracts it from GetTickCount(). */
static void info_set_time(TFileVarProto *fv, DWORD elapsed, int bytes)
{
	TtXfer *x = fv->Comm->private_data;
	(void)bytes;
	x->prog.elapsed_ms = (uint32_t)(GetTickCount() - elapsed);
}

static void info_set_packet_num(TFileVarProto *fv, LONG num)
{
	TtXfer *x = fv->Comm->private_data;
	x->prog.packets = num;
}

static void info_set_byte_count(TFileVarProto *fv, LONG num)
{
	TtXfer *x = fv->Comm->private_data;
	x->prog.bytes = num;
}

/* `dlglib.c:133`. `*p` is a high-water mark the protocol owns: it only ever
 * rises, and a protocol that does not know the size sets it negative to say
 * "no bar". Both halves are upstream's and both are visible to the user. */
static void info_set_percent(TFileVarProto *fv, LONG a, LONG b, int *p)
{
	TtXfer *x = fv->Comm->private_data;
	double num = (b == 0) ? 100.0 : 100.0 * (double)a / (double)b;

	x->prog.done = a;
	x->prog.total = b;
	if (p != NULL && *p >= 0 && (double)*p < num) {
		*p = (int)num;
		x->prog.percent = *p;
	} else if (p != NULL && *p < 0) {
		x->prog.percent = -1;
	}
}

static void info_set_proto_text(TFileVarProto *fv, const char *text)
{
	set_str(&((TtXfer *)fv->Comm->private_data)->proto_name, text);
}

static void info_set_proto_filename(TFileVarProto *fv, const char *text)
{
	set_str(&((TtXfer *)fv->Comm->private_data)->file_name, text);
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

/* -------------------------------------------- symbols from outside ttpfile */

/*
 * Tera Term drives the protocols' one timer through a Win32 window timer.
 * Only zmodem arms it, and only on cancel (`zmodem.c:1586`): "finish 0.5 s
 * after sending the cancel sequence". Dropping it would leave a cancelled
 * transfer waiting for a peer that has already been told to stop.
 */
UINT_PTR SetTimer(HWND hWnd, UINT_PTR nIDEvent, UINT uElapse, void *lpTimerFunc)
{
	(void)hWnd; (void)lpTimerFunc;
	if (current != NULL)
		current->timer_at = now_sec() + uElapse / 1000.0;
	return nIDEvent;
}

BOOL KillTimer(HWND hWnd, UINT_PTR uIDEvent)
{
	(void)hWnd; (void)uIDEvent;
	if (current != NULL)
		current->timer_at = 0;
	return TRUE;
}

/* Called when the protocol has decided it cannot be driven any further —
 * always from a `!cv->Ready` branch, i.e. the connection went away and Parse
 * will not be called again. */
void ProtoEnd(void)
{
	if (current != NULL)
		current->state |= TT_XFER_STATE_ENDED;
}

static void sink_message(void *ctx, const char *caption, const char *text)
{
	TtXfer *x = ctx;
	char *joined;
	size_t n;

	if (x == NULL)
		return;
	n = strlen(caption ? caption : "") + strlen(text ? text : "") + 3;
	joined = malloc(n);
	if (joined == NULL)
		return;
	snprintf(joined, n, "%s%s%s", caption ? caption : "",
	         (caption && text) ? ": " : "", text ? text : "");
	free(x->message);
	x->message = joined;
}

int TTMessageBoxW(HWND hWnd, const TTMessageBoxInfoW *info,
                  const wchar_t *UILanguageFile, ...)
{
	char buf[512];
	(void)hWnd; (void)UILanguageFile;

	if (current != NULL && info != NULL && info->message_default != NULL) {
		/* The .lng lookup is the frontend's job; the default text is
		 * English and is what a headless caller has. */
		snprintf(buf, sizeof(buf), "%ls", info->message_default);
		sink_message(current, NULL, buf);
	}
	return IDOK;
}

/* ---------------------------------------------------------------- lifecycle */

static void apply_settings(TtXfer *x, const TtXferOpts *o)
{
	TTTSet *ts = &x->ts;

	memset(ts, 0, sizeof(*ts));
	ts->Baud = o->baud;
	ts->DataBit = o->data_bit_7 ? IdDataBit7 : IdDataBit8;

	ts->XmodemTimeOutInit = o->xmodem_timeout_init;
	ts->XmodemTimeOutInitCRC = o->xmodem_timeout_init_crc;
	ts->XmodemTimeOutShort = o->xmodem_timeout_short;
	ts->XmodemTimeOutLong = o->xmodem_timeout_long;
	ts->XmodemTimeOutVLong = o->xmodem_timeout_vlong;

	ts->YmodemTimeOutInit = o->ymodem_timeout_init;
	ts->YmodemTimeOutInitCRC = o->ymodem_timeout_init_crc;
	ts->YmodemTimeOutShort = o->ymodem_timeout_short;
	ts->YmodemTimeOutLong = o->ymodem_timeout_long;
	ts->YmodemTimeOutVLong = o->ymodem_timeout_vlong;

	ts->ZmodemTimeOutNormal = o->zmodem_timeout_normal;
	ts->ZmodemTimeOutTCPIP = o->zmodem_timeout_tcpip;
	ts->ZmodemTimeOutInit = o->zmodem_timeout_init;
	ts->ZmodemTimeOutFin = o->zmodem_timeout_fin;
	ts->ZmodemDataLen = o->zmodem_data_len;
	ts->ZmodemWinSize = o->zmodem_win_size;

	ts->QVWinSize = o->qv_win_size;
	ts->FTFlag = (WORD)o->ft_flag;
	ts->KermitOpt = (WORD)o->kermit_opt;
	ts->LogFlag = (WORD)o->log_flag;
	/* LogDirW is a `wchar_t *` that upstream points at long-lived storage
	 * (`GetLogDirW()`); protolog only ever passes it to SetFolderW, which
	 * copies. Ours is owned by the instance and freed with it. */
	free(x->log_dir_w);
	x->log_dir_w = NULL;
	if (o->log_dir != NULL) {
		size_t i, n = strlen(o->log_dir);
		x->log_dir_w = malloc((n + 1) * sizeof(wchar_t));
		if (x->log_dir_w != NULL) {
			/* A path out of the settings is UTF-8; widening byte by byte is
			 * right for ASCII and wrong above it, which is why this is the
			 * one place a real codeconv would be needed if log directories
			 * ever stop being ours to choose. */
			for (i = 0; i < n; i++)
				x->log_dir_w[i] = (wchar_t)(unsigned char)o->log_dir[i];
			x->log_dir_w[n] = L'\0';
		}
	}
	ts->LogDirW = x->log_dir_w;

	memset(&x->cv, 0, sizeof(x->cv));
	x->cv.Ready = TRUE;
	x->cv.PortType = (WORD)o->port_type;
}

TtXfer *tt_xfer_create(TtXferProtocol proto, TtXferDirection dir,
                       const TtXferOpts *opts)
{
	TtXfer *x;
	int sending = (dir == TT_XFER_SEND);

	if (opts == NULL)
		return NULL;
	x = calloc(1, sizeof(*x));
	if (x == NULL)
		return NULL;

	apply_settings(x, opts);

	x->file = tt_xfer_fileio_create();
	if (x->file == NULL) {
		free(x);
		return NULL;
	}

	x->comm.op = &comm_op;
	x->comm.private_data = x;

	x->fv.Comm = &x->comm;
	x->fv.file_fv = x->file;
	x->fv.InfoOp = &info_op;
	x->fv.GetNextFname = svc_get_next_fname;
	x->fv.GetRecievePath = svc_get_receive_path;   /* sic — upstream spelling */
	x->fv.FTSetTimeOut = svc_set_timeout;
	x->fv.SetDialogCation = svc_set_dialog_caption;
	x->fv.OverWrite = opts->overwrite ? TRUE : FALSE;
	x->fv.NoMsg = FALSE;

	current = x;
	switch (proto) {
	case TT_XFER_XMODEM:
		x->proto = XCreate(&x->fv);
		x->fv.OpId = sending ? OpXSend : OpXRcv;
		if (x->proto != NULL) {
			x->proto->Op->SetOpt(x->proto, XMODEM_MODE, opts->mode);
			x->proto->Op->SetOpt(x->proto, XMODEM_OPT, opts->opt);
			x->proto->Op->SetOpt(x->proto, XMODEM_TEXT_FLAG, opts->text_flag);
		}
		break;
	case TT_XFER_YMODEM:
		x->proto = YCreate(&x->fv);
		x->fv.OpId = sending ? OpYSend : OpYRcv;
		if (x->proto != NULL) {
			x->proto->Op->SetOpt(x->proto, YMODEM_MODE, opts->mode);
			x->proto->Op->SetOpt(x->proto, YMODEM_OPT, opts->opt);
		}
		break;
	case TT_XFER_ZMODEM:
		x->proto = ZCreate(&x->fv);
		x->fv.OpId = sending ? OpZSend : OpZRcv;
		if (x->proto != NULL) {
			x->proto->Op->SetOpt(x->proto, ZMODEM_MODE, opts->mode);
			x->proto->Op->SetOpt(x->proto, ZMODEM_BINFLAG, opts->opt);
		}
		break;
	case TT_XFER_KERMIT:
		x->proto = KmtCreate(&x->fv);
		x->fv.OpId = sending ? OpKmtSend : OpKmtRcv;
		if (x->proto != NULL)
			x->proto->Op->SetOpt(x->proto, KMT_MODE, opts->mode);
		break;
	case TT_XFER_BPLUS:
		x->proto = BPCreate(&x->fv);
		x->fv.OpId = sending ? OpBPSend : OpBPRcv;
		if (x->proto != NULL)
			x->proto->Op->SetOpt(x->proto, BPLUS_MODE, opts->mode);
		break;
	case TT_XFER_QUICKVAN:
		x->proto = QVCreate(&x->fv);
		x->fv.OpId = sending ? OpQVSend : OpQVRcv;
		if (x->proto != NULL)
			x->proto->Op->SetOpt(x->proto, QUICKVAN_MODE, opts->mode);
		break;
	case TT_XFER_RAW:
		x->proto = RawCreate(&x->fv);
		x->fv.OpId = OpRawRcv;
		if (x->proto != NULL)
			x->proto->Op->SetOpt(x->proto, RAW_AUTOSTOP_SEC, opts->autostop_sec);
		break;
	default:
		x->proto = NULL;
		break;
	}
	current = NULL;

	if (x->proto == NULL) {
		x->file->FileSysDestroy(x->file);
		free(x);
		return NULL;
	}
	x->fv.Proto = x->proto;
	return x;
}

void tt_xfer_destroy(TtXfer *x)
{
	int i;

	if (x == NULL)
		return;
	if (x->proto != NULL) {
		current = x;
		x->proto->Op->Destroy(x->proto);
		current = NULL;
	}
	if (x->file != NULL)
		x->file->FileSysDestroy(x->file);
	for (i = 0; i < x->send_count; i++)
		free(x->send_paths[i]);
	free(x->send_paths);
	free(x->recv_dir);
	free(x->proto_name);
	free(x->file_name);
	free(x->message);
	free(x->log_dir_w);
	free(x);
}

int tt_xfer_add_send_file(TtXfer *x, const char *pathU8)
{
	char **grown;

	if (x == NULL || pathU8 == NULL)
		return 0;
	grown = realloc(x->send_paths, sizeof(char *) * (size_t)(x->send_count + 1));
	if (grown == NULL)
		return 0;
	x->send_paths = grown;
	x->send_paths[x->send_count] = strdup(pathU8);
	if (x->send_paths[x->send_count] == NULL)
		return 0;
	x->send_count++;
	return 1;
}

int tt_xfer_set_recv_dir(TtXfer *x, const char *dirU8)
{
	size_t n;

	if (x == NULL || dirU8 == NULL)
		return 0;
	n = strlen(dirU8);
	free(x->recv_dir);
	x->recv_dir = malloc(n + 2);
	if (x->recv_dir == NULL)
		return 0;
	memcpy(x->recv_dir, dirU8, n + 1);
	/* The protocols concatenate path + filename with no separator between
	 * them, so the trailing one is part of the contract — filesys_proto.h
	 * says so of RecievePath: 終端にパスセパレータが付加されている. */
	if (n > 0 && x->recv_dir[n - 1] != '/') {
		x->recv_dir[n] = '/';
		x->recv_dir[n + 1] = '\0';
	}
	return 1;
}

int tt_xfer_init(TtXfer *x)
{
	int ok;

	if (x == NULL)
		return 0;
	current = x;
	winshim_set_message_sink(sink_message, x);
	x->start = now_sec();
	ok = x->proto->Op->Init(x->proto, &x->cv, &x->ts) ? 1 : 0;
	winshim_set_message_sink(NULL, NULL);
	current = NULL;
	return ok;
}

int tt_xfer_parse(TtXfer *x)
{
	int more;

	if (x == NULL)
		return 0;
	current = x;
	winshim_set_message_sink(sink_message, x);
	more = x->proto->Op->Parse(x->proto) ? 1 : 0;
	winshim_set_message_sink(NULL, NULL);
	current = NULL;

	if (!more) {
		x->state |= TT_XFER_STATE_DONE;
		/*
		 * Close the file here, not at Destroy.
		 *
		 * No protocol closes it on the receive path — XMODEM's EOT arm sets
		 * Success, ACKs and returns FALSE (`xmodem.c:444`), and `XDestroy`
		 * frees its state without touching the file. Upstream gets away with
		 * it because ProtoEnd tears the whole FileVar down a moment later; a
		 * library cannot, because the caller is entitled to report "done" and
		 * let the user open the file. With stdio buffering that means the
		 * last 4 KB is still in memory, so the file is short by up to a
		 * buffer — which looks exactly like a truncated transfer.
		 */
		x->file->Close(x->file);
	}
	return more;
}

void tt_xfer_timeout(TtXfer *x)
{
	if (x == NULL)
		return;
	current = x;
	winshim_set_message_sink(sink_message, x);
	x->deadline = 0;   /* the protocol re-arms if it wants more */
	x->proto->Op->TimeOutProc(x->proto);
	winshim_set_message_sink(NULL, NULL);
	current = NULL;
}

void tt_xfer_cancel(TtXfer *x)
{
	if (x == NULL)
		return;
	current = x;
	winshim_set_message_sink(sink_message, x);
	x->proto->Op->Cancel(x->proto);
	winshim_set_message_sink(NULL, NULL);
	current = NULL;
	x->state |= TT_XFER_STATE_CANCELLED;
}

unsigned tt_xfer_state(const TtXfer *x)
{
	unsigned state;

	if (x == NULL)
		return TT_XFER_STATE_DONE;
	state = x->state;
	/*
	 * Read Success live rather than latching it. It is a plain field the
	 * protocols assign at whatever point they consider the job done, and it
	 * is not monotonic: a YMODEM batch sets it per file. Latching the first
	 * TRUE would report a batch that failed on its second file as a success.
	 */
	if (x->fv.Success)
		state |= TT_XFER_STATE_SUCCESS;
	return state;
}

double tt_xfer_timeout_remaining(const TtXfer *x)
{
	double soonest = 0;

	if (x == NULL)
		return -1;
	if (x->deadline > 0)
		soonest = x->deadline;
	if (x->timer_at > 0 && (soonest == 0 || x->timer_at < soonest))
		soonest = x->timer_at;
	if (soonest == 0)
		return -1;
	return soonest - now_sec();
}

size_t tt_xfer_push_rx(TtXfer *x, const uint8_t *data, size_t len)
{
	size_t room;

	if (x == NULL || data == NULL || len == 0)
		return 0;

	/* PackInBuff (`ttcmn.c:742`): close the gap at the front before
	 * measuring the space at the back. */
	if (x->cv.InPtr > 0) {
		memmove(&x->cv.InBuff[0], &x->cv.InBuff[x->cv.InPtr],
		        (size_t)x->cv.InBuffCount);
		x->cv.InPtr = 0;
	}
	room = (size_t)(InBuffSize - x->cv.InBuffCount);
	if (len > room)
		len = room;
	memcpy(&x->cv.InBuff[x->cv.InBuffCount], data, len);
	x->cv.InBuffCount += (int)len;
	return len;
}

size_t tt_xfer_rx_pending(const TtXfer *x)
{
	return x != NULL ? (size_t)x->cv.InBuffCount : 0;
}

size_t tt_xfer_take_tx(TtXfer *x, uint8_t *out, size_t cap)
{
	size_t n;

	if (x == NULL || out == NULL)
		return 0;
	n = (size_t)x->cv.OutBuffCount;
	if (n > cap)
		n = cap;
	if (n == 0)
		return 0;
	memcpy(out, &x->cv.OutBuff[x->cv.OutPtr], n);
	x->cv.OutPtr += (int)n;
	x->cv.OutBuffCount -= (int)n;
	if (x->cv.OutBuffCount == 0)
		x->cv.OutPtr = 0;
	return n;
}

size_t tt_xfer_tx_pending(const TtXfer *x)
{
	return x != NULL ? (size_t)x->cv.OutBuffCount : 0;
}

void tt_xfer_set_ready(TtXfer *x, int ready)
{
	if (x != NULL)
		x->cv.Ready = ready ? TRUE : FALSE;
}

const TtXferProgress *tt_xfer_progress(const TtXfer *x)
{
	return x != NULL ? &x->prog : NULL;
}

const char *tt_xfer_proto_name(const TtXfer *x)
{
	return x != NULL ? x->proto_name : NULL;
}

const char *tt_xfer_file_name(const TtXfer *x)
{
	return x != NULL ? x->file_name : NULL;
}

const char *tt_xfer_message(const TtXfer *x)
{
	return x != NULL ? x->message : NULL;
}
