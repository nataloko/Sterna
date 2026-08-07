/*
 * Driver for Tera Term's real file-transfer protocols on Linux.
 *
 *   xfer --proto z --send FILE...  --pty 'rz'      # spawn a peer on a pty
 *   xfer --proto z --recv DIR      --serial /dev/ttyUSB1
 *
 * The protocol model is: Create -> SetOpt* -> Init -> Parse until FALSE ->
 * Destroy, with bytes flowing through TComm and timeouts reported back by the
 * host. That is all filesys_proto.cpp does either, minus the dialogs.
 */
#define _GNU_SOURCE
#include "xfer.h"

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pty.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#include "tttypes.h"
#include "xmodem.h"
#include "ymodem.h"
#include "zmodem.h"
#include "kermit.h"
#include "bplus.h"
#include "quickvan.h"

TProto *ZCreate(TFileVarProto *fv);   /* zmodem.h omits this declaration */

/*
 * Mirrors ttpset/ttset.c's per-key fallbacks for the ~20 fields the protocols
 * actually read. Zeroing TTTSet instead would set every timeout to 0 and every
 * transfer would abort on its first wait — the same class of bug the oracle's
 * settings_defaults() exists to prevent. Values are ttset.c's, by line.
 */
static void settings_defaults(TTTSet *ts)
{
	memset(ts, 0, sizeof(*ts));

	ts->XmodemTimeOutInit    = 10;   /* ttset.c:1821 */
	ts->XmodemTimeOutInitCRC = 3;
	ts->XmodemTimeOutShort   = 10;
	ts->XmodemTimeOutLong    = 20;
	ts->XmodemTimeOutVLong   = 60;

	ts->YmodemTimeOutInit    = 10;
	ts->YmodemTimeOutInitCRC = 3;
	ts->YmodemTimeOutShort   = 10;
	ts->YmodemTimeOutLong    = 20;
	ts->YmodemTimeOutVLong   = 60;

	ts->ZmodemTimeOutNormal  = 10;
	ts->ZmodemTimeOutInit    = 10;
	ts->ZmodemTimeOutFin     = 3;
	ts->ZmodemTimeOutTCPIP   = 0;
	ts->ZmodemDataLen        = 1024;    /* ttset.c:1400 */
	ts->ZmodemWinSize        = 32767;   /* ttset.c:1403 */

	ts->QVWinSize            = 8;       /* ttset.c QVWinSize */
	ts->KermitOpt            = 0;       /* KmtLongPacket/KmtFileAttr off */
	ts->FTFlag               = 0;
	ts->LogFlag              = 0;

	ts->Baud                 = 115200;
	ts->DataBit              = IdDataBit8;
}

static void comvar_defaults(TComVar *cv)
{
	memset(cv, 0, sizeof(*cv));
	cv->Ready = TRUE;
	/* Serial rather than TCPIP: it selects the non-TCP timeout branches and
	 * disables the telnet escaping zmodem applies over a network link. */
	cv->PortType = IdSerial;
}

static void set_nonblocking(int fd)
{
	int fl = fcntl(fd, F_GETFL, 0);
	fcntl(fd, F_SETFL, fl | O_NONBLOCK);
}

/* Spawn a peer (sz/rz/gkermit) on a pty and return the master fd. */
static int spawn_pty_peer(const char *cmd, pid_t *out_pid)
{
	int master;
	struct termios tio;
	pid_t pid;

	memset(&tio, 0, sizeof(tio));
	tio.c_cflag = CS8 | CREAD | CLOCAL;
	cfmakeraw(&tio);   /* 8-bit clean: any cooking corrupts binary packets */

	pid = forkpty(&master, NULL, &tio, NULL);
	if (pid < 0) {
		perror("forkpty");
		return -1;
	}
	if (pid == 0) {
		execl("/bin/sh", "sh", "-c", cmd, (char *)NULL);
		_exit(127);
	}
	*out_pid = pid;
	set_nonblocking(master);
	return master;
}

