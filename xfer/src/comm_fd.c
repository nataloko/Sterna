/*
 * TComm over a file descriptor.
 *
 * The contract the protocols rely on (see XReadPacket in xmodem.c, which loops
 * `for (c = Read1Byte(..); c > 0 && !GetPkt; ..)`) is that Read1Byte is
 * NON-BLOCKING and returns 0 when nothing is buffered. Block here and the
 * driver loop stalls instead of getting a chance to time out.
 */
#include "xfer.h"

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct {
	int fd;
	unsigned long in, out;

	/* Insert1Byte puts a byte at the FRONT of the outbound stream. Tera Term
	 * uses it to inject a flow-control or cancel byte ahead of queued data;
	 * we write it immediately, which preserves the ordering that matters. */
	unsigned char pushback[8];
	int pushback_n;
} CommFd;

static int comm_read1(TComm *comm, BYTE *b)
{
	CommFd *c = comm->private_data;

	if (c->pushback_n > 0) {
		*b = c->pushback[--c->pushback_n];
		return 1;
	}
	for (;;) {
		ssize_t n = read(c->fd, b, 1);
		if (n == 1) {
			c->in++;
			return 1;
		}
		if (n < 0 && errno == EINTR)
			continue;
		/* EAGAIN: nothing buffered. 0/EIO: peer gone — both mean
		 * "no byte for you", and the protocol's own timeout handles it. */
		return 0;
	}
}

static int comm_binary_out(TComm *comm, const CHAR *buf, size_t len)
{
	CommFd *c = comm->private_data;
	size_t sent = 0;

	while (sent < len) {
		ssize_t n = write(c->fd, buf + sent, len - sent);
		if (n > 0) {
			sent += (size_t)n;
			continue;
		}
		if (n < 0 && (errno == EINTR || errno == EAGAIN))
			continue;   /* pty/serial back-pressure; keep trying */
		break;
	}
	c->out += sent;
	return (int)sent;
}

static void comm_insert1(TComm *comm, BYTE b)
{
	CommFd *c = comm->private_data;
	CHAR ch = (CHAR)b;
	comm_binary_out(comm, &ch, 1);
}

static void comm_flush_recv(TComm *comm)
{
	CommFd *c = comm->private_data;
	unsigned char scratch[256];

	c->pushback_n = 0;
	for (;;) {
		ssize_t n = read(c->fd, scratch, sizeof(scratch));
		if (n <= 0)
			break;
		c->in += (unsigned long)n;
	}
}

static const CommOp comm_fd_op = {
	comm_binary_out,
	comm_read1,
	comm_insert1,
	comm_flush_recv,
};

TComm *comm_fd_create(int fd)
{
	CommFd *c = calloc(1, sizeof(*c));
	TComm *comm = calloc(1, sizeof(*comm));

	if (c == NULL || comm == NULL) {
		free(c);
		free(comm);
		return NULL;
	}
	c->fd = fd;
	comm->op = &comm_fd_op;
	comm->private_data = c;
	return comm;
}

void comm_fd_destroy(TComm *comm)
{
	if (comm == NULL)
		return;
	free(comm->private_data);
	free(comm);
}

unsigned long comm_fd_bytes_in(TComm *comm)
{
	return comm ? ((CommFd *)comm->private_data)->in : 0;
}

unsigned long comm_fd_bytes_out(TComm *comm)
{
	return comm ? ((CommFd *)comm->private_data)->out : 0;
}
