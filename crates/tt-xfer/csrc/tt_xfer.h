/*
 * The C surface `tt-xfer` drives Tera Term's protocols through.
 *
 * Deliberately narrow, and deliberately not a mirror of `filesys_proto.cpp`:
 * that file is the protocol lifecycle *plus* a dialog, a message pump and a
 * global. Here the lifecycle is all that crosses, and the loop that runs it
 * lives in Rust — so there is no callback from C into Rust anywhere, and
 * nothing to get wrong about unwinding across the boundary.
 *
 * Bytes move through two queues rather than a file descriptor. That is not a
 * simplification, it is the requirement: a transfer runs over the *terminal's
 * own connection*, so the same reader that feeds the VT engine has to be able
 * to hand its bytes here instead, and this code must not own the socket.
 */
#ifndef TT_XFER_H
#define TT_XFER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct TtXfer TtXfer;

/* Which protocol. The values are ours, not upstream's — upstream has no such
 * enum, only a Create function per protocol. */
typedef enum {
	TT_XFER_XMODEM = 0,
	TT_XFER_YMODEM = 1,
	TT_XFER_ZMODEM = 2,
	TT_XFER_KERMIT = 3,
	TT_XFER_BPLUS = 4,
	TT_XFER_QUICKVAN = 5,
	TT_XFER_RAW = 6,
} TtXferProtocol;

typedef enum {
	TT_XFER_SEND = 0,
	TT_XFER_RECEIVE = 1,
} TtXferDirection;

/*
 * Everything the protocols read out of TTTSet and TComVar, with upstream's
 * defaults cited on the Rust side. Passed whole rather than through setters
 * because Init() reads it once and never again: a value that arrives after
 * Init is a value that does nothing, silently.
 */
typedef struct {
	/* IdSerial (1) or IdTCPIP (2). Selects the timeout branch and, for
	 * zmodem, the maximum block size; kermit also uses it to decide whether
	 * 7-bit quoting is needed. Not cosmetic. */
	int port_type;
	int baud;      /* zmodem's block size ladder reads this. Serial only. */
	int data_bit_7;

	int xmodem_timeout_init, xmodem_timeout_init_crc;
	int xmodem_timeout_short, xmodem_timeout_long, xmodem_timeout_vlong;
	int ymodem_timeout_init, ymodem_timeout_init_crc;
	int ymodem_timeout_short, ymodem_timeout_long, ymodem_timeout_vlong;
	int zmodem_timeout_normal, zmodem_timeout_tcpip;
	int zmodem_timeout_init, zmodem_timeout_fin;
	int zmodem_data_len, zmodem_win_size;
	int qv_win_size;

	int ft_flag;      /* FT_ZESCCTL | FT_ZAUTO | FT_BPESCCTL | FT_BPAUTO | FT_RENAME */
	int kermit_opt;   /* KmtOptLongPacket | KmtOptFileAttr */
	int log_flag;     /* LOG_KMT | LOG_X | LOG_Z | LOG_BP | LOG_QV | LOG_Y */
	const char *log_dir;   /* UTF-8; NULL for the working directory */

	/* Per-protocol mode/option words, passed straight to SetOpt. Which
	 * fields matter depends on the protocol; the Rust side fills them. */
	int mode;         /* IdXSend/IdXReceive, IdZAutoR, IdBPAuto, ... */
	int opt;          /* XoptCRC, Yopt1K, ZMODEM_BINFLAG, ... */
	int text_flag;    /* XMODEM only */
	int autostop_sec; /* RAW only */

	int overwrite;    /* fv->OverWrite */
} TtXferOpts;

/*
 * What the protocol has told the progress dialog. Every field is something an
 * InfoOp entry point was called with; nothing here is computed.
 */
typedef struct {
	int64_t bytes;     /* SetDlgByteCount */
	int64_t packets;   /* SetDlgPacketNum */
	int64_t done;      /* SetDlgPercent's first argument */
	int64_t total;     /* ...and its second; 0 means "size unknown" */
	int32_t percent;   /* the high-water mark upstream keeps; -1 = no bar */
	uint32_t elapsed_ms;
} TtXferProgress;

/* Bits from tt_xfer_state(). */
#define TT_XFER_STATE_DONE       1u  /* Parse() returned FALSE */
#define TT_XFER_STATE_SUCCESS    2u  /* fv->Success, read live */
#define TT_XFER_STATE_ENDED      4u  /* the protocol called ProtoEnd() */
#define TT_XFER_STATE_CANCELLED  8u  /* tt_xfer_cancel was called */

TtXfer *tt_xfer_create(TtXferProtocol proto, TtXferDirection dir,
                       const TtXferOpts *opts);
void tt_xfer_destroy(TtXfer *x);

/* Both must be called before tt_xfer_init. Sending needs at least one file;
 * receiving needs a directory, and XMODEM additionally needs a file because
 * its wire format carries no name. */
int tt_xfer_add_send_file(TtXfer *x, const char *pathU8);
int tt_xfer_set_recv_dir(TtXfer *x, const char *dirU8);

/* Op->Init. Zero on failure, after which only destroy is legal. */
int tt_xfer_init(TtXfer *x);

/* Op->Parse once. Returns nonzero while the protocol wants to be called
 * again. Call it at least once even with no input: most protocols send their
 * opening packet from the first Parse. */
int tt_xfer_parse(TtXfer *x);

void tt_xfer_timeout(TtXfer *x);   /* Op->TimeOutProc */
void tt_xfer_cancel(TtXfer *x);    /* Op->Cancel */

unsigned tt_xfer_state(const TtXfer *x);

/*
 * Seconds until the armed timeout fires. **Zero means it is already due** and
 * negative means nothing is armed — a caller that reads an overdue deadline as
 * "nothing armed" sleeps for ever on a timeout that has already passed.
 *
 * FTSetTimeOut re-arms a deadline and is called with the *same* number on
 * every packet for exactly that reason. Reading it as a change-of-value
 * signal leaves a stale deadline that fires mid-transfer, which presents as a
 * flaky failure on large files rather than as a timeout bug. It cost a
 * one-in-three ymodem flake in `xfer/`.
 */
double tt_xfer_timeout_remaining(const TtXfer *x);

/* Bytes that arrived on the connection. Returns how many were taken: the
 * receive buffer is TComVar's own 64 KB and a caller with more than that must
 * come back for the rest. */
size_t tt_xfer_push_rx(TtXfer *x, const uint8_t *data, size_t len);
size_t tt_xfer_rx_pending(const TtXfer *x);

/* Bytes the protocol wants written to the connection. */
size_t tt_xfer_take_tx(TtXfer *x, uint8_t *out, size_t cap);
size_t tt_xfer_tx_pending(const TtXfer *x);

/* The far end went away. Clears cv->Ready, which is what the protocols test
 * before deciding they cannot finish. */
void tt_xfer_set_ready(TtXfer *x, int ready);

const TtXferProgress *tt_xfer_progress(const TtXfer *x);
const char *tt_xfer_proto_name(const TtXfer *x);   /* "ZMODEM", "Kermit", ... */
const char *tt_xfer_file_name(const TtXfer *x);    /* the file in flight */
const char *tt_xfer_message(const TtXfer *x);      /* the last MessageBox, or NULL */

#ifdef __cplusplus
}
#endif

#endif /* TT_XFER_H */
