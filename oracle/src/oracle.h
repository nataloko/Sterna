/*
 * termitta oracle — public interface of the harness around Tera Term's
 * vtterm.c / buffer.c.
 */
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * The nominal character cell, in pixels. vtterm.c converts between window and
 * screen coordinates for every mouse report, so a headless oracle still needs
 * a cell size; 8x16 is arbitrary but fixed, and case files that inject mouse
 * events give their positions in these pixels.
 */
#define ORACLE_CELL_W 8
#define ORACLE_CELL_H 16

/* Input feed. CommRead1Byte() serves bytes from here. */
void oracle_feed(const void *bytes, size_t len);
int  oracle_feed_remaining(void);

/* Modifier keys, as ShiftKey()/ControlKey()/AltKey() report them. */
void oracle_set_modifiers(int shift, int control, int alt);

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
