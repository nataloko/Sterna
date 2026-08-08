// DEC Special Graphics, byte to Unicode.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <cstdint>

/// Map a cell carrying `TT_ATTR_SPECIAL` to the character to draw.
///
/// The grid stores the *raw byte* for these, not a Unicode codepoint, because
/// `ts.DecSpMappingDir` defaults to "do not map" and so upstream's buffer holds
/// `q` with an attribute bit rather than U+2500. Turning it into a line is the
/// renderer's job, which is why the table lives here and not in the core.
///
/// The table is `vtterm.c:815`'s `dec2unicode` verbatim, including U+00A0 for
/// 0x5F where the DEC manual says blank. Bytes outside 0x5F..0x7E keep their
/// own value: upstream masks to 7 bits and leaves anything below the range
/// alone, so an `A` with the attribute set is still an `A`.
inline uint32_t decSpecialToUnicode(uint32_t code)
{
    static const uint16_t kMap[] = {
        0x00a0,                          // 0x5f
        0x25c6, 0x2592, 0x2409, 0x240c,  // 0x60 -
        0x240d, 0x240a, 0x00b0, 0x00b1,
        0x2424, 0x240b, 0x2518, 0x2510,
        0x250c, 0x2514, 0x253c, 0x23ba,  // 0x6c - 0x6f
        0x23bb, 0x2500, 0x23bc, 0x23bd,  // 0x70 -
        0x251c, 0x2524, 0x2534, 0x252c,
        0x2502, 0x2264, 0x2265, 0x03c0,
        0x2260, 0x00a3, 0x00b7,          // 0x7c - 0x7e
    };
    const uint32_t masked = code & 0x7F;
    if (masked >= 0x5F && masked <= 0x7E) {
        return kMap[masked - 0x5F];
    }
    return code;
}
