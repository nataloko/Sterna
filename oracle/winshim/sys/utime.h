/* qtterm oracle: MSVC declares utime in <sys/utime.h>; POSIX uses <utime.h>. */
#pragma once
#include <utime.h>
struct _utimbuf { time_t actime; time_t modtime; };
