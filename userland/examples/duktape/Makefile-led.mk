# Makefile for user application

# Specify this directory relative to the current application.
TOCK_USERLAND_BASE_DIR = ../..

# Which files to compile.
C_SRCS := duktape.c ledcontrol.c main-led.c

CFLAGS += -Os -DSTACK_SIZE=6134 -DAPP_HEAP_SIZE=32768

# Include userland master makefile. Contains rules and flags for actually
# building the application.
include $(TOCK_USERLAND_BASE_DIR)/Makefile

duk2: main.c duktape.c duk_config.h	duktape.h
	gcc -o $@ *.c -Os