static int open_serial(const char *dev, int baud)
{
	struct termios tio;
	speed_t sp;
	int fd = open(dev, O_RDWR | O_NOCTTY | O_NONBLOCK);

	if (fd < 0) {
		perror(dev);
		return -1;
	}
	switch (baud) {
	case 9600:   sp = B9600;   break;
	case 38400:  sp = B38400;  break;
	case 921600: sp = B921600; break;
	default:     sp = B115200; break;
	}
	memset(&tio, 0, sizeof(tio));
	cfmakeraw(&tio);
	tio.c_cflag |= CS8 | CREAD | CLOCAL;
	cfsetispeed(&tio, sp);
	cfsetospeed(&tio, sp);
	tcsetattr(fd, TCSANOW, &tio);
	tcflush(fd, TCIOFLUSH);
	return fd;
}

static double now_sec(void)
{
	struct timespec t;
	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec + t.tv_nsec / 1e9;
}

static void usage(void)
{
	fprintf(stderr,
	    "usage: xfer --proto x|y|z|kermit|bplus|quickvan\n"
	    "            --send FILE... | --recv DIR\n"
	    "            --pty 'CMD' | --serial DEV [--baud N] | --fd N\n"
    "            [--recv-name NAME]   # required by xmodem: no name on the wire\n"
	    "            [--limit SECONDS] [-v]\n");
}

