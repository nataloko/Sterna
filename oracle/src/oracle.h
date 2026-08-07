/*
 * termitta oracle — public interface of the harness around Tera Term's
 * vtterm.c / buffer.c.
 */
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Input feed. CommRead1Byte() serves bytes from here. */
void oracle_feed(const void *bytes, size_t len);
int  oracle_feed_remaining(void);

/* Everything the terminal sent back to the host, as UTF-8 bytes. */
const unsigned char *oracle_reply(size_t *len);
void oracle_reply_reset(void);

/* Last OSC 0/2 title, UTF-8. */
const char *oracle_title(void);

/* Deterministic clock, so bell throttling doesn't depend on wall time. */
void  oracle_clock_set_frozen(int frozen);
void  oracle_clock_advance(unsigned int ms);
unsigned int oracle_clock_now(void);

#ifdef __cplusplus
}
#endif
