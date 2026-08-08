# termitta xfer — Tera Term's real file-transfer protocols, running on Linux.
#
# Shares ../winshim: the Win32 surface these protocols need is a subset of
# what the VT engine already needed. See README.md.

TT      ?= ../../teraterm/teraterm
SHIM    ?= ../winshim
BUILD   ?= build

# Compiled UNMODIFIED, exactly as found in the Tera Term tree.
TT_C    := $(TT)/ttpfile/xmodem.c \
           $(TT)/ttpfile/ymodem.c \
           $(TT)/ttpfile/zmodem.c \
           $(TT)/ttpfile/kermit.c \
           $(TT)/ttpfile/bplus.c \
           $(TT)/ttpfile/quickvan.c \
           $(TT)/ttpfile/ftlib.c
TT_CXX  := $(TT)/ttpfile/protolog.cpp \
           $(TT)/common/asprintf.cpp

# protolog.cpp converts paths through Tera Term's codeconv; the shim already
# carries the minimal subset of that, so share it rather than write a second.
SHIM_C  := $(SHIM)/winshim.c $(SHIM)/msvc_crt.c $(SHIM)/swscanf_s.c \
           $(SHIM)/codeconv_min.c
OURS    := src/main.c src/comm_fd.c src/fileio_posix.c src/host.c

INCLUDES := -include $(SHIM)/msvc_compat.h -I$(SHIM) -Isrc \
            -I$(TT)/common -I$(TT)/teraterm -I$(TT)/ttpfile

# -w for Tera Term's own sources: someone else's code, another compiler.
CFLAGS   := -std=gnu99 -O1 -g $(INCLUDES) -w
CXXFLAGS := -O1 -g $(INCLUDES) -w
OURFLAGS := -std=gnu99 -O1 -g $(INCLUDES) -Wall -Wextra -Wno-unused-parameter

OBJS := $(addprefix $(BUILD)/,$(notdir $(TT_C:.c=.o))) \
        $(addprefix $(BUILD)/,$(notdir $(TT_CXX:.cpp=.o))) \
        $(addprefix $(BUILD)/,$(notdir $(SHIM_C:.c=.o))) \
        $(addprefix $(BUILD)/,$(notdir $(OURS:.c=.o)))

VPATH := $(TT)/ttpfile:$(TT)/common:$(SHIM):src

.PHONY: all clean test
all: $(BUILD)/xfer

$(BUILD):
	@mkdir -p $(BUILD)

# Our sources build with warnings on; Tera Term's do not. Order matters here:
# make picks the first matching rule, so the src/ rule must come first.
$(BUILD)/main.o $(BUILD)/comm_fd.o $(BUILD)/fileio_posix.o $(BUILD)/host.o: \
  $(BUILD)/%.o: src/%.c | $(BUILD)
	$(CC) -c -o $@ $< $(OURFLAGS)

$(BUILD)/%.o: %.c | $(BUILD)
	$(CC) -c -o $@ $< $(CFLAGS)

$(BUILD)/%.o: %.cpp | $(BUILD)
	$(CXX) -c -o $@ $< $(CXXFLAGS)

$(BUILD)/xfer: $(OBJS)
	$(CC) -o $@ $^ -lstdc++ -lm -lutil

test: $(BUILD)/xfer
	@./run_tests.sh

clean:
	rm -rf $(BUILD)