int main(int argc, char **argv)
{
	const char *proto_name = NULL, *pty_cmd = NULL, *serial = NULL;
	const char *recv_dir = NULL, *recv_name = NULL;
	char **send_files = NULL;
	int send_count = 0, baud = 115200, fd = -1, limit = 60;
	int sending = 0;
	pid_t peer = -1;

	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--proto") && i + 1 < argc) {
			proto_name = argv[++i];
		} else if (!strcmp(argv[i], "--pty") && i + 1 < argc) {
			pty_cmd = argv[++i];
		} else if (!strcmp(argv[i], "--serial") && i + 1 < argc) {
			serial = argv[++i];
		} else if (!strcmp(argv[i], "--baud") && i + 1 < argc) {
			baud = atoi(argv[++i]);
		} else if (!strcmp(argv[i], "--fd") && i + 1 < argc) {
			fd = atoi(argv[++i]);
		} else if (!strcmp(argv[i], "--recv") && i + 1 < argc) {
			recv_dir = argv[++i];
		} else if (!strcmp(argv[i], "--recv-name") && i + 1 < argc) {
			recv_name = argv[++i];
		} else if (!strcmp(argv[i], "--limit") && i + 1 < argc) {
			limit = atoi(argv[++i]);
		} else if (!strcmp(argv[i], "-v")) {
			xfer_verbose = 1;
		} else if (!strcmp(argv[i], "--send")) {
			sending = 1;
			send_files = &argv[i + 1];
			while (i + 1 < argc && argv[i + 1][0] != '-')
				i++, send_count++;
		} else {
			usage();
			return 2;
		}
	}
	if (proto_name == NULL || (!sending && recv_dir == NULL)) {
		usage();
		return 2;
	}

	if (pty_cmd)
		fd = spawn_pty_peer(pty_cmd, &peer);
	else if (serial)
		fd = open_serial(serial, baud);
	else if (fd >= 0)
		set_nonblocking(fd);
	if (fd < 0) {
		usage();
		return 2;
	}

	TTTSet ts;
	TComVar cv;
	settings_defaults(&ts);
	comvar_defaults(&cv);

	TComm *comm = comm_fd_create(fd);
	TFileIO *file = fileio_posix_create();
	TFileVarProto *fv = filevar_create(comm, file);
	if (comm == NULL || file == NULL || fv == NULL) {
		fprintf(stderr, "out of memory\n");
		return 1;
	}
	/*
	 * XMODEM carries no filename, so on receive the destination comes from
	 * GetNextFname — in Tera Term that is what the receive dialog puts into
	 * FileNames[]. Protocols that do carry a name (y/z/kermit) must NOT get
	 * one this way, or they will use it instead of the peer's.
	 */
	static char *recv_target[1];
	char recv_path[4096];
	if (sending) {
		filevar_set_send_files(fv, send_files, send_count);
	} else {
		filevar_set_receive_dir(fv, recv_dir);
		if (recv_name != NULL) {
			snprintf(recv_path, sizeof(recv_path), "%s/%s", recv_dir, recv_name);
			recv_target[0] = recv_path;
			filevar_set_send_files(fv, recv_target, 1);
		}
	}

	TProto *proto = NULL;
	if (!strcmp(proto_name, "x")) {
		proto = XCreate(fv);
		fv->OpId = sending ? OpXSend : OpXRcv;
		proto->Op->SetOpt(proto, XMODEM_MODE, sending ? IdXSend : IdXReceive);
		proto->Op->SetOpt(proto, XMODEM_OPT, XoptCRC);
		proto->Op->SetOpt(proto, XMODEM_TEXT_FLAG, 0);
	} else if (!strcmp(proto_name, "y")) {
		proto = YCreate(fv);
		fv->OpId = sending ? OpYSend : OpYRcv;
		proto->Op->SetOpt(proto, YMODEM_MODE, sending ? IdXSend : IdXReceive);
		/* Tera Term hardcodes Yopt1K (filesys_proto.cpp:1409). Leaving this
		 * unset means YOpt==0, which falls through YSendPacket's switch to
		 * assert(0) — a crash, not a protocol error. */
		proto->Op->SetOpt(proto, YMODEM_OPT, Yopt1K);
	} else if (!strcmp(proto_name, "z")) {
		proto = ZCreate(fv);
		fv->OpId = sending ? OpZSend : OpZRcv;
		proto->Op->SetOpt(proto, ZMODEM_MODE, sending ? IdZSend : IdZReceive);
		proto->Op->SetOpt(proto, ZMODEM_BINFLAG, 1);
	} else if (!strcmp(proto_name, "kermit")) {
		proto = KmtCreate(fv);
		fv->OpId = sending ? OpKmtSend : OpKmtRcv;
		/* KMT_MODE takes KMT_MODE_T, NOT the OpId_t used for fv->OpId. The two
		 * enums overlap misleadingly: OpKmtRcv == 3 == IdKmtSend, so passing
		 * the OpId here tells kermit to send when you asked it to receive. */
		proto->Op->SetOpt(proto, KMT_MODE, sending ? IdKmtSend : IdKmtReceive);
	} else if (!strcmp(proto_name, "bplus")) {
		proto = BPCreate(fv);
		fv->OpId = sending ? OpBPSend : OpBPRcv;
		proto->Op->SetOpt(proto, BPLUS_MODE, sending ? IdBPSend : IdBPReceive);
	} else if (!strcmp(proto_name, "quickvan")) {
		proto = QVCreate(fv);
		fv->OpId = sending ? OpQVSend : OpQVRcv;
		proto->Op->SetOpt(proto, QUICKVAN_MODE, sending ? 2 : 1);
	} else {
		usage();
		return 2;
	}
	if (proto == NULL) {
		fprintf(stderr, "protocol create failed\n");
		return 1;
	}
	fv->Proto = proto;

	if (!proto->Op->Init(proto, &cv, &ts)) {
		fprintf(stderr, "protocol Init failed\n");
		return 1;
	}

	double start = now_sec();
	int timeouts = 0, rc = 0;

	for (;;) {
		struct pollfd pfd = { fd, POLLIN, 0 };
		poll(&pfd, 1, 50);

		if (!proto->Op->Parse(proto))
			break;

		/* FTSetTimeOut arms the deadline in host.c; when it elapses the
		 * protocol wants TimeOutProc, which is how it retries a NAK. */
		if (xfer_deadline > 0 && now_sec() > xfer_deadline) {
			proto->Op->TimeOutProc(proto);
			timeouts++;
			xfer_deadline = 0;   /* the protocol re-arms if it wants more */
		}
		if (now_sec() - start > limit) {
			fprintf(stderr, "wall-clock limit (%ds) hit\n", limit);
			rc = 3;
			break;
		}
	}

	printf("result: %s  in=%lu out=%lu  timeouts=%d  %.1fs\n",
	       fv->Success ? "SUCCESS" : "FAILED",
	       comm_fd_bytes_in(comm), comm_fd_bytes_out(comm),
	       timeouts, now_sec() - start);

	if (rc == 0)
		rc = fv->Success ? 0 : 1;

	proto->Op->Destroy(proto);
	file->FileSysDestroy(file);
	comm_fd_destroy(comm);
	filevar_destroy(fv);

	if (peer > 0) {
		int st;
		kill(peer, SIGTERM);
		waitpid(peer, &st, 0);
	}
	close(fd);
	return rc;
}
