# termitta oracle — builds Tera Term's real VT engine headless on Linux.
#
# The point of this target is differential testing: feed the same bytes to
# this and to the Rust reimplementation, diff the grid dumps. See README.md.

TT      ?= ../../teraterm/teraterm
SHIMDIR ?= ../winshim
BUILD   ?= build

# Tera Term sources compiled UNMODIFIED. Adding to this list is always
# preferable to reimplementing behaviour in a stub. Empty since vtterm.c
# joined buffer.c in TT_PATCHED below; both are still compiled whole, with a
# handful of one-line fixes for defects reported upstream.
TT_C    :=
TT_CXX  := $(TT)/teraterm/charset.cpp \
           $(TT)/teraterm/unicode.cpp \
           $(TT)/teraterm/checkeol.cpp \
           $(TT)/common/ttlib_charset.cpp \
           $(TT)/common/asprintf.cpp \
           $(TT)/common/tttypes_termid.cpp \
           $(TT)/common/makeoutputstring.cpp

# Sources needing a local fix are copied to build/patched/ and patched there;
# the Tera Term tree itself is never modified. See patches/ and apply_patches.py.
TT_PATCHED := buffer.c vtterm.c

# ../winshim is shared with xfer/ and crates/tt-xfer/, which is why it is not
# under oracle/ any more: a shipped crate must not reach into the test oracle.
SHIM    := $(SHIMDIR)/winshim.c $(SHIMDIR)/msvc_crt.c $(SHIMDIR)/swscanf_s.c \
           $(SHIMDIR)/codeconv_min.c
HARNESS := src/stubs_manual.c src/keys.c src/stubs_generated.c src/main.c

INCLUDES := -include $(SHIMDIR)/msvc_compat.h -I$(SHIMDIR) -Isrc \
            -I$(TT)/common -I$(TT)/teraterm

# -w because these are someone else's sources built for a different compiler;
# our own code is built with warnings on below.
CFLAGS   := -std=gnu99 -O1 -g $(INCLUDES) -w
CXXFLAGS := -O1 -g $(INCLUDES) -w
OURFLAGS := -std=gnu99 -O1 -g $(INCLUDES) -Wall -Wextra -Wno-unused-parameter

OBJS := $(addprefix $(BUILD)/,$(notdir $(TT_C:.c=.o))) \
        $(addprefix $(BUILD)/,$(TT_PATCHED:.c=.o)) \
        $(addprefix $(BUILD)/,$(notdir $(TT_CXX:.cpp=.o))) \
        $(addprefix $(BUILD)/,$(notdir $(SHIM:.c=.o))) \
        $(addprefix $(BUILD)/,$(notdir $(HARNESS:.c=.o)))

VPATH := $(TT)/teraterm:$(TT)/common:$(SHIMDIR):src

.PHONY: all clean test stubs
all: $(BUILD)/oracle

$(BUILD):
	@mkdir -p $(BUILD)

# apply_patches.py is a prerequisite, not just the tool: the patch set lives
# inside it, so editing a patch must re-copy the source. Without this a new
# patch silently does nothing until someone deletes build/patched by hand.
$(BUILD)/patched/%.c: $(TT)/teraterm/%.c apply_patches.py | $(BUILD)
	@mkdir -p $(BUILD)/patched
	./apply_patches.py --src $< --out $@

# Explicit rules, because VPATH would otherwise let the generic %.o: %.c rule
# find the UNPATCHED source first and silently win.
define PATCHED_RULE
$(BUILD)/$(1:.c=.o): $(BUILD)/patched/$(1)
	$$(CC) -c -o $$@ $$< $$(CFLAGS)
endef
$(foreach f,$(TT_PATCHED),$(eval $(call PATCHED_RULE,$(f))))

$(BUILD)/%.o: %.c | $(BUILD)
	$(CC) -c -o $@ $< $(CFLAGS)

$(BUILD)/%.o: %.cpp | $(BUILD)
	$(CXX) -c -o $@ $< $(CXXFLAGS)

$(BUILD)/oracle: $(OBJS)
	$(CC) -o $@ $^ -lstdc++ -lm

# Regenerate the stub layer after Tera Term's headers change. Writes the
# still-missing symbol list to build/real.txt first.
stubs: $(BUILD)/oracle
	@echo "int main(void){return 0;}" > $(BUILD)/nullmain.c
	@$(CC) -o /dev/null $(BUILD)/nullmain.c \
	  $(filter-out $(BUILD)/main.o $(BUILD)/stubs_generated.o,$(OBJS)) \
	  -lstdc++ -lm 2>&1 \
	  | grep -oE "undefined reference to .[^']+'" \
	  | sed "s/.*\`//;s/'//" | sort -u > $(BUILD)/real.txt || true
	./gen_stubs.py --missing $(BUILD)/real.txt --out src/stubs_generated.c

test: $(BUILD)/oracle
	@./run_tests.sh

clean:
	rm -rf $(BUILD)
