# Makefile for user application

# Specify this directory relative to the current application.
TOCK_USERLAND_BASE_DIR = ../..

# Which files to compile.
C_SRCS := duktape.c main-simple.c


#CFLAGS += -Os -DSTACK_SIZE=8192 -DAPP_HEAP_SIZE=16284
#CFLAGS += -Os -DSTACK_SIZE=1024 -DAPP_HEAP_SIZE=28000

# Use this one without debugging
#CFLAGS += -Os -DSTACK_SIZE=2048 -DAPP_HEAP_SIZE=27000

# Use this one with process.rs changed and debugging enabled
CFLAGS += -Os -DSTACK_SIZE=6134 -DAPP_HEAP_SIZE=32768


# Include userland master makefile. Contains rules and flags for actually
# building the application.
include $(TOCK_USERLAND_BASE_DIR)/Makefile

duk2: main.c duktape.c duk_config.h	duktape.h
	gcc -o $@ *.c -Os
