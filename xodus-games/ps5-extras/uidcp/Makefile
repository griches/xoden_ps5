# uidcp - file commands as Playden's user. Build: limactl shell ps5 -- make -C <this folder>
PS5_PAYLOAD_SDK ?= $(HOME)/ps5-payload-sdk
include $(PS5_PAYLOAD_SDK)/toolchain/prospero.mk

CFLAGS := -std=gnu11 -O2 -Wall -Wextra -Werror

all: uidcp.elf

uidcp.elf: main.c
	$(CC) $(CFLAGS) -o $@ main.c

clean:
	rm -f uidcp.elf

.PHONY: all clean
