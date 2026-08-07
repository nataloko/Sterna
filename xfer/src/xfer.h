/*
 * termitta xfer — runs Tera Term's real file-transfer protocols on Linux.
 *
 * The protocols in ttpfile/ talk to the rest of Tera Term through three
 * vtables and nothing else:
 *
 *   TComm         BinaryOut / Read1Byte / Insert1Byte / FlashReceiveBuf
 *   TFileIO       14 file operations (filesys_io.h)
 *   TFileVarProto state + service callbacks, incl. the InfoOp progress vtable
 *
 * Implement those three and the protocol C runs unmodified. That is the whole
 * spike: this harness is the proof, and later becomes the FFI shape `tt-xfer`
 * exposes to the Rust core.
 */
#pragma once

/* tttypes.h first: filesys_proto.h uses PComVar/PTTSet without including it,
 * because in the real build tttypes.h always arrives ahead of it. */
#include "tttypes.h"
#include "filesys_proto.h"

/* Transport over a file descriptor: pty, socket, or a real serial port.
 * Read1Byte must be non-blocking — the protocols drain until it returns 0. */
TComm *comm_fd_create(int fd);
void comm_fd_destroy(TComm *comm);

/* True once the far end has gone away (EOF/EIO), as opposed to merely having
 * nothing buffered. The protocols cannot distinguish the two. */
int comm_fd_peer_closed(TComm *comm);

/* Bytes moved, for the summary line. */
unsigned long comm_fd_bytes_in(TComm *comm);
unsigned long comm_fd_bytes_out(TComm *comm);

/* POSIX TFileIO — the replacement for filesys_win32.cpp. */
TFileIO *fileio_posix_create(void);

/* The FileVarProto services and the InfoOp progress vtable. */
TFileVarProto *filevar_create(TComm *comm, TFileIO *file);
void filevar_destroy(TFileVarProto *fv);
void filevar_set_send_files(TFileVarProto *fv, char *const *paths, int count);
void filevar_set_receive_dir(TFileVarProto *fv, const char *dir);

/*
 * Protocol-requested timeout, as an absolute CLOCK_MONOTONIC deadline; 0 means
 * disarmed. It must be a deadline rather than a duration: protocols call
 * FTSetTimeOut with the SAME value on every packet in order to RE-ARM the
 * timer, so treating it as a value-change signal makes spurious timeouts fire
 * mid-transfer. That showed up as a ~1-in-3 flaky ymodem send.
 */
extern double xfer_deadline;
extern int    xfer_timer_ms;   /* from SetTimer, 0 when killed */
extern int    xfer_verbose;

double xfer_now(void);
